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

use super::text::{TextInner, TextLayoutInner};
use super::{DisplayBuilder, Error, ResultExt, SwitchToSwrast, Text, TextLayout};

use glutin::config::{Config, ConfigTemplateBuilder};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext, Version,
};
use glutin::display::{Display as GlutinDisplay, DisplayApiPreference};
use glutin::prelude::*;
use glutin::surface::{Surface as GlutinSurface, SurfaceAttributesBuilder, WindowSurface};

use glow::Context;
use piet::kurbo::{Point, Rect, Shape};
use piet::{RenderContext as _, StrokeStyle};
use piet_glow::GlContext;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

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
    scope: ContextScope<'dsp>,

    /// The piet-glow render context.
    inner: piet_glow::RenderContext<'dsp, Context>,

    /// The surface.
    surface: &'surf mut Surface,

    /// The text renderer.
    text: Text,

    /// Whether or not we need to check for the context being current.
    check_current: bool,

    /// The status from `check_current`.
    current_mismatch: Result<(), Error>,
}

type Brush = piet_glow::Brush<Context>;
type Image = piet_glow::Image<Context>;

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
            Some(hook) => DisplayApiPreference::GlxThenEgl(hook),
            None => DisplayApiPreference::Egl,
        };

        #[cfg(all(wgl_backend, egl_backend))]
        let _preference = DisplayApiPreference::EglThenWgl(builder.window);

        // Use the API preference to create the display.
        let display = GlutinDisplay::new(raw, _preference).piet_err()?;

        // Create a template for the config.
        let mut template_chooser = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(cfg!(target_vendor = "apple") || builder.transparent);

        if let Some(window) = builder.window {
            template_chooser = template_chooser.compatible_with_native_window(window);
        }

        let template = template_chooser.build();

        // Get the list of configs for the display.
        let config_list = display.find_configs(template).piet_err()?;

        // Get the config that matches our transparency support and has the most samples.
        let config = config_list
            .reduce(|accum, config| {
                let transparency_check = config.supports_transparency().unwrap_or(false)
                    & !accum.supports_transparency().unwrap_or(false);

                if transparency_check || config.num_samples() > accum.num_samples() {
                    config
                } else {
                    accum
                }
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

    pub(super) async unsafe fn make_surface(
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
    pub(super) unsafe fn new(
        display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        Self::new_impl(display, surface, width, height, true)
    }

    pub(super) unsafe fn new_unchecked(
        display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        Self::new_impl(display, surface, width, height, false)
    }

    unsafe fn new_impl(
        display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
        check_current: bool,
    ) -> Result<Self, Error> {
        let Display {
            context,
            renderer,
            display,
            ..
        } = display;

        // Make the context current.
        let not_current_context = context.take().unwrap();

        // TODO: Restore not_current_context if this call fails.
        let current_context = not_current_context
            .make_current(&surface.surface)
            .piet_err()?;
        let scope = ContextScope {
            slot: context,
            context: Some(current_context),
        };

        // Resize the surface.
        surface.surface.resize(
            scope.context(),
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );

        // Initialize the renderer if it hasn't been initialized yet.
        let renderer = match renderer {
            Some(ref mut renderer) => renderer,
            slot @ None => {
                // Create the new GlContext.
                // SAFETY: The context is current.
                slot.insert(unsafe {
                    let context = glow::Context::from_loader_function_cstr(|s| {
                        display.get_proc_address(s) as *const _
                    });

                    GlContext::new(context).piet_err()?
                })
            }
        };

        // Create a draw context on top of that.
        // SAFETY: The context is current.
        let mut draw_context = unsafe { renderer.render_context(width, height) };

        Ok(Self {
            scope,
            text: Text(TextInner::Glow(draw_context.text().clone())),
            inner: draw_context,
            surface,
            check_current,
            current_mismatch: Ok(()),
        })
    }

    #[inline]
    fn check_current(&self) -> Result<(), Error> {
        if self.check_current && !self.scope.context().is_current() {
            return Err(Error::BackendError("Context is not current".into()));
        }

        Ok(())
    }

    #[inline]
    fn not_current(&mut self) -> bool {
        match self.check_current() {
            Ok(()) => false,
            Err(e) => {
                self.current_mismatch = Err(e);
                true
            }
        }
    }

    pub(super) fn status(&mut self) -> Result<(), Error> {
        let status = self.inner.status();
        let mismatch = std::mem::replace(&mut self.current_mismatch, Ok(()));
        status.and(mismatch)
    }

    pub(super) fn solid_brush(&mut self, color: piet::Color) -> Brush {
        // SAFETY: This doesn't involve any GL for the time being, and probably won't ever.
        self.inner.solid_brush(color)
    }

    pub(super) fn gradient(&mut self, gradient: piet::FixedGradient) -> Result<Brush, Error> {
        self.check_current()?;
        self.inner.gradient(gradient)
    }

    pub(super) fn clear(&mut self, region: Option<Rect>, color: piet::Color) {
        if self.not_current() {
            return;
        }

        self.inner.clear(region, color)
    }

    pub(super) fn stroke(&mut self, shape: impl Shape, brush: &Brush, width: f64) {
        if self.not_current() {
            return;
        }
        self.inner.stroke(shape, brush, width)
    }

    pub(super) fn stroke_styled(
        &mut self,
        shape: impl Shape,
        brush: &Brush,
        width: f64,
        style: &StrokeStyle,
    ) {
        if self.not_current() {
            return;
        }
        self.inner.stroke_styled(shape, brush, width, style)
    }

    pub(super) fn fill(&mut self, shape: impl Shape, brush: &Brush) {
        if self.not_current() {
            return;
        }
        self.inner.fill(shape, brush)
    }

    pub(super) fn fill_even_odd(&mut self, shape: impl Shape, brush: &Brush) {
        if self.not_current() {
            return;
        }
        self.inner.fill_even_odd(shape, brush)
    }

    pub(super) fn clip(&mut self, shape: impl Shape) {
        if self.not_current() {
            return;
        }
        self.inner.clip(shape)
    }

    pub(super) fn text(&mut self) -> &mut Text {
        // SAFETY: Doesn't involve GL.
        &mut self.text
    }

    pub(super) fn draw_text(&mut self, layout: &TextLayout, pos: Point) {
        if self.not_current() {
            return;
        }
        let layout = match layout.0 {
            TextLayoutInner::Glow(ref layout) => layout,
            _ => {
                panic!("TextLayout was not created by this backend")
            }
        };
        self.inner.draw_text(layout, pos)
    }

    pub(super) fn save(&mut self) -> Result<(), Error> {
        self.check_current()?;
        self.inner.save()
    }

    pub(super) fn restore(&mut self) -> Result<(), Error> {
        self.check_current()?;
        self.inner.restore()
    }

    pub(super) fn finish(&mut self) -> Result<(), Error> {
        self.check_current()?;
        self.inner.finish()?;

        // Swap the buffers.
        // SAFETY: The context is current.
        self.surface
            .surface
            .swap_buffers(self.scope.context())
            .piet_err()?;

        Ok(())
    }

    pub(super) fn transform(&mut self, transform: piet::kurbo::Affine) {
        // SAFETY: Doesn't involve GL.
        self.inner.transform(transform)
    }

    pub(super) fn make_image(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        format: piet::ImageFormat,
    ) -> Result<Image, Error> {
        self.check_current()?;
        self.inner.make_image(width, height, buf, format)
    }

    pub(super) fn draw_image(
        &mut self,
        image: &Image,
        dst_rect: Rect,
        interp: piet::InterpolationMode,
    ) {
        if self.not_current() {
            return;
        }
        self.inner.draw_image(image, dst_rect, interp)
    }

    pub(super) fn draw_image_area(
        &mut self,
        image: &Image,
        src_rect: Rect,
        dst_rect: Rect,
        interp: piet::InterpolationMode,
    ) {
        if self.not_current() {
            return;
        }
        self.inner
            .draw_image_area(image, src_rect, dst_rect, interp)
    }

    pub(super) fn capture_image_area(&mut self, src_rect: Rect) -> Result<Image, Error> {
        self.check_current()?;
        self.inner.capture_image_area(src_rect)
    }

    pub(super) fn blurred_rect(&mut self, rect: Rect, blur_radius: f64, brush: &Brush) {
        if self.not_current() {
            return;
        }
        self.inner.blurred_rect(rect, blur_radius, brush)
    }

    pub(super) fn current_transform(&self) -> piet::kurbo::Affine {
        // SAFETY: Doesn't involve GL.
        self.inner.current_transform()
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
