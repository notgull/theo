// SPDX-License-Identifier: LGPL-3.0-or-later OR MPL-2.0
// This file is a part of `theo`.
//
// `theo` is free software: you can redistribute it and/or modify it under the terms of
// either:
//
// * GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
// * Mozilla Public License as published by the Mozilla Foundation, version 2.
//
// `theo` is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License or the Mozilla Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License and the Mozilla
// Public License along with `theo`. If not, see <https://www.gnu.org/licenses/>.

//! The GL-based hardware-accelerated backend for `theo`.
//!
//! We use `piet-glow` as the main rendering backend, and `glutin` to set up the `glow`
//! context.

use super::{DisplayBuilder, Error, ResultExt, Text, TextLayout};

use glutin::config::{Config, ConfigTemplateBuilder};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext, Version,
};
use glutin::display::{Display as GlutinDisplay, DisplayApiPreference};
use glutin::prelude::*;
use glutin::surface::{Surface as GlutinSurface, SurfaceAttributesBuilder, WindowSurface};

use glow::Context;
use piet::kurbo::{Rect, Shape};
use piet::{IntoBrush, StrokeStyle};
use piet_glow::GlContext;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroU32;
use std::ptr::NonNull;

/// The display for the GL backend.
pub(super) struct Display {
    /// The `glutin` display.
    display: GlutinDisplay,

    /// The `GlConfig` that we are using.
    config: Config,

    /// The GL context, but not current.
    ///
    /// This is taken by the `RenderContext` to be made current.
    context: Option<NotCurrentContext>,

    /// The cached OpenGL context.
    renderer: Option<GlContext<Context>>,
}

/// The surface for the GL backend.
pub(super) struct Surface {
    /// The `glutin` window.
    surface: GlutinSurface<WindowSurface>,
}

/// The rendering context for the GL backend.
///
/// This is a wrapper around the `piet-glow` render context that makes the context
/// not current when it is dropped.
pub(super) struct RenderContext<'dsp, 'surf> {
    /// The scope object that makes the context not current when it is dropped.
    _scope: ContextScope<'dsp>,

    /// The piet-glow render context.
    inner: piet_glow::RenderContext<'dsp, Context>,

    /// The surface.
    surface: &'surf mut Surface,

    /// The text renderer.
    text: Text,
}

/// The brush for the GL backend.
#[derive(Clone)]
pub(super) struct Brush(piet_glow::Brush<Context>);

impl Display {
    pub(super) unsafe fn new(
        builder: &mut DisplayBuilder,
        raw: RawDisplayHandle,
    ) -> Result<Self, Error> {
        if builder.force_swrast {
            return Err(Error::BackendError(SwitchToSwrast.into()));
        }

        // Get the API preference to use.
        #[cfg(egl_backend)]
        let _preference = DisplayApiPreference::Egl;

        #[cfg(cgl_backend)]
        let _preference = DisplayApiPreference::Cgl;

        #[cfg(wgl_backend)]
        let _preference = DisplayApiPreference::Wgl(builder.window);

        #[cfg(all(glx_backend, not(egl_backend)))]
        let _preference = match builder.glx_error_hook.take() {
            Some(hook) => DisplayApiPreference::Glx(hook),
            None => {
                return Err(Error::BackendError(
                    "GLX error hook not set, enable the egl feature to avoid this error".into(),
                ))
            }
        };

        #[cfg(all(glx_backend, egl_backend))]
        let _preference = match builder.glx_error_hook.take() {
            Some(hook) => DisplayApiPreference::EglThenGlx(hook),
            None => DisplayApiPreference::Egl,
        };

        #[cfg(all(wgl_backend, egl_backend))]
        let _preference = DisplayApiPreference::EglThenWgl(builder.window);

        // Use the API preference to create the display.
        let display = GlutinDisplay::new(raw, _preference).piet_err()?;

        // Create a template for the config.
        let mut template_chooser = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(builder.transparent);

        if let Some(window) = builder.window {
            template_chooser = template_chooser.compatible_with_native_window(window);
        }

        let template = template_chooser.build();

        // Get the list of configs for the display.
        let config_list = display.find_configs(template).piet_err()?;

        // Get the config that matches our transparency support and has the most samples.
        let config = config_list
            .max_by_key(|config| {
                let mut score = config.num_samples() as u16;

                if config.supports_transparency() == Some(builder.transparent) {
                    score += 1_000;
                }

                score
            })
            .ok_or_else(|| Error::BackendError("No matching configs found".into()))?;

        // Try to create a relatively modern context.
        let modern_context = ContextAttributesBuilder::new().build(builder.window);

        // Fall back to a GLES context if we can't get a modern context.
        let gles_context = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(builder.window);

        // Fall back to a slightly older context if we can't get a GLES context.
        let old_context = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(builder.window);

        let contexts = [modern_context, gles_context, old_context];

        // Try contexts until one works.
        let context = (|| {
            let mut last_error = None;

            for context in &contexts {
                match display.create_context(&config, context) {
                    Ok(context) => return Ok(context),
                    Err(err) => last_error = Some(err),
                }
            }

            Err(last_error.unwrap())
        })()
        .piet_err()?;

        Ok(Self {
            display,
            config,
            context: Some(context),
            renderer: None,
        })
    }

    pub(super) fn supports_transparency(&self) -> bool {
        self.config.supports_transparency().unwrap_or(false)
    }

