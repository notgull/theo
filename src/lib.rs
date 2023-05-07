// SPDX-License-Identifier: LGPL-3.0-or-later OR MPL-2.0
// This file is a part of `theo`.
//
// `theo` is free software: you can redistribute it and/or modify it under the terms of
// either:
//
// * GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
// * Mozilla Public License as published by the Mozilla Foundation, version 2.
// * The Patron License (https://github.com/notgull/theo/blob/main/LICENSE-PATRON.md)
//   for sponsors and contributors, who can ignore the copyleft provisions of the above licenses
//   for this project.
//
// `theo` is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License or the Mozilla Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License and the Mozilla
// Public License along with `theo`. If not, see <https://www.gnu.org/licenses/>.

//! A generic [`piet`] rendering context for all windowing and graphics backends.
//!
//! Windowing frameworks like [`winit`] do not provide a way to draw into them by default. This decision
//! is intentional; it allows the user to choose which graphics backend that they'd like to use, and also
//! makes maintaining the windowing code much simpler. For games (what [`winit`] was originally designed
//! for), usually a 3D rendering context like [`wgpu`] or [`glow`] would be used in this case. However,
//! GUI applications will need a 2D vector graphics context.
//!
//! [`piet`] is a 2D graphics abstraction that can be used with many different graphics backends. However,
//! [`piet`]'s default implementation, [`piet-common`], is difficult to integrate with windowing systems.
//! [`theo`] aims to bridge this gap by providing a generic [`piet`] rendering context that easily
//! integrates with windowing systems.
//!
//! Rather than going through drawing APIs like [`cairo`] and DirectX, `theo` directly uses GPU APIs in
//! order to render to the window. This allows for better performance and greater flexibility, and also
//! ensures that much of the rendering logic is safe. This also reduces the number of dynamic
//! dependencies that your final program needs to rely on.
//!
//! `theo` prioritizes versatility and performance. By default, `theo` uses an optimized GPU backend for
//! rendering. If the GPU is not available, `theo` will fall back to software rendering.
//!
//! ## Usage Example
//!
//! First, users must create a [`Display`], which represents the root display of the system. From here,
//! users should create [`Surface`]s, which represent drawing areas. Finally, a [`Surface`] can be used
//! to create the [`RenderContext`] type, which is used to draw.
//!
//! ```no_run
//! use piet::{RenderContext as _, kurbo::Circle};
//! use theo::{Display, Surface, RenderContext};
//! # struct MyDisplay;
//! # unsafe impl raw_window_handle::HasRawDisplayHandle for MyDisplay {
//! #     fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
//! #         unimplemented!()
//! #     }
//! # }
//! # struct Window;
//! # unsafe impl raw_window_handle::HasRawWindowHandle for Window {
//! #     fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
//! #         unimplemented!()
//! #     }
//! # }
//! # impl Window {
//! #     fn width(&self) -> u32 { 0 }
//! #     fn height(&self) -> u32 { 0 }
//! #     fn on_draw(&self, f: impl FnOnce()) { f() }
//! # }
//! # let my_display = MyDisplay;
//! # let window = Window;
//!
//! # futures_lite::future::block_on(async move {
//! // Create a display using a display handle from your windowing framework.
//! // It must implement `raw_window_handle::HasRawDisplayHandle`.
//! let mut display = unsafe {
//!     Display::builder()
//!         .build(&my_display)
//!         .expect("failed to create display")
//! };
//!
//! // Create a surface using a window handle from your windowing framework.
//! // It must implement `raw_window_handle::HasRawWindowHandle`.
//! let surface_future = unsafe {
//!     display.make_surface(
//!         &window,
//!         window.width(),
//!         window.height()
//!     )
//! };
//!
//! // make_surface returns a future that needs to be polled.
//! let mut surface = surface_future.await.expect("failed to create surface");
//!
//! // Set up drawing logic.
//! window.on_draw(|| {
//!     // Create the render context.
//!     let mut ctx = RenderContext::new(
//!         &mut display,
//!         &mut surface,
//!         window.width(),
//!         window.height()
//!     ).expect("failed to create render context");
//!
//!     // Clear the screen and draw a circle.
//!     ctx.clear(None, piet::Color::WHITE);
//!     ctx.fill(
//!         &Circle::new((200.0, 200.0), 50.0),
//!         &piet::Color::RED
//!     );
//!
//!     // Finish drawing.
//!     ctx.finish().expect("failed to finish drawing");
//! });
//! # });
//! ```
//!
//! See the documentation for the [`piet`] crate for more information on how to use the drawing API.
//!
//! # Backends
//!
//! As of the time of writing, `theo` supports the following backends:
//!
//! - [`wgpu`] backend (enabled with the `wgpu` feature), which uses the [`piet-wgpu`] crate to render
//!   to the window. This backend supports all of the graphics APIs that `wgpu` supports, including
//!   Vulkan, Metal, and DirectX 11/12.
//! - [`glow`] backend (enabled with the `gl` feature), which uses the [`piet-glow`] crate to render to
//!   the window. [`glutin`] is used on desktop platforms to create the OpenGL context, and [`glow`] is
//!   used to interact with the OpenGL API. This backend supports OpenGL 3.2 and above.
//! - A software rasterization backend. [`tiny-skia`] is used to render to a bitmap, and then
//!   [`softbuffer`] is used to copy the bitmap to the window. This backend is enabled by default and is
//!   used when no other backend is available.
//!
//! # Performance
//!
//! As `theo` implements most of its own rendering logic, this can lead to serious performance
//! degradations if used improperly, especially on the software rasterization backend. In some cases,
//! compiling `theo` on Debug Mode rather than Release Mode can half the frame rate of the application.
//! If you are experiencing low frame rates with `theo`, make sure that you are compiling it on Release
//! Mode.
//!
//! In addition, gradient brushes are optimized in such a way that the actual gradient needs to be
//! computed only once. However, this means that, if you re-instantiate the brush every time, the
//! gradient will be re-computed every time. This can lead to serious performance degradations even on
//! hardware-accelerated backends. The solution is to cache the brushes that you use. For instance,
//! instead of doing this:
//!
//! ```no_compile
//! let gradient = /* ... */;
//! window.on_draw(|| {
//!     let mut ctx = /* ... */;
//!     ctx.fill(&Circle::new((200.0, 200.0), 50.0), &gradient);
//! })
//! ```
//!
//! Do this, making sure to cache the gradient brush:
//!
//! ```no_compile
//! let gradient = /* ... */;
//! let mut gradient_brush = None;
//! window.on_draw(|| {
//!     let mut ctx = /* ... */;
//!     let gradient_brush = gradient_brush.get_or_insert_with(|| {
//!         ctx.gradient_brush(gradient.clone()).unwrap()
//!     });
//!     ctx.fill(&Circle::new((200.0, 200.0), 50.0), gradient_brush);
//! })
//! ```
//!
//! `theo` explicitly opts into a thread-unsafe model. Not only is thread-unsafe code more performant,
//! but these API types are usually thread-unsafe anyways.
//!
//! [`cairo`]: https://www.cairographics.org/
//! [`softbuffer`]: https://crates.io/crates/softbuffer
//! [`tiny-skia`]: https://crates.io/crates/tiny-skia
//! [`piet-wgpu`]: https://crates.io/crates/piet-wgpu
//! [`piet-glow`]: https://crates.io/crates/piet-glow
//! [`glutin`]: https://crates.io/crates/glutin
//! [`piet`]: https://crates.io/crates/piet
//! [`piet-common`]: https://crates.io/crates/piet-common
//! [`winit`]: https://crates.io/crates/winit
//! [`wgpu`]: https://crates.io/crates/wgpu
//! [`glow`]: https://crates.io/crates/glow
//! [`theo`]: https://crates.io/crates/theo

