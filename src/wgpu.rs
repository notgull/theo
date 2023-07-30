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

//! The `wgpu` backend.

use crate::text::{Text, TextInner};
use crate::{DisplayBuilder, Error, ResultExt, SwitchToSwrast};

use piet::kurbo::{Point, Rect, Shape};
use piet::{RenderContext as _, StrokeStyle};
use piet_wgpu::WgpuContext;
use raw_window_handle::{
    HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use slab::Slab;

use std::rc::{Rc, Weak};

/// The display for the `wgpu` backend.
pub(super) struct Display {
    /// The instance.
    instance: wgpu::Instance,

    /// The underlying raw display handle.
    raw: RawDisplayHandle,

    /// Do we support transparency?
    supports_transparency: bool,

    /// The list of known adapters.
    adapters: Vec<AdapterInfo>,

    /// The list of known surfaces.
    surfaces: Slab<SurfaceInfo>,
}

/// The surface for the `wgpu` backend.
pub(super) struct Surface {
    /// The index into `surfaces` in `Display`.
    surface_index: usize,

    /// Shared state indicating that this has been dropped.
    _dropped: Rc<()>,
}

struct AdapterInfo {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

struct SurfaceInfo {
    /// The underlying `wgpu` surface.
    surface: wgpu::Surface,

    /// The surface configuration.
    config: wgpu::SurfaceConfiguration,

    /// The WGPU context.
    context: WgpuContext,

    /// The index of the adapter that this surface is associated with.
    adapter_index: usize,

    /// The texture associated with the surface.
    texture: Option<wgpu::SurfaceTexture>,

    /// Whether or not the representative `Surface` has been dropped.
    dropped: Weak<()>,
}

/// The rendering context.
pub(super) struct RenderContext<'dsp, 'srf> {
    /// The inner context.
    inner: piet_wgpu::RenderContext<'dsp, 'dsp, 'dsp>,

    /// The surface we're drawing to.
    _surface: &'srf mut Surface,

    /// The text context.
    text: Text,
}

impl Display {
    pub(super) unsafe fn new(
        builder: &mut DisplayBuilder,
        raw: RawDisplayHandle,
    ) -> Result<Self, Error> {
        if builder.force_swrast {
            return Err(Error::BackendError(SwitchToSwrast.into()));
        }

        // Create the instance.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
        });

        Ok(Self {
            instance,
            raw,
            supports_transparency: builder.transparent,
            adapters: vec![],
            surfaces: Slab::new(),
        })
    }

    pub(super) fn supports_transparency(&self) -> bool {
        self.supports_transparency
    }

    pub(super) fn x11_visual(&self) -> Option<std::ptr::NonNull<()>> {
        None
    }

    pub(super) async unsafe fn make_surface(
        &mut self,
        raw: RawWindowHandle,
        width: u32,
        height: u32,
    ) -> Result<Surface, Error> {
        // Create a new surface.
        let surface = self
            .instance
            .create_surface(&RawHandles(self.raw, raw))
            .piet_err()?;

        // See if we have an adaptor for this surface.
        let (index, adapter) = if let Some(adapter) = self
            .adapters
            .iter()
            .enumerate()
            .find(|(_, a)| a.adapter.is_surface_supported(&surface))
        {
            adapter
        } else {
            // Request a new adapter.
            let adapter = self
                .instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                })
                .await
                .ok_or_else(|| Error::NotSupported)?;

            // Create the device and queue.
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("theo device and queue"),
                        features: wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER,
                        limits: wgpu::Limits::default(),
                    },
                    None,
                )
                .await
                .piet_err()?;

            // Add it to the list of known adapters.
            self.adapters.push(AdapterInfo {
                adapter,
                device,
                queue,
            });
            (self.adapters.len() - 1, self.adapters.last().unwrap())
        };

        // Get the surface capabilities.
        let cap = surface.get_capabilities(&adapter.adapter);

        // Create the surface configuration.
        let format = cap
            .formats
            .iter()
            .find(|format| {
                matches!(format, wgpu::TextureFormat::Rgba8Unorm)
                    | matches!(format, wgpu::TextureFormat::Bgra8Unorm)
            })
            .or_else(|| cap.formats.first())
            .ok_or(Error::NotSupported)?;
        let alpha_mode = cap
            .alpha_modes
            .iter()
            .find(|am| {
                if self.supports_transparency {
                    matches!(
                        am,
                        wgpu::CompositeAlphaMode::PostMultiplied
                            | wgpu::CompositeAlphaMode::Inherit
                    )
                } else {
                    true
                }
            })
            .or_else(|| cap.alpha_modes.first())
            .ok_or(Error::NotSupported)?;

        let config = wgpu::SurfaceConfiguration {
            format: *format,
            width,
            height,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: *alpha_mode,
            view_formats: vec![*format],
        };

        // Create a signal to indicate that the surface has been dropped.
        let signal = Rc::new(());

        let info = SurfaceInfo {
            surface,
            config,
            context: WgpuContext::new(&adapter.device, &adapter.queue, *format, None, 1),
            texture: None,
            adapter_index: index,
            dropped: Rc::downgrade(&signal),
        };

        // Put the surface in our list.
        let surface_index = self.surfaces.insert(info);

        Ok(Surface {
            surface_index,
            _dropped: signal,
        })
    }

    #[inline]
    pub(crate) async fn present(&mut self) {
        // TODO: Use an executor to .await on the queues finishing.

        // Run submit operations for each adapter.
        for (adapter_index, adapter) in self.adapters.iter().enumerate() {
            let mut encoder =
                adapter
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("theo command encoder"),
                    });

            // Encode every surface's operations that are attached to this adapter.
            // TODO: Could this be more efficient?
            for (i, surface) in &mut self.surfaces {
                if surface.adapter_index == adapter_index {
                    let surface_texture = surface
                        .texture
                        .get_or_insert_with(|| surface.surface.get_current_texture().unwrap());
                    let view = surface_texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());

                    // TODO: MSAA
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some(&format!("theo render pass for surface #{i}")),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });
                    surface.context.render(&mut pass);
                }
            }

            // Submit the queue.
            adapter.queue.submit(Some(encoder.finish()));
        }

        // Swap the buffers on each surface.
        self.surfaces.retain(|_, surface| {
            let adapter = &self.adapters[surface.adapter_index];
            surface.context.after_submit(&adapter.device);

            if let Some(texture) = surface.texture.take() {
                texture.present();
            }

            // If we need to garbage-collect this surface, do so now.
            surface.dropped.upgrade().is_some()
        });
    }
}