    pub(super) fn x11_visual(&self) -> Option<NonNull<()>> {
        #[cfg(x11_platform)]
        {
            use glutin::platform::x11::X11GlConfigExt;
            return self
                .config
                .x11_visual()
                .and_then(|x| NonNull::new(x.into_raw() as *mut _));
        }

        #[allow(unreachable_code)]
        None
    }

    pub(super) unsafe fn make_surface(
        &mut self,
        raw: RawWindowHandle,
        width: u32,
        height: u32,
    ) -> Result<Surface, Error> {
        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );

        let surface = self
            .display
            .create_window_surface(&self.config, &attrs)
            .piet_err()?;

        Ok(Surface { surface })
    }
}

impl<'dsp, 'surf> RenderContext<'dsp, 'surf> {
    pub(super) fn new(
        display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        todo!()
    }
}

impl piet::RenderContext for RenderContext<'_, '_> {
    type Brush = Brush;
    type Image = piet_glow::Image<Context>;
    type Text = Text;
    type TextLayout = TextLayout;

    fn status(&mut self) -> Result<(), Error> {
        self.inner.status()
    }

    fn solid_brush(&mut self, color: piet::Color) -> Self::Brush {
        Brush(self.inner.solid_brush(color))
    }

    fn gradient(&mut self, gradient: impl Into<piet::FixedGradient>) -> Result<Self::Brush, Error> {
        self.inner.gradient(gradient).map(Brush)
    }

    fn clear(&mut self, region: impl Into<Option<Rect>>, color: piet::Color) {
        self.inner.clear(region, color)
    }

    fn stroke(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>, width: f64) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.inner.stroke(shape, &brush.as_ref().0, width)
    }

    fn stroke_styled(
        &mut self,
        shape: impl Shape,
        brush: &impl IntoBrush<Self>,
        width: f64,
        style: &StrokeStyle,
    ) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.inner
            .stroke_styled(shape, &brush.as_ref().0, width, style)
    }

    fn fill(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.inner.fill(shape, &brush.as_ref().0)
    }

    fn fill_even_odd(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.inner.fill_even_odd(shape, &brush.as_ref().0)
    }

    fn clip(&mut self, shape: impl Shape) {
        self.inner.clip(shape)
    }

    fn text(&mut self) -> &mut Self::Text {
        &mut self.text
    }

    fn draw_text(&mut self, layout: &Self::TextLayout, pos: impl Into<piet::kurbo::Point>) {
        self.inner.draw_text(&layout.0, pos)
    }

    fn save(&mut self) -> Result<(), Error> {
        self.inner.save()
    }

    fn restore(&mut self) -> Result<(), Error> {
        self.inner.restore()
    }

    fn finish(&mut self) -> Result<(), Error> {
        self.inner.finish()
    }

    fn transform(&mut self, transform: piet::kurbo::Affine) {
        self.inner.transform(transform)
    }

    fn make_image(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        format: piet::ImageFormat,
    ) -> Result<Self::Image, Error> {
        self.inner.make_image(width, height, buf, format)
    }

    fn draw_image(
        &mut self,
        image: &Self::Image,
        dst_rect: impl Into<Rect>,
        interp: piet::InterpolationMode,
    ) {
        self.inner.draw_image(image, dst_rect, interp)
    }

    fn draw_image_area(
        &mut self,
        image: &Self::Image,
        src_rect: impl Into<Rect>,
        dst_rect: impl Into<Rect>,
        interp: piet::InterpolationMode,
    ) {
        self.inner
            .draw_image_area(image, src_rect, dst_rect, interp)
    }

    fn capture_image_area(&mut self, src_rect: impl Into<Rect>) -> Result<Self::Image, Error> {
        self.inner.capture_image_area(src_rect)
    }

    fn blurred_rect(&mut self, rect: Rect, blur_radius: f64, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || rect);
        self.inner
            .blurred_rect(rect, blur_radius, &brush.as_ref().0)
    }

    fn current_transform(&self) -> piet::kurbo::Affine {
        self.inner.current_transform()
    }
}

impl IntoBrush<RenderContext<'_, '_>> for Brush {
    fn make_brush<'a>(
        &'a self,
        piet: &mut RenderContext<'_, '_>,
        bbox: impl FnOnce() -> Rect,
    ) -> Cow<'a, <RenderContext<'_, '_> as piet::RenderContext>::Brush> {
        Cow::Borrowed(self)
    }
}

struct ContextScope<'a> {
    /// The display we're borrowing from.
    slot: &'a mut Option<NotCurrentContext>,

    /// The context we're borrowing.
    context: Option<PossiblyCurrentContext>,
}

impl ContextScope<'_> {
    fn context(&self) -> &PossiblyCurrentContext {
        self.context.as_ref().unwrap()
    }

    fn context_mut(&mut self) -> &mut PossiblyCurrentContext {
        self.context.as_mut().unwrap()
    }
}

impl Drop for ContextScope<'_> {
    fn drop(&mut self) {
        let context = self.context.take().unwrap();

        *self.slot = Some(
            context
                .make_not_current()
                .expect("Failed to make context not current"),
        );
    }
}

#[derive(Debug)]
struct SwitchToSwrast;

impl fmt::Display for SwitchToSwrast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Switching to software rendering, this may cause lower performance"
        )
    }
}

impl std::error::Error for SwitchToSwrast {}