#[cfg(feature = "wgpu")]
extern crate wgpu0 as wgpu;

#[cfg(feature = "gl")]
mod desktop_gl;
mod swrast;
mod text;
#[cfg(feature = "wgpu")]
#[path = "wgpu.rs"]
mod wgpu_backend;

use piet::kurbo::{Affine, Point, Shape, Size};
use piet::{kurbo::Rect, Error};
use piet::{FixedGradient, ImageFormat, InterpolationMode, IntoBrush, StrokeStyle};

use raw_window_handle::{
    HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle,
};

use std::borrow::Cow;
use std::cell::Cell;
use std::ffi::c_void;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

pub use text::{Text, TextLayout, TextLayoutBuilder};

std::thread_local! {
    // Make sure that we don't try to multiple contexts per thread.
    static HAS_CONTEXT: Cell<bool> = Cell::new(false);
}

/// An error handler for GLX.
pub type XlibErrorHook = Box<dyn Fn(*mut c_void, *mut c_void) -> bool + Send + Sync>;

/// An error handler for GLX.
type XlibErrorHookRegistrar = Box<dyn Fn(XlibErrorHook)>;

/// A builder containing system-specific information.
pub struct DisplayBuilder {
    /// The raw window handle to use to bootstrap the context.
    ///
    /// This is only necessary for WGL bootstrapping.
    window: Option<RawWindowHandle>,

