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
use piet::{
    FixedGradient, FixedLinearGradient, FixedRadialGradient, ImageFormat, InterpolationMode,
    LineCap, LineJoin, StrokeStyle,
};

use cosmic_text::{CacheKey, Color as CosmicColor, Font};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use softbuffer::{Context, Surface as SoftbufferSurface};
use tiny_skia::{ClipMask, FillRule, Paint, PathBuilder, Pixmap, PixmapMut, Shader};
use tinyvec::TinyVec;

use std::collections::hash_map::{Entry, HashMap};
use std::mem;
use std::ptr::NonNull;

/// The display for the software rasterizer.
pub(super) struct Display {
    /// The software rasterizer context.
    context: Context,

    /// The text backend.
    text: Text,

    /// A cached path builder.
    path_builder: PathBuilder,

    /// Map between cache keys and cached glyphs.
    glyph_cache: HashMap<(CacheKey, CosmicColor), Pixmap>,
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

    /// Whether we currently need to update the render state.
    dirty: bool,
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

    /// A fixed linear gradient brush.
    ///
    /// TODO: Cache the gradient.
    LinearGradient(FixedLinearGradient),

    /// A fixed radial gradient brush.
    ///
    /// TODO: Cache the gradient.
    RadialGradient(FixedRadialGradient),
}

impl Brush {
    fn to_shader(&self) -> Result<Shader<'_>, Error> {
        match self {
            Brush::Solid(color) => Ok(Shader::SolidColor(convert_color(*color))),
            Brush::LinearGradient(linear) => {
                let start = convert_point(linear.start);
                let end = convert_point(linear.end);
                let stops = linear.stops.iter().map(convert_gradient_stop).collect();

                tiny_skia::LinearGradient::new(
                    start,
                    end,
                    stops,
                    tiny_skia::SpreadMode::Pad,
                    tiny_skia::Transform::identity(),
                )
                .ok_or_else(|| Error::BackendError("failed to create linear gradient".into()))
            }
            Brush::RadialGradient(radial) => {
                let start = convert_point(radial.center);
                let end = convert_point(radial.center + radial.origin_offset);
                let stops = radial.stops.iter().map(convert_gradient_stop).collect();

                tiny_skia::RadialGradient::new(
                    start,
                    end,
                    radial.radius as _,
                    stops,
                    tiny_skia::SpreadMode::Pad,
                    tiny_skia::Transform::identity(),
                )
                .ok_or_else(|| Error::BackendError("failed to create radial gradient".into()))
            }
        }
    }
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
            glyph_cache: HashMap::new(),
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
        mem::replace(&mut self.path_builder, PathBuilder::new())
    }

    fn cache_path_builder(&mut self, path_builder: PathBuilder) {
        self.path_builder = path_builder;
    }
}

macro_rules! leap {
    ($self:expr, $e:expr) => {{
        match $e {
            Ok(x) => x,
            Err(err) => {
                $self.last_error = Err(err);
                return;
            }
        }
    }};
    ($self:expr, $e:expr, $err:expr) => {{
        match $e {
            Some(x) => x,
            None => {
                let err = $err;
                $self.last_error = Err(Error::BackendError(err.into()));
                return;
            }
        }
    }};
}