impl<'dsp, 'surf> RenderContext<'dsp, 'surf> {
    pub(super) unsafe fn new(
        display: &'dsp mut Display,
        surface: &'surf mut Surface,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        let real_surface = &mut display.surfaces[surface.surface_index];
        let adapter = &display.adapters[real_surface.adapter_index];

        // Set the texture size.
        real_surface.config.width = width;
        real_surface.config.height = height;
        real_surface
            .surface
            .configure(&adapter.device, &real_surface.config);

        // Create the inner context.
        let mut inner =
            real_surface
                .context
                .prepare(&adapter.device, &adapter.queue, width, height);

        Ok(Self {
            text: Text(TextInner::Wgpu(inner.text().clone())),
            _surface: surface,
            inner,
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
            crate::text::TextLayoutInner::Wgpu(ref layout) => self.inner.draw_text(layout, pos),

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
        self.inner.finish()?;
        Ok(())
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

type Brush = piet_wgpu::Brush;
type Image = piet_wgpu::Image;

/// Combines a raw display handle and a raw window handle.
struct RawHandles(RawDisplayHandle, RawWindowHandle);

unsafe impl HasRawDisplayHandle for RawHandles {
    fn raw_display_handle(&self) -> RawDisplayHandle {
        self.0
    }
}

unsafe impl HasRawWindowHandle for RawHandles {
    fn raw_window_handle(&self) -> RawWindowHandle {
        self.1
    }
}
