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

use crate::text::TextLayoutInner;

use super::text::{Text, TextLayout};
use super::{DisplayBuilder, Error};

use softbuffer as sb;

use piet::kurbo::{Affine, Point, Rect, Shape};
use piet::{FixedGradient, ImageFormat, InterpolationMode, RenderContext as _, StrokeStyle};

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use tiny_skia::PixmapMut;

use std::mem;
use std::num::NonZeroU32;
use std::ptr::NonNull;

/// The display for the software rasterizer.
pub(super) struct Display {
    /// The root display for the backend.
    root: sb::Context,

    /// `piet-tiny-skia`-specific rendering information.
    cache: piet_tiny_skia::Cache,
}

/// The surface for the software rasterizer.
pub(super) struct Surface {
    /// The software rasterizer surface.
    surface: sb::Surface,
}

/// The rendering context for the software rasterizer.
pub(super) struct RenderContext<'dsp, 'surf> {
    /// The underlying context.
    inner: Option<piet_tiny_skia::RenderContext<'dsp, Buffer<'surf>>>,

    /// The text interface.
    text: Text,

    /// Whether we currently need to update the render state.
    dirty: bool,

    /// Error from mismatched type usages.
    mismatch_err: Result<(), piet::Error>,
}

struct Buffer<'a> {
    buffer: sb::Buffer<'a>,
    width: u32,
    height: u32,
}

impl piet_tiny_skia::AsPixmapMut for Buffer<'_> {
    fn as_pixmap_mut(&mut self) -> PixmapMut<'_> {
        let (width, height) = (self.width, self.height);
        PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut self.buffer), width, height)
            .expect("This should never fail")
    }
}

pub(crate) type Brush = piet_tiny_skia::Brush;
pub(crate) type Image = piet_tiny_skia::Image;

impl Display {
    pub(super) unsafe fn new(
        _builder: &mut DisplayBuilder,
        raw: RawDisplayHandle,
    ) -> Result<Self, Error> {
        Ok(Self {
            root: sb::Context::from_raw(raw).unwrap(),
            cache: piet_tiny_skia::Cache::new(),
        })
    }

    pub(super) async unsafe fn make_surface(
        &mut self,
        raw: RawWindowHandle,
        width: u32,
        height: u32,
    ) -> Result<Surface, Error> {
        let mut surface = unsafe { sb::Surface::from_raw(&self.root, raw).unwrap() };

        surface
            .resize(
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            )
            .unwrap();

        Ok(Surface { surface })
    }

    pub(super) fn supports_transparency(&self) -> bool {
        false
    }

    pub(super) fn x11_visual(&self) -> Option<NonNull<()>> {
        None
    }

    pub(super) async fn present(&mut self) {
        // no-op
    }
}