impl<'dsp, 'surf> RenderContext<'dsp, 'surf> {
    pub(super) unsafe fn new(
        display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        if width == 0 || height == 0 {
            return Err(Error::InvalidInput);
        }

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
            dirty: true,
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

    fn drawing_parts(&mut self) -> (&mut Display, PixmapMut<'_>, &mut RenderState, f64) {
        let Self {
            display,
            surface: Surface { buffer, .. },
            render_states,
            size,
            tolerance,
            ..
        } = self;

        let pixmap = PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(buffer.as_mut_slice()),
            size.0,
            size.1,
        )
        .expect("There should be no way to create a pixmap with invalid parameters");

        (
            display,
            pixmap,
            render_states.last_mut().expect(STACK_UNBALANCE),
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

        if convert_rect(rect)
            .and_then(|rect| buffer.fill_rect(rect, &paint, transform, state.clip.as_ref()))
            .is_none()
        {
            self.last_error = Err(Error::BackendError("Failed to fill rect".into()));
        }

        self.dirty = true;
    }

    fn fill_impl(&mut self, shape: impl Shape, brush: &Brush, fill_rule: FillRule) {
        let mut builder = self.display.path_builder();
        let (_, mut buffer, state, tolerance) = self.drawing_parts();
        let paint = Paint {
            shader: leap!(self, brush.to_shader()),
            ..Default::default()
        };

        let transform = convert_transform(state.transform);
        convert_shape(&mut builder, shape, tolerance, None);
        let path = leap!(self, builder.finish(), "Failed to build path");

        buffer
            .fill_path(&path, &paint, fill_rule, transform, state.clip.as_ref());

        self.display.cache_path_builder(path.clear());
        self.dirty = true;
    }

    fn size(&self) -> Size {
        Size::new(self.size.0 as f64, self.size.1 as f64)
    }

    pub(super) fn status(&mut self) -> Result<(), Error> {
        mem::replace(&mut self.last_error, Ok(()))
    }

    pub(super) fn solid_brush(&mut self, color: piet::Color) -> Brush {
        Brush::Solid(color)
    }

    pub(super) fn gradient(&mut self, gradient: FixedGradient) -> Result<Brush, Error> {
        match gradient {
            FixedGradient::Linear(linear) => Ok(Brush::LinearGradient(linear)),
            FixedGradient::Radial(radial) => Ok(Brush::RadialGradient(radial)),
        }
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
        self.stroke_styled(shape, brush, width, &Default::default())
    }

    pub(super) fn stroke_styled(
        &mut self,
        shape: impl Shape,
        brush: &Brush,
        width: f64,
        style: &StrokeStyle,
    ) {
        let mut builder = self.display.path_builder();
        let (_, mut buffer, state, tolerance) = self.drawing_parts();

        let paint = Paint {
            shader: leap!(self, brush.to_shader()),
            ..Default::default()
        };

        let transform = convert_transform(state.transform);

        convert_shape(&mut builder, shape, tolerance, None);
        let path = leap!(self, builder.finish(), "Failed to build path");

        // Convert stroke properties.
        let stroke = tiny_skia::Stroke {
            width: width as f32,
            miter_limit: style.miter_limit().map_or(4.0, |limit| limit as f32),
            line_cap: match style.line_cap {
                LineCap::Butt => tiny_skia::LineCap::Butt,
                LineCap::Round => tiny_skia::LineCap::Round,
                LineCap::Square => tiny_skia::LineCap::Square,
            },
            line_join: match style.line_join {
                LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
                LineJoin::Round => tiny_skia::LineJoin::Round,
                LineJoin::Miter { .. } => tiny_skia::LineJoin::Miter,
            },
            dash: if style.dash_pattern.is_empty() {
                None
            } else {
                tiny_skia::StrokeDash::new(
                    style.dash_pattern.iter().map(|&x| x as f32).collect(),
                    style.dash_offset as f32,
                )
            },
        };

        // Draw the path.
        buffer
            .stroke_path(&path, &paint, &stroke, transform, state.clip.as_ref());        

        self.display.cache_path_builder(path.clear());
        self.dirty = true;
    }

    pub(super) fn fill(&mut self, shape: impl Shape, brush: &Brush) {
        self.fill_impl(shape, brush, FillRule::Winding)
    }

    pub(super) fn fill_even_odd(&mut self, shape: impl Shape, brush: &Brush) {
        self.fill_impl(shape, brush, FillRule::EvenOdd)
    }

    pub(super) fn clip(&mut self, shape: impl Shape) {
        let mut builder = self.display.path_builder();
        let (width, height) = self.size;
        let (_, _, state, tolerance) = self.drawing_parts();

        let transform = state.transform;
        convert_shape(&mut builder, shape, tolerance, Some(transform));
        let path = match builder.finish() {
            Some(path) => path,
            None => {
                // Empty path.
                return;
            }
        };

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

    pub(super) fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<Point>) {
        let (display, mut buffer, state, ..) = self.drawing_parts();
        let pos = pos.into();

        for (glyph, y_start) in layout
            .0
            .layout_runs()
            .flat_map(|run| run.glyphs.iter().map(move |glyph| (glyph, run.line_y)))
        {
            // Get the rasterized glyph for this glyph.
            let color = glyph
                .color_opt
                .unwrap_or(CosmicColor::rgba(0x0, 0x0, 0x0, 0xFF));
            let rasterized = match display.glyph_cache.entry((glyph.cache_key, color)) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => {
                    let font = layout
                        .0
                        .buffer()
                        .font_system()
                        .get_font(glyph.cache_key.font_id)
                        .expect("Font not found");
                    let pixmap = leap!(self, rasterize_glyph(&font, glyph.cache_key, color));
                    v.insert(pixmap)
                }
            };

            let pattern = tiny_skia::Pattern::new(
                rasterized.as_ref(),
                tiny_skia::SpreadMode::Pad,
                tiny_skia::FilterQuality::Bilinear,
                1.0,
                tiny_skia::Transform::identity(),
            );

            // Compute the position of the glyph.
            let x = glyph.x_int + pos.x as i32;
            let y = glyph.y_int + y_start + pos.y as i32;

            let paint = Paint {
                shader: pattern,
                ..Default::default()
            };

            // Composite the glyph onto the buffer.
            let transform = convert_transform(state.transform);
            leap!(
                self,
                buffer.fill_rect(
                    convert_rect(Rect::from_origin_size(
                        (x as f64, y as f64),
                        (rasterized.width() as f64, rasterized.height() as f64),
                    ))
                    .unwrap(),
                    &paint,
                    transform,
                    state.clip.as_ref(),
                ),
                "Failed to draw text"
            );
        }
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
        // tiny-skia uses an RGBA format, while softbuffer uses XRGB. To convert, we need to
        // iterate over the pixels and shift the pixels over.
        self.surface.buffer.iter_mut().for_each(|pixel| {
            let [r, g, b, _] = pixel.to_ne_bytes();
            *pixel = (b as u32) | ((g as u32) << 8) | ((r as u32) << 16);
        });

        // Upload the buffer.
        self.surface
            .surface
            .set_buffer(&self.surface.buffer, self.size.0 as _, self.size.1 as _);

        Ok(())
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
        let buffer = match format {
            ImageFormat::RgbaPremul => {
                // This is the format that tiny-skia uses, so we can just use the buffer directly.
                buf.to_vec()
            }

            _ => {
                return Err(Error::NotSupported);
            }
        };

        let pixmap = Pixmap::from_vec(
            buffer,
            tiny_skia_path::IntSize::from_wh(width as _, height as _)
                .ok_or_else(|| Error::InvalidInput)?,
        )
        .ok_or_else(|| Error::InvalidInput)?;

        Ok(Image(pixmap))
    }

