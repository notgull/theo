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

use piet::kurbo::{Affine, PathEl, Point, Rect, Shape, Size};
use piet::{FixedGradient, ImageFormat, InterpolationMode, IntoBrush, StrokeStyle};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use softbuffer::{Context, Surface as SoftbufferSurface};
use tiny_skia::{ClipMask, Paint, PathBuilder, PixmapMut, Shader};
use tinyvec::TinyVec;

use std::borrow::Cow;
use std::f32::consts::PI;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;

/// The display for the software rasterizer.
pub(super) struct Display {
    /// The software rasterizer context.
    context: Context,

    /// The text backend.
    text: Text,

    /// A cached path builder.
    path_builder: PathBuilder,
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

    /// The last error that occurred.
    last_error: Result<(), Error>,

    /// The stack of render states.
    render_states: TinyVec<[RenderState; 1]>,

    /// The tolerance for curves.
    tolerance: f64,
}

struct RenderState {
    /// The current transform.
    transform: Affine,

    /// The clipping mask.
    clip: Option<ClipMask>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            transform: Affine::IDENTITY,
            clip: None,
        }
    }
}

/// The brush used for the software rasterizer.
#[derive(Clone)]
pub(super) enum Brush {
    /// A solid color brush.
    Solid(piet::Color),
    // TODO: Other variants
}

/// The image used for the software rasterizer.
pub(super) struct Image(tiny_skia::Pixmap);

impl Display {
    pub(super) unsafe fn new(
        _builder: &mut DisplayBuilder,
        raw: RawDisplayHandle,
    ) -> Result<Self, Error> {
        let context = Context::from_raw(raw).piet_err()?;
        Ok(Self {
            context,
            text: Text(piet_cosmic_text::Text::new()),
            path_builder: PathBuilder::new(),
        })
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

    fn path_builder(&mut self) -> PathBuilder {
        self.path_builder.clear();
        std::mem::replace(&mut self.path_builder, PathBuilder::new())
    }

    fn cache_path_builder(&mut self, path_builder: PathBuilder) {
        self.path_builder = path_builder;
    }
}

impl<'dsp, 'surf> RenderContext<'dsp, 'surf> {
    pub(super) unsafe fn new(
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
            last_error: Ok(()),
            render_states: TinyVec::from([Default::default()]),
            tolerance: 5.0,
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

    fn current_state(&self) -> &RenderState {
        self.render_states.last().unwrap()
    }

    fn drawing_parts(&mut self) -> (&mut SoftbufferSurface, PixmapMut<'_>, &mut RenderState, f64) {
        let Self {
            surface,
            render_states,
            size,
            tolerance,
            ..
        } = self;
        let Surface { surface, buffer } = surface;
        let pixmap = PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(buffer.as_mut_slice()),
            size.0,
            size.1,
        )
        .unwrap();

        (
            surface,
            pixmap,
            render_states.last_mut().unwrap(),
            *tolerance,
        )
    }

    fn fill_rect(&mut self, rect: Rect, shader: Shader<'_>) {
        let (_, mut buffer, state, ..) = self.drawing_parts();

        let paint = Paint {
            shader,
            ..Default::default()
        };

        let transform = convert_transform(state.transform);

        if buffer
            .fill_rect(convert_rect(rect), &paint, transform, state.clip.as_ref())
            .is_none()
        {
            self.last_error = Err(Error::BackendError("Failed to fill rect".into()));
        }
    }

    fn size(&self) -> Size {
        Size::new(self.size.0 as f64, self.size.1 as f64)
    }

    pub(super) fn status(&mut self) -> Result<(), Error> {
        std::mem::replace(&mut self.last_error, Ok(()))
    }

    pub(super) fn solid_brush(&mut self, color: piet::Color) -> Brush {
        Brush::Solid(color)
    }

    pub(super) fn gradient(&mut self, gradient: FixedGradient) -> Result<Brush, Error> {
        todo!()
    }

    pub(super) fn clear(&mut self, region: Option<Rect>, color: piet::Color) {
        if region.is_some() || self.current_state().clip.is_some() {
            self.fill_rect(
                region.unwrap_or_else(|| Rect::from_origin_size((0.0, 0.0), self.size())),
                Shader::SolidColor(convert_color(color)),
            );
        } else {
            let (_, mut buffer, ..) = self.drawing_parts();
            buffer.fill(convert_color(color));
        }
    }

    pub(super) fn stroke(&mut self, shape: impl Shape, brush: &Brush, width: f64) {
        todo!()
    }

    pub(super) fn stroke_styled(
        &mut self,
        shape: impl Shape,
        brush: &Brush,
        width: f64,
        style: &StrokeStyle,
    ) {
        todo!()
    }

    pub(super) fn fill(&mut self, shape: impl Shape, brush: &Brush) {
        todo!()
    }

