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

//! The software rasterizer backend for `theo`.
//!
//! `tiny-skia` is used for rendering, `cosmic-text` is used to layout text, and `ab-glyph`
//! is used to rasterize glyphs.

use super::text::{Text, TextLayout};
use super::{DisplayBuilder, Error, ResultExt};

use piet::kurbo::{Affine, Rect, Shape, Size};
use piet::{FixedGradient, ImageFormat, InterpolationMode, IntoBrush, StrokeStyle};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use softbuffer::{Context, Surface as SoftbufferSurface};

use std::borrow::Cow;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;

/// The display for the software rasterizer.
pub(super) struct Display {
    /// The software rasterizer context.
    context: Context,
}

/// The surface for the software rasterizer.
pub(super) struct Surface {
    /// The software rasterizer surface.
    surface: SoftbufferSurface,

    /// The buffer that we use for rendering.
    buffer: Vec<u32>,
}

/// The rendering context for the software rasterizer.
pub(super) struct RenderContext<'dsp, 'surf> {
    /// The software rasterizer display.
    display: &'dsp mut Display,

    /// The software rasterizer surface.
    surface: &'surf mut Surface,

    /// The current size of the surface.
    size: (u32, u32),
}

/// The brush used for the software rasterizer.
#[derive(Clone)]
pub(super) enum Brush {
    /// A solid color brush.
    Solid(piet::Color),
    // TODO: Other variants
}

/// The image used for the software rasterizer.
#[derive(Clone)]
pub(super) struct Image(Rc<tiny_skia::Pixmap>);

impl Display {
    pub(super) unsafe fn new(
        _builder: &mut DisplayBuilder,
        raw: RawDisplayHandle,
    ) -> Result<Self, Error> {
        let context = Context::from_raw(raw).piet_err()?;
        Ok(Self { context })
    }

    pub(super) unsafe fn make_surface(
        &mut self,
        raw: RawWindowHandle,
        _width: u32,
        _height: u32,
    ) -> Result<Surface, Error> {
        let surface = SoftbufferSurface::from_raw(&self.context, raw).piet_err()?;

        Ok(Surface {
            surface,
            buffer: Vec::new(),
        })
    }

    pub(super) fn supports_transparency(&self) -> bool {
        false
    }

    pub(super) fn x11_visual(&self) -> Option<NonNull<()>> {
        None
    }
}

impl<'dsp, 'surf> RenderContext<'dsp, 'surf> {
    pub(super) fn new(
        display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        // Resize the buffer.
        let len = (width * height) as usize;
        surface.buffer.resize(len, 0);

        Ok(Self {
            display,
            surface,
            size: (width, height),
        })
    }
}

impl piet::RenderContext for RenderContext<'_, '_> {
    type Brush = Brush;
    type Image = Image;
    type Text = Text;
    type TextLayout = TextLayout;

    fn status(&mut self) -> Result<(), Error> {
        todo!()
    }

    fn solid_brush(&mut self, color: piet::Color) -> Self::Brush {
        todo!()
    }

    fn gradient(&mut self, gradient: impl Into<FixedGradient>) -> Result<Self::Brush, Error> {
        todo!()
    }

    fn clear(&mut self, region: impl Into<Option<Rect>>, color: piet::Color) {
        todo!()
    }

    fn stroke(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>, width: f64) {
        todo!()
    }

    fn stroke_styled(
        &mut self,
        shape: impl Shape,
        brush: &impl IntoBrush<Self>,
        width: f64,
        style: &StrokeStyle,
    ) {
        todo!()
    }

    fn fill(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
        todo!()
    }

    fn fill_even_odd(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
        todo!()
    }

    fn clip(&mut self, shape: impl Shape) {
        todo!()
    }

    fn text(&mut self) -> &mut Self::Text {
        todo!()
    }

    fn draw_text(&mut self, layout: &Self::TextLayout, pos: impl Into<piet::kurbo::Point>) {
        todo!()
    }

    fn save(&mut self) -> Result<(), Error> {
        todo!()
    }

    fn restore(&mut self) -> Result<(), Error> {
        todo!()
    }

    fn finish(&mut self) -> Result<(), Error> {
        todo!()
    }

    fn transform(&mut self, transform: Affine) {
        todo!()
    }

    fn make_image(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        format: ImageFormat,
    ) -> Result<Self::Image, Error> {
        todo!()
    }

    fn draw_image(
        &mut self,
        image: &Self::Image,
        dst_rect: impl Into<Rect>,
        interp: InterpolationMode,
    ) {
        todo!()
    }

    fn draw_image_area(
        &mut self,
        image: &Self::Image,
        src_rect: impl Into<Rect>,
        dst_rect: impl Into<Rect>,
        interp: InterpolationMode,
    ) {
        todo!()
    }

    fn capture_image_area(&mut self, src_rect: impl Into<Rect>) -> Result<Self::Image, Error> {
        todo!()
    }

    fn blurred_rect(&mut self, rect: Rect, blur_radius: f64, brush: &impl IntoBrush<Self>) {
        todo!()
    }

    fn current_transform(&self) -> Affine {
        todo!()
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

impl piet::Image for Image {
    fn size(&self) -> Size {
        let width = self.0.width() as f64;
        let height = self.0.height() as f64;

        Size::new(width, height)
    }
}