    /// The error handler for GLX.
    glx_error_hook: Option<XlibErrorHookRegistrar>,

    /// Whether or not we should support transparent backgrounds.
    transparent: bool,

    /// Force software rendering.
    force_swrast: bool,

    _thread_unsafe: PhantomData<*mut ()>,
}

impl Default for DisplayBuilder {
    fn default() -> Self {
        Self {
            window: None,
            glx_error_hook: None,
            transparent: true,
            force_swrast: false,
            _thread_unsafe: PhantomData,
        }
    }
}

impl DisplayBuilder {
    /// Create a new [`DisplayBuilder`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the raw window handle to use to bootstrap the context.
    ///
    /// This is only necessary for WGL bootstrapping.
    pub fn window(mut self, window: impl HasRawWindowHandle) -> Self {
        self.window = Some(window.raw_window_handle());
        self
    }

    /// Set the error handler for GLX.
    pub fn glx_error_hook(mut self, hook: impl Fn(XlibErrorHook) + 'static) -> Self {
        self.glx_error_hook = Some(Box::new(hook));
        self
    }

    /// Set whether or not we should support transparent backgrounds.
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Force software rendering.
    pub fn force_swrast(mut self, force_swrast: bool) -> Self {
        self.force_swrast = force_swrast;
        self
    }

    /// Build a new [`Display`].
    ///
    /// # Safety
    ///
    /// - The `display` handle must be a valid `display` that isn't currently suspended.
    /// - The `window` handle, if any, must also be valid.
    pub unsafe fn build(self, display: impl HasRawDisplayHandle) -> Result<Display, Error> {
        self.build_from_raw(display.raw_display_handle())
    }
}

/// The display used to manage all surfaces.
pub struct Display {
    dispatch: Box<DisplayDispatch>,
    _thread_unsafe: PhantomData<*mut ()>,
}

impl fmt::Debug for Display {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Display").finish_non_exhaustive()
    }
}

impl From<DisplayDispatch> for Display {
    fn from(dispatch: DisplayDispatch) -> Self {
        Self {
            dispatch: Box::new(dispatch),
            _thread_unsafe: PhantomData,
        }
    }
}

impl Display {
    /// Create a new [`DisplayBuilder`].
    pub fn builder() -> DisplayBuilder {
        DisplayBuilder::new()
    }

    /// Create a new, default [`Display`].
    ///
    /// # Safety
    ///
    /// The `display` handle must be a valid `display` that isn't currently suspended.
    /// See the safety requirements of [`DisplayBuilder::build`] for more information.
    pub unsafe fn new(display: impl HasRawDisplayHandle) -> Result<Self, Error> {
        Self::builder().build_from_raw(display.raw_display_handle())
    }