    pub(super) fn fill_even_odd(&mut self, shape: impl Shape, brush: &Brush) {
        todo!()
    }

    pub(super) fn clip(&mut self, shape: impl Shape) {
        let mut builder = self.display.path_builder();
        let (width, height) = self.size;
        let (_, _, state, tolerance) = self.drawing_parts();

        let transform = state.transform;
        convert_shape(&mut builder, shape, tolerance, Some(transform));
        let path = builder.finish().unwrap();

        // Either intersect with the current clip or create a new one.
        match &mut state.clip {
            Some(ref mut clip) => {
                clip.intersect_path(&path, tiny_skia::FillRule::EvenOdd, false)
                    .unwrap();
            }

            slot @ None => {
                let mut mask = ClipMask::new();
                mask.set_path(width, height, &path, tiny_skia::FillRule::EvenOdd, false)
                    .unwrap();
                *slot = Some(mask);
            }
        }

        self.display.cache_path_builder(path.clear());
    }

    pub(super) fn text(&mut self) -> &mut Text {
        &mut self.display.text
    }

    pub(super) fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<piet::kurbo::Point>) {
        todo!()
    }

    pub(super) fn save(&mut self) -> Result<(), Error> {
        self.render_states.push(Default::default());
        Ok(())
    }

    pub(super) fn restore(&mut self) -> Result<(), Error> {
        if self.render_states.len() <= 1 {
            return Err(Error::StackUnbalance);
        }

        self.render_states.pop();
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<(), Error> {
        todo!()
    }

    pub(super) fn transform(&mut self, transform: Affine) {
        let (_, _, state, ..) = self.drawing_parts();
        state.transform = transform * state.transform;
    }

    pub(super) fn make_image(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        format: ImageFormat,
    ) -> Result<Image, Error> {
        todo!()
    }

    pub(super) fn draw_image(&mut self, image: &Image, dst_rect: Rect, interp: InterpolationMode) {
        todo!()
    }

    pub(super) fn draw_image_area(
        &mut self,
        image: &Image,
        src_rect: Rect,
        dst_rect: Rect,
        interp: InterpolationMode,
    ) {
        todo!()
    }

    pub(super) fn capture_image_area(&mut self, src_rect: Rect) -> Result<Image, Error> {
        todo!()
    }

    pub(super) fn blurred_rect(&mut self, rect: Rect, blur_radius: f64, brush: &Brush) {
        todo!()
    }

    pub(super) fn current_transform(&self) -> Affine {
        self.current_state().transform
    }
}

impl Image {
    pub(super) fn size(&self) -> Size {
        let width = self.0.width() as f64;
        let height = self.0.height() as f64;

        Size::new(width, height)
    }
}

fn convert_transform(affine: Affine) -> tiny_skia::Transform {
    let [a, b, c, d, e, f] = affine.as_coeffs();
    tiny_skia::Transform::from_row(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32)
}

fn convert_rect(rect: Rect) -> tiny_skia::Rect {
    let x = rect.x0 as f32;
    let y = rect.y0 as f32;
    let width = rect.width() as f32;
    let height = rect.height() as f32;

    tiny_skia::Rect::from_xywh(x, y, width, height).unwrap()
}

fn convert_point(point: Point) -> tiny_skia::Point {
    tiny_skia::Point {
        x: point.x as f32,
        y: point.y as f32,
    }
}

fn convert_color(color: piet::Color) -> tiny_skia::Color {
    let (r, g, b, a) = color.as_rgba();
    tiny_skia::Color::from_rgba(r as f32, g as f32, b as f32, a as f32).unwrap()
}

fn convert_shape(
    builder: &mut PathBuilder,
    shape: impl Shape,
    tolerance: f64,
    transform: Option<Affine>,
) {
    let transform = |pt: Point| {
        if let Some(transform) = transform {
            transform * pt
        } else {
            pt
        }
    };

    shape.path_elements(tolerance).for_each(|el| match el {
        PathEl::MoveTo(pt) => {
            let pt = transform(pt);
            builder.move_to(pt.x as f32, pt.y as f32);
        }

        PathEl::LineTo(pt) => {
            let pt = transform(pt);
            builder.line_to(pt.x as f32, pt.y as f32);
        }

        PathEl::QuadTo(p1, p2) => {
            let p1 = transform(p1);
            let p2 = transform(p2);
            builder.quad_to(p1.x as f32, p1.y as f32, p2.x as f32, p2.y as f32);
        }

        PathEl::CurveTo(p1, p2, p3) => {
            let p1 = transform(p1);
            let p2 = transform(p2);
            let p3 = transform(p3);
            builder.cubic_to(
                p1.x as f32,
                p1.y as f32,
                p2.x as f32,
                p2.y as f32,
                p3.x as f32,
                p3.y as f32,
            );
        }

        PathEl::ClosePath => {
            builder.close();
        }
    })
}
