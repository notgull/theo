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

//! A generic [`piet`] context for operating on windowing systems.
//!
//! This create aims to provide an easy, performant drawing framework that can be easily
//! integrated into all windowing systems. The constructors for the contexts take traits
//! from [`raw-window-handle`] for easy integration.
//!
//! `theo` prioritizes versatility and performance. By default, `theo` uses an optimized
//! OpenGL backend for rendering. If OpenGl is not available, `theo` will fall back to
//! software rendering.
//!
//! # Usage
//!
//! First, users must create a [`Display`], which represents the root display of the system.
//! From here, users should create [`Surface`]s, which represent drawing areas. Finally,
//! a [`Surface`] can be used to create the [`RenderContext`] type, which is used to draw.

#[cfg(feature = "gl")]
mod desktop_gl;
mod swrast;
mod text;

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
    /// # Safety
    ///
    /// The `window` handle must be a valid `window` that isn't currently suspended. The
    /// `width` and `height` parameters aren't necessarily required to be correct, but
    /// it's recommended that they are in order to avoid visual bugs.
    pub unsafe fn make_surface(
        &mut self,
        window: impl HasRawWindowHandle,
        width: u32,
        height: u32,
    ) -> Result<Surface, Error> {
        self.make_surface_from_raw(window.raw_window_handle(), width, height)
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
            /// # Safety
            ///
            ///
            pub unsafe fn make_surface_from_raw(
                &mut self,
                window: RawWindowHandle,
                width: u32,
                height: u32,
            ) -> Result<Surface, Error> {
                match &mut *self.dispatch {
                    $(
                        $(#[$meta])*
                        DisplayDispatch::$name(display) => {
                            let surface = display.make_surface(window, width, height)?;
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