    /// Create a new [`Surface`] from a window.
    ///
    /// # Asynchronous
    ///
    /// This function is asynchronous, as it may be necessary to wait for the window to be
    /// ready to be drawn to.
    ///
    /// # Safety
    ///
    /// The `window` handle must be a valid `window` that isn't currently suspended. The
    /// `width` and `height` parameters aren't necessarily required to be correct, but
    /// it's recommended that they are in order to avoid visual bugs.
    pub async unsafe fn make_surface(
        &mut self,
        window: impl HasRawWindowHandle,
        width: u32,
        height: u32,
    ) -> Result<Surface, Error> {
        self.make_surface_from_raw(window.raw_window_handle(), width, height)
            .await
    }
}

/// The surface used to draw to.
pub struct Surface {
    dispatch: Box<SurfaceDispatch>,
    _thread_unsafe: PhantomData<*mut ()>,
}

impl fmt::Debug for Surface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Surface").finish_non_exhaustive()
    }
}

impl From<SurfaceDispatch> for Surface {
    fn from(dispatch: SurfaceDispatch) -> Self {
        Self {
            dispatch: Box::new(dispatch),
            _thread_unsafe: PhantomData,
        }
    }
}

/// The context used to draw to a [`Surface`].
pub struct RenderContext<'dsp, 'surf> {
    /// The dispatch used to draw to the surface.
    dispatch: Box<ContextDispatch<'dsp, 'surf>>,

    /// The mismatch error.
    mismatch: Result<(), Error>,

    /// Whether we check for an existing context.
    check_context: bool,

    /// Ensure that the context is not sent to another thread.
    _thread_unsafe: PhantomData<*mut ()>,
}

impl<'dsp, 'surf> RenderContext<'dsp, 'surf> {
    fn from_dispatch(dispatch: ContextDispatch<'dsp, 'surf>, check_context: bool) -> Self {
        Self {
            dispatch: Box::new(dispatch),
            mismatch: Ok(()),
            check_context,
            _thread_unsafe: PhantomData,
        }
    }
}

impl fmt::Debug for RenderContext<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderContext").finish_non_exhaustive()
    }
}

impl Drop for RenderContext<'_, '_> {
    fn drop(&mut self) {
        if self.check_context {
            // Unlock the thread.
            HAS_CONTEXT
                .try_with(|has_context| has_context.set(false))
                .ok();
        }
    }
}

/// The brushes used to draw to a [`Surface`].
#[derive(Clone)]
pub struct Brush {
    dispatch: Rc<BrushDispatch>,
    _thread_unsafe: PhantomData<*mut ()>,
}

impl From<BrushDispatch> for Brush {
    fn from(dispatch: BrushDispatch) -> Self {
        Self {
            dispatch: Rc::new(dispatch),
            _thread_unsafe: PhantomData,
        }
    }
}

impl fmt::Debug for Brush {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Brush").finish_non_exhaustive()
    }
}

/// The images used to draw to a [`Surface`].
#[derive(Clone)]
pub struct Image {
    dispatch: Rc<ImageDispatch>,
    _thread_unsafe: PhantomData<*mut ()>,
}

impl From<ImageDispatch> for Image {
    fn from(dispatch: ImageDispatch) -> Self {
        Self {
            dispatch: Rc::new(dispatch),
            _thread_unsafe: PhantomData,
        }
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image").finish_non_exhaustive()
    }
}