    pub(super) fn draw_image(&mut self, image: &Image, dst_rect: Rect, interp: InterpolationMode) {
        self.draw_image_area(
            image,
            Rect::new(0.0, 0.0, image.size().width, image.size().height),
            dst_rect,
            interp,
        )
    }

    pub(super) fn draw_image_area(
        &mut self,
        image: &Image,
        src_rect: Rect,
        dst_rect: Rect,
        interp: InterpolationMode,
    ) {
        // Create a transform to scale the image to the correct size.
        let transform = convert_transform(
            Affine::translate((src_rect.x0, src_rect.y0))
                * Affine::scale_non_uniform(
                    src_rect.width() / dst_rect.width(),
                    src_rect.height() / dst_rect.height(),
                ),
        );

        // Create a pattern.
        let pattern = tiny_skia::Pattern::new(
            image.0.as_ref(),
            tiny_skia::SpreadMode::Pad,
            match interp {
                InterpolationMode::NearestNeighbor => tiny_skia::FilterQuality::Nearest,
                InterpolationMode::Bilinear => tiny_skia::FilterQuality::Bilinear,
            },
            1.0,
            transform,
        );
        let paint = Paint {
            shader: pattern,
            ..Default::default()
        };

        // Draw the image.
        let (_, mut buffer, state, ..) = self.drawing_parts();
        let transform = convert_transform(state.transform);
        leap!(
            self,
            buffer.fill_rect(
                convert_rect(dst_rect).unwrap(),
                &paint,
                transform,
                state.clip.as_ref(),
            ),
            "Failed to draw image"
        );
    }

