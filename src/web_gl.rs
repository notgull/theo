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

use std::marker::PhantomData;

use crate::{text::Text, DisplayBuilder, Error, OptionExt, SwitchToSwrast};

use piet::kurbo::{Point, Rect, Shape};
use piet::{RenderContext as _, StrokeStyle};
use piet_glow::GlContext;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use wasm_bindgen::JsCast;
use web_sys::Document;

/// The display for the WebGL backend.
pub(crate) struct Display {
    /// Cache the document for later use.
    document: Document,

    /// Allow the use of transparency.
    transparency: bool,
}

/// The window for the WebGL backend.
pub(crate) struct Surface {
    /// The OpenGL context.
    context: GlContext<glow::Context>,
}

/// The render context for the WebGL backend.
pub(crate) struct RenderContext<'dsp, 'surf> {
    /// The inner context.
    inner: piet_glow::RenderContext<'surf, glow::Context>,

    /// Text data.
    text: Text,

    /// Eat the display lifetime.
    _display: PhantomData<&'dsp mut Display>,
}

impl Display {
    pub(super) unsafe fn new(
        builder: &mut DisplayBuilder,
        raw: RawDisplayHandle,
    ) -> Result<Self, Error> {
        if builder.force_swrast {
            return Err(Error::BackendError(SwitchToSwrast.into()));
        }

        // Make sure this is a raw web handle.
        match raw {
            RawDisplayHandle::Web(..) => {}
            _ => return Err(Error::NotSupported),
        }

        // Load the document.
        let document = web_sys::window()
            .and_then(|window| window.document())
            .piet_err("Failed to load document")?;

        Ok(Self {
            document,
            transparency: builder.transparent,
        })
    }

    pub(super) fn supports_transparency(&self) -> bool {
        self.transparency
    }

    pub(super) fn x11_visual(&self) -> Option<std::ptr::NonNull<()>> {
        None
    }

    pub(super) async unsafe fn make_surface(
        &mut self,
        raw: RawWindowHandle,
        _width: u32,
        _height: u32,
    ) -> Result<Surface, Error> {
        // Get the canvas ID.
        let id = match raw {
            RawWindowHandle::Web(web) => web.id,
            _ => return Err(Error::NotSupported),
        };

        // Load the canvas.
        let canvas = self
            .document
            .query_selector(&format!("canvas[data-raw-handle=\"{id}\"]"))
            .map_err(|_| Error::InvalidInput)?
            .piet_err(format!("Failed to load canvas with id {id}"))?
            .unchecked_into::<web_sys::HtmlCanvasElement>();

        // Try to get a WebGL2 context.
        if let Some(webgl_ctx) = canvas
            .get_context("webgl2")
            .map_err(|_| Error::BackendError("Failed to get WebGL2 context".into()))?
            .and_then(|ctx| ctx.dyn_into::<web_sys::WebGl2RenderingContext>().ok())
        {
            // Create the context.
            let glow_ctx = glow::Context::from_webgl2_context(webgl_ctx);

            // Use the context.
            Ok(Surface {
                context: unsafe { GlContext::new(glow_ctx)? },
            })
        } else {
            // Create a WebGL1 context instead.
            let webgl_ctx = canvas
                .get_context("webgl")
                .map_err(|_| Error::BackendError("Failed to get WebGL context".into()))?
                .and_then(|ctx| ctx.dyn_into::<web_sys::WebGlRenderingContext>().ok())
                .piet_err("Failed to get WebGL context")?;

            // Create the context.
            let glow_ctx = glow::Context::from_webgl1_context(webgl_ctx);

            // Use the context.
            Ok(Surface {
                context: unsafe { GlContext::new(glow_ctx)? },
            })
        }
    }

    pub(super) async fn present(&mut self) {
        // no-op
    }
}

impl<'dsp, 'surf> RenderContext<'dsp, 'surf> {
    pub(super) unsafe fn new(
        _display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        let mut ctx = unsafe { surface.context.render_context(width, height) };
        Ok(Self {
            text: Text(crate::text::TextInner::Glow(ctx.text().clone())),
            inner: ctx,
            _display: PhantomData,
        })
    }

    pub(super) unsafe fn new_unchecked(
        display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        Self::new(display, surface, width, height)
    }

    pub(super) fn status(&mut self) -> Result<(), Error> {
        self.inner.status()
    }

    pub(super) fn solid_brush(&mut self, color: piet::Color) -> Brush {
        self.inner.solid_brush(color)
    }

    pub(super) fn gradient(&mut self, gradient: piet::FixedGradient) -> Result<Brush, Error> {
        self.inner.gradient(gradient)
    }

    pub(super) fn clear(&mut self, region: Option<Rect>, color: piet::Color) {
        self.inner.clear(region, color)
    }

    pub(super) fn stroke(&mut self, shape: impl Shape, brush: &Brush, width: f64) {
        self.inner.stroke(shape, brush, width)
    }

    pub(super) fn stroke_styled(
        &mut self,
        shape: impl Shape,
        brush: &Brush,
        width: f64,
        style: &StrokeStyle,
    ) {
        self.inner.stroke_styled(shape, brush, width, style)
    }

    pub(super) fn fill(&mut self, shape: impl Shape, brush: &Brush) {
        self.inner.fill(shape, brush)
    }

    pub(super) fn fill_even_odd(&mut self, shape: impl Shape, brush: &Brush) {
        self.inner.fill_even_odd(shape, brush)
    }

    pub(super) fn clip(&mut self, shape: impl Shape) {
        self.inner.clip(shape)
    }

    pub(super) fn text(&mut self) -> &mut Text {
        &mut self.text
    }

    pub(super) fn draw_text(&mut self, layout: &crate::text::TextLayout, pos: Point) {
        match layout.0 {
            crate::text::TextLayoutInner::Glow(ref layout) => self.inner.draw_text(layout, pos),

            _ => panic!("invalid text layout"),
        }
    }

    pub(super) fn save(&mut self) -> Result<(), Error> {
        self.inner.save()
    }

    pub(super) fn restore(&mut self) -> Result<(), Error> {
        self.inner.restore()
    }

    pub(super) fn finish(&mut self) -> Result<(), Error> {
        self.inner.finish()
    }

    pub(super) fn transform(&mut self, transform: piet::kurbo::Affine) {
        self.inner.transform(transform)
    }

    pub(super) fn make_image(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        format: piet::ImageFormat,
    ) -> Result<Image, Error> {
        self.inner.make_image(width, height, buf, format)
    }

    pub(super) fn draw_image(
        &mut self,
        image: &Image,
        rect: Rect,
        interp: piet::InterpolationMode,
    ) {
        self.inner.draw_image(image, rect, interp)
    }

    pub(super) fn draw_image_area(
        &mut self,
        image: &Image,
        src_rect: Rect,
        dst_rect: Rect,
        interp: piet::InterpolationMode,
    ) {
        self.inner
            .draw_image_area(image, src_rect, dst_rect, interp)
    }

    pub(super) fn capture_image_area(&mut self, src_rect: Rect) -> Result<Image, Error> {
        self.inner.capture_image_area(src_rect)
    }

    pub(super) fn blurred_rect(&mut self, rect: Rect, blur_radius: f64, brush: &Brush) {
        self.inner.blurred_rect(rect, blur_radius, brush)
    }

    pub(super) fn current_transform(&self) -> piet::kurbo::Affine {
        self.inner.current_transform()
    }
}

type Image = piet_glow::Image<glow::Context>;
type Brush = piet_glow::Brush<glow::Context>;