macro_rules! make_dispatch {
    ($($(#[$meta:meta])* $name:ident (
        $display:ty,
        $surface:ty,
        $ctx:ty,
        $brush:ty,
        $image:ty
    )),* $(,)?) => {
        enum DisplayDispatch {
            $(
                $(#[$meta])*
                $name($display),
            )*
        }

        enum SurfaceDispatch {
            $(
                $(#[$meta])*
                $name($surface),
            )*
        }

        enum ContextDispatch<'dsp, 'surf> {
            $(
                $(#[$meta])*
                $name($ctx),
            )*
        }

        enum BrushDispatch {
            $(
                $(#[$meta])*
                $name($brush),
            )*
        }

        enum ImageDispatch {
            $(
                $(#[$meta])*
                $name($image),
            )*
        }

        impl DisplayBuilder {
            /// Build the [`Display`] from a raw display handle.
            ///
            /// # Safety
            ///
            /// The `raw` handle must be a valid `display` that isn't currently suspended.
            /// The `raw` handle must be valid for the duration of the [`Display`].
            #[allow(unused_assignments)]
            pub unsafe fn build_from_raw(
                mut self,
                raw: RawDisplayHandle
            ) -> Result<Display, Error> {
                let mut last_error;

                $(
                    $(#[$meta])*
                    {
                        match <$display>::new(&mut self, raw) {
                            Ok(display) => {
                                tracing::trace!("Created `{}` display", stringify!($name));
                                return Ok(DisplayDispatch::$name(display).into());
                            },

                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create `{}` display: {}",
                                    stringify!($name),
                                    e
                                );

                                last_error = e;
                            }
                        }
                    }
                )*

                Err(last_error)
            }
        }

        impl Display {
            /// Whether or not this display supports transparent backgrounds.
            pub fn supports_transparency(&self) -> bool {
                match &*self.dispatch {
                    $(
                        $(#[$meta])*
                        DisplayDispatch::$name(display) => display.supports_transparency(),
                    )*
                }
            }

            /// The X11 visual used by this display, if any.
            pub fn x11_visual(&self) -> Option<std::ptr::NonNull<()>> {
                match &*self.dispatch {
                    $(
                        $(#[$meta])*
                        DisplayDispatch::$name(display) => display.x11_visual(),
                    )*
                }
            }

            /// Create a new [`Surface`] from a raw window handle.
            ///
            /// # Asynchronous
            ///
            /// This function is asynchronous, as it may be necessary to wait for the window to be
            /// ready to be drawn to.
            ///
            /// # Safety
            ///
            ///
            pub async unsafe fn make_surface_from_raw(
                &mut self,
                window: RawWindowHandle,
                width: u32,
                height: u32,
            ) -> Result<Surface, Error> {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        DisplayDispatch::$name(display) => {
                            let surface = display.make_surface(window, width, height).await?;
                            Ok(SurfaceDispatch::$name(surface).into())
                        },
                    )*
                }
            }
        }

        impl<'dsp, 'surf> RenderContext<'dsp, 'surf> {
            /// Create a new [`RenderContext`] from a [`Surface`] and a [`Display`].
            pub fn new(
                display: &'dsp mut Display,
                surface: &'surf mut Surface,
                width: u32,
                height: u32,
            ) -> Result<Self, Error> {
                // Make sure there's only one per thread.
                let prev = HAS_CONTEXT
                    .try_with(|has_context| has_context.replace(true))
                    .piet_err()?;
                if prev {
                    return Err(Error::BackendError(
                        "Only one context can be active per thread.".into()
                    ));
                }

                match (&mut *display.dispatch, &mut *surface.dispatch) {
                    $(
                        $(#[$meta])*
                        (DisplayDispatch::$name(display), SurfaceDispatch::$name(surface)) => {
                            let ctx = unsafe {
                                <$ctx>::new(display, surface, width, height)?
                            };

                            Ok(RenderContext::from_dispatch(
                                ContextDispatch::$name(ctx),
                                true
                            ))
                        },
                    )*
                    _ => Err(Error::InvalidInput)
                }
            }

            /// Create a new [`RenderContext`] without checking for exclusive access.
            ///
            /// # Safety
            ///
            ///
            pub unsafe fn new_unchecked(
                display: &'dsp mut Display,
                surface: &'surf mut Surface,
                width: u32,
                height: u32,
            ) -> Result<Self, Error> {
                match (&mut *display.dispatch, &mut *surface.dispatch) {
                    $(
                        $(#[$meta])*
                        (DisplayDispatch::$name(display), SurfaceDispatch::$name(surface)) => {
                            let ctx = <$ctx>::new_unchecked(display, surface, width, height)?;
                            Ok(RenderContext::from_dispatch(
                                ContextDispatch::$name(ctx),
                                false
                            ))
                        },
                    )*
                    _ => Err(Error::InvalidInput)
                }
            }
        }

        impl piet::RenderContext for RenderContext<'_, '_> {
            type Brush = Brush;
            type Image = Image;
            type Text = Text;
            type TextLayout = TextLayout;

            fn status(&mut self) -> Result<(), Error> {
                let mismatch = std::mem::replace(&mut self.mismatch, Ok(()));
                let status = match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => ctx.status(),
                    )*
                };

                status.and(mismatch)
            }

            fn solid_brush(&mut self, color: piet::Color) -> Self::Brush {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => {
                            let brush = ctx.solid_brush(color);
                            BrushDispatch::$name(brush).into()
                        },
                    )*
                }
            }

            fn gradient(
                &mut self,
                gradient: impl Into<FixedGradient>
            ) -> Result<Self::Brush, Error> {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => {
                            Ok(BrushDispatch::$name(ctx.gradient(gradient.into())?).into())
                        },
                    )*
                }
            }

            fn clear(&mut self, region: impl Into<Option<Rect>>, color: piet::Color) {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => ctx.clear(region.into(), color),
                    )*
                }
            }

            fn stroke(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>, width: f64) {
                let brush = brush.make_brush(self, || shape.bounding_box());
                match (&mut *self.dispatch, &*brush.dispatch) {
                    $(
                        $(#[$meta])*
                        (ContextDispatch::$name(ctx), BrushDispatch::$name(brush)) => {
                            ctx.stroke(shape, brush, width)
                        },
                    )*
                    _ => unreachable!(),
                }
            }

            fn stroke_styled(
                &mut self,
                shape: impl Shape,
                brush: &impl IntoBrush<Self>,
                width: f64,
                style: &StrokeStyle,
            ) {
                let brush = brush.make_brush(self, || shape.bounding_box());
                match (&mut *self.dispatch, &*brush.dispatch) {
                    $(
                        $(#[$meta])*
                        (ContextDispatch::$name(ctx), BrushDispatch::$name(brush)) => {
                            ctx.stroke_styled(shape, brush, width, style)
                        },
                    )*
                    _ => unreachable!(),
                }
            }

            fn fill(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
                let brush = brush.make_brush(self, || shape.bounding_box());
                match (&mut *self.dispatch, &*brush.dispatch) {
                    $(
                        $(#[$meta])*
                        (ContextDispatch::$name(ctx), BrushDispatch::$name(brush)) => {
                            ctx.fill(shape, brush)
                        },
                    )*
                    _ => unreachable!(),
                }
            }

            fn fill_even_odd(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
                let brush = brush.make_brush(self, || shape.bounding_box());
                match (&mut *self.dispatch, &*brush.dispatch) {
                    $(
                        $(#[$meta])*
                        (ContextDispatch::$name(ctx), BrushDispatch::$name(brush)) => {
                            ctx.fill_even_odd(shape, brush)
                        },
                    )*
                    _ => unreachable!(),
                }
            }

            fn clip(&mut self, shape: impl Shape) {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => ctx.clip(shape),
                    )*
                }
            }

            fn text(&mut self) -> &mut Self::Text {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => {
                            ctx.text()
                        },
                    )*
                }
            }

            fn draw_text(&mut self, layout: &Self::TextLayout, pos: impl Into<Point>) {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => ctx.draw_text(layout, pos.into()),
                    )*
                }
            }

            fn save(&mut self) -> Result<(), Error> {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => ctx.save(),
                    )*
                }
            }

            fn restore(&mut self) -> Result<(), Error> {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => ctx.restore(),
                    )*
                }
            }

            fn finish(&mut self) -> Result<(), Error> {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => ctx.finish(),
                    )*
                }
            }

            fn transform(&mut self, transform: Affine) {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => ctx.transform(transform),
                    )*
                }
            }

            fn make_image(
                &mut self,
                width: usize,
                height: usize,
                buf: &[u8],
                format: ImageFormat,
            ) -> Result<Self::Image, Error> {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => {
                            let img = ctx.make_image(width, height, buf, format)?;
                            Ok(ImageDispatch::$name(img).into())
                        }
                    )*
                }
            }

            fn draw_image(
                &mut self,
                image: &Self::Image,
                dst_rect: impl Into<Rect>,
                interp: InterpolationMode,
            ) {
                match (&mut *self.dispatch, &*image.dispatch) {
                    $(
                        $(#[$meta])*
                        (ContextDispatch::$name(ctx), ImageDispatch::$name(img)) => {
                            ctx.draw_image(img, dst_rect.into(), interp)
                        }
                    )*
                    _ => unreachable!(),
                }
            }

            fn draw_image_area(
                &mut self,
                image: &Self::Image,
                src_rect: impl Into<Rect>,
                dst_rect: impl Into<Rect>,
                interp: InterpolationMode,
            ) {
                match (&mut *self.dispatch, &*image.dispatch) {
                    $(
                        $(#[$meta])*
                        (ContextDispatch::$name(ctx), ImageDispatch::$name(img)) => {
                            ctx.draw_image_area(
                                img,
                                src_rect.into(),
                                dst_rect.into(),
                                interp
                            )
                        }
                    )*
                    _ => unreachable!(),
                }
            }

            fn capture_image_area(&mut self, src_rect: impl Into<Rect>) -> Result<Self::Image, Error> {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => {
                            let img = ctx.capture_image_area(src_rect.into())?;
                            Ok(ImageDispatch::$name(img).into())
                        }
                    )*
                }
            }

            fn blurred_rect(&mut self, rect: Rect, blur_radius: f64, brush: &impl IntoBrush<Self>) {
                let brush = brush.make_brush(self, || rect);
                match (&mut *self.dispatch, &*brush.dispatch) {
                    $(
                        $(#[$meta])*
                        (ContextDispatch::$name(ctx), BrushDispatch::$name(brush)) => {
                            ctx.blurred_rect(rect, blur_radius, brush)
                        },
                    )*
                    _ => unreachable!(),
                }
            }

            fn current_transform(&self) -> Affine {
                match &*self.dispatch {
                    $(
                        $(#[$meta])*
                        ContextDispatch::$name(ctx) => ctx.current_transform(),
                    )*
                }
            }
        }

        impl piet::IntoBrush<RenderContext<'_, '_>> for Brush {
            fn make_brush<'a>(
                &'a self,
                _piet: &mut RenderContext<'_, '_>,
                _bbox: impl FnOnce() -> Rect,
            ) -> Cow<'a, <RenderContext<'_, '_> as piet::RenderContext>::Brush> {
                Cow::Borrowed(self)
            }
        }

        impl piet::Image for Image {
            fn size(&self) -> Size {
                match &*self.dispatch {
                    $(
                        $(#[$meta])*
                        ImageDispatch::$name(image) => image.size(),
                    )*
                }
            }
        }
    }
}