impl<'dsp, 'surf> RenderContext<'dsp, 'surf> {
    pub(super) unsafe fn new(
        display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        let width = NonZeroU32::new(width).ok_or(Error::InvalidInput)?;
        let height = NonZeroU32::new(height).ok_or(Error::InvalidInput)?;

        // Resize the surface.
        surface.surface.resize(width, height).unwrap();

        // Create the context.
        let mut context = display.cache.render_context(Buffer {
            buffer: surface.surface.buffer_mut().unwrap(),
            width: width.get(),
            height: height.get(),
        });

        Ok(Self {
            text: Text(crate::text::TextInner::Cosmic(context.text().clone())),
            inner: Some(context),
            dirty: false,
            mismatch_err: Ok(()),
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

    fn inner(&mut self) -> &mut piet_tiny_skia::RenderContext<'dsp, Buffer<'surf>> {
        self.inner
            .as_mut()
            .expect("Tried to use context after finish()")
    }

    pub(super) fn status(&mut self) -> Result<(), Error> {
        mem::replace(&mut self.mismatch_err, Ok(()))?;

        if let Some(inner) = self.inner.as_mut() {
            inner.status()
        } else {
            Ok(())
        }
    }

    pub(super) fn solid_brush(&mut self, color: piet::Color) -> Brush {
        self.inner().solid_brush(color)
    }

    pub(super) fn gradient(&mut self, gradient: FixedGradient) -> Result<Brush, Error> {
        self.inner().gradient(gradient)
    }

    pub(super) fn clear(&mut self, region: Option<Rect>, color: piet::Color) {
        self.inner().clear(region, color);
        self.dirty = true;
    }

    pub(super) fn stroke(&mut self, shape: impl Shape, brush: &Brush, width: f64) {
        self.inner().stroke(shape, brush, width);
        self.dirty = true;
    }

    pub(super) fn stroke_styled(
        &mut self,
        shape: impl Shape,
        brush: &Brush,
        width: f64,
        style: &StrokeStyle,
    ) {
        self.inner().stroke_styled(shape, brush, width, style);
        self.dirty = true;
    }

    pub(super) fn fill(&mut self, shape: impl Shape, brush: &Brush) {
        self.inner().fill(shape, brush);
        self.dirty = true;
    }

    pub(super) fn fill_even_odd(&mut self, shape: impl Shape, brush: &Brush) {
        self.inner().fill_even_odd(shape, brush);
        self.dirty = true;
    }

    pub(super) fn clip(&mut self, shape: impl Shape) {
        self.inner().clip(shape);
        self.dirty = true;
    }

    pub(super) fn text(&mut self) -> &mut Text {
        &mut self.text
    }

    pub(super) fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<Point>) {
        let layout = match &layout.0 {
            TextLayoutInner::Cosmic(ct) => ct,
            _ => {
                self.mismatch_err = Err(piet::Error::NotSupported);
                return;
            }
        };
        self.inner().draw_text(layout, pos);
        self.dirty = true;
    }

    pub(super) fn save(&mut self) -> Result<(), Error> {
        self.inner().save()
    }

    pub(super) fn restore(&mut self) -> Result<(), Error> {
        self.inner().restore()
    }

    pub(super) fn finish(&mut self) -> Result<(), Error> {
        // Wrap and get the inner buffer.
        let Buffer { mut buffer, .. } = self.inner.take().unwrap().into_target();

        // tiny-skia uses an RGBA format, while softbuffer uses XRGB. To convert, we need to
        // iterate over the pixels and shift the pixels over.
        buffer.iter_mut().for_each(|pixel| {
            let [r, g, b, _] = pixel.to_ne_bytes();
            *pixel = (b as u32) | ((g as u32) << 8) | ((r as u32) << 16);
        });

        // Upload the buffer.
        buffer.present().unwrap();

        Ok(())
    }

    pub(super) fn transform(&mut self, transform: Affine) {
        self.inner().transform(transform);
    }

    pub(super) fn make_image(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        format: ImageFormat,
    ) -> Result<Image, Error> {
        self.inner().make_image(width, height, buf, format)
    }

    pub(super) fn draw_image(&mut self, image: &Image, dst_rect: Rect, interp: InterpolationMode) {
        self.inner().draw_image(image, dst_rect, interp);
        self.dirty = true;
    }

    pub(super) fn draw_image_area(
        &mut self,
        image: &Image,
        src_rect: Rect,
        dst_rect: Rect,
        interp: InterpolationMode,
    ) {
        self.inner()
            .draw_image_area(image, src_rect, dst_rect, interp);
        self.dirty = true;
    }

    pub(super) fn capture_image_area(&mut self, src_rect: Rect) -> Result<Image, Error> {
        self.inner().capture_image_area(src_rect)
    }

    pub(super) fn blurred_rect(&mut self, _rect: Rect, _blur_radius: f64, _brush: &Brush) {
        self.inner().blurred_rect(_rect, _blur_radius, _brush);
        self.dirty = true;
    }

    pub(super) fn current_transform(&self) -> Affine {
        self.inner.as_ref().unwrap().current_transform()
    }
}