    pub(super) fn capture_image_area(&mut self, src_rect: Rect) -> Result<Image, Error> {
        let (width, height) = (src_rect.width() as u32, src_rect.height() as u32);
        let mut pixmap = Pixmap::new(width, height).ok_or_else(|| Error::InvalidInput)?;

        // Copy the pixels from the surface.
        let transform = convert_transform(
            Affine::translate((src_rect.x0, src_rect.y0))
                * Affine::scale_non_uniform(
                    src_rect.width() / self.size.0 as f64,
                    src_rect.height() / self.size.1 as f64,
                ),
        );

        let (_, buffer, ..) = self.drawing_parts();
        let shader = tiny_skia::Pattern::new(
            buffer.as_ref(),
            tiny_skia::SpreadMode::Pad,
            tiny_skia::FilterQuality::Bilinear,
            1.0,
            transform,
        );
        let paint = tiny_skia::Paint {
            shader,
            ..Default::default()
        };

        pixmap
            .fill_rect(
                tiny_skia::Rect::from_xywh(0.0, 0.0, width as _, height as _).unwrap(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            )
            .ok_or_else(|| Error::InvalidInput)?;

        // Return the image.
        Ok(Image(pixmap))
    }

    pub(super) fn blurred_rect(&mut self, _rect: Rect, _blur_radius: f64, _brush: &Brush) {
        self.last_error = Err(Error::NotSupported);
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

/// Rasterize a glyph to a `Pixmap` using `ab_glyph`.
fn rasterize_glyph(
    face: &Font<'_>,
    glyph_key: CacheKey,
    color: CosmicColor,
) -> Result<Pixmap, Error> {
    use ab_glyph::Font;

    // Set up a rasterizer.
    let font_ref =
        ab_glyph::FontRef::try_from_slice_and_index(face.data, face.info.index).piet_err()?;

    // Get the scaled glyph.
    let glyph = ab_glyph::GlyphId(glyph_key.glyph_id).with_scale(glyph_key.font_size as f32);
    let [r, g, b, a] = [color.r(), color.g(), color.b(), color.a()];

    let color = tiny_skia::ColorU8::from_rgba(r, g, b, a);

    // Outline the glyph.
    let outlined = match font_ref.outline_glyph(glyph) {
        Some(outlined) => outlined,
        None => {
            // TODO
            return Err(Error::Unimplemented);
        }
    };

    // Create a pixmap.
    let width = outlined.glyph().scale.x as u32;
    let height = outlined.glyph().scale.y as u32;

    let mut pixmap = Pixmap::new(width, height).expect("Bounds should never be zero");
    let width = pixmap.width();
    let pixels = pixmap.pixels_mut();

    // Rasterize the glyph.
    outlined.draw(|x, y, c| {
        let color = {
            tiny_skia::ColorU8::from_rgba(
                color.red(),
                color.green(),
                color.blue(),
                (color.alpha() as f32 * c) as u8,
            )
        };

        let index = (y * width + x) as usize;
        pixels[index] = color.premultiply();
    });

    Ok(pixmap)
}

fn convert_transform(affine: Affine) -> tiny_skia::Transform {
    let [a, b, c, d, e, f] = affine.as_coeffs();
    tiny_skia::Transform::from_row(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32)
}

fn convert_rect(rect: Rect) -> Option<tiny_skia::Rect> {
    let x = rect.x0 as f32;
    let y = rect.y0 as f32;
    let width = rect.width() as f32;
    let height = rect.height() as f32;

    tiny_skia::Rect::from_xywh(x, y, width, height)
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

fn convert_gradient_stop(stop: &piet::GradientStop) -> tiny_skia::GradientStop {
    tiny_skia::GradientStop::new(stop.pos, convert_color(stop.color))
}

const STACK_UNBALANCE: &str = "Render state stack unbalance";