make_dispatch! {
    #[cfg(feature = "wgpu")]
    Wgpu(
        wgpu_backend::Display,
        wgpu_backend::Surface,
        wgpu_backend::RenderContext<'dsp, 'surf>,
        piet_wgpu::Brush<wgpu_backend::WgpuInnards>,
        piet_wgpu::Image<wgpu_backend::WgpuInnards>
    ),

    #[cfg(all(feature = "gl", not(target_family = "wasm")))]
    DesktopGl(
        desktop_gl::Display,
        desktop_gl::Surface,
        desktop_gl::RenderContext<'dsp, 'surf>,
        piet_glow::Brush<glow::Context>,
        piet_glow::Image<glow::Context>
    ),

    SwRast(
        swrast::Display,
        swrast::Surface,
        swrast::RenderContext<'dsp, 'surf>,
        swrast::Brush,
        swrast::Image
    ),
}

/// A wrapper around an error that doesn't expose it to public API.
struct LibraryError<E>(E);

impl<E: fmt::Debug> fmt::Debug for LibraryError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<E: fmt::Display> fmt::Display for LibraryError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for LibraryError<E> {}

trait ResultExt<T, E: std::error::Error + 'static> {
    fn piet_err(self) -> Result<T, Error>;
}

impl<T, E: std::error::Error + 'static> ResultExt<T, E> for Result<T, E> {
    fn piet_err(self) -> Result<T, Error> {
        self.map_err(|e| Error::BackendError(Box::new(LibraryError(e))))
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
