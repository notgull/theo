# theo

A generic [`piet`] rendering context for all windowing and graphics backends.

This project is hosted on [`SourceHut`](https://git.sr.ht/~notgull/theo).
The GitHub mirror is kept for convenience.

Windowing frameworks like [`winit`] do not provide a way to draw into them by
default. This decision is intentional; it allows the user to choose which
graphics backend that they'd like to use, and also makes maintaining the
windowing code much simpler. For games (what [`winit`] was originally designed
for), usually a 3D rendering context like [`wgpu`] or [`glow`] would be used in
this case. However, GUI applications will need a 2D vector graphics context.

[`piet`] is a 2D graphics abstraction that can be used with many different
graphics backends. However, [`piet`]'s default implementation, [`piet-common`],
is difficult to integrate with windowing systems other than [`druid-shell`],
which doesn't support many operations that other windowing systems support.
[`theo`] aims to bridge this gap by providing a generic [`piet`] rendering
context that easily integrates with windowing systems.

Rather than going through drawing APIs like [`cairo`] and DirectX, `theo`
directly uses GPU APIs in order to render to the window. This allows for better
performance and greater flexibility, and also ensures that much of the rendering
logic is safe. This also reduces the number of dynamic dependencies that your
final program needs to rely on.

`theo` prioritizes versatility and performance. By default, `theo` uses an
optimized GPU backend for rendering. If the GPU is not available, `theo` will
fall back to software rendering.

## Source Code

The canonical code for this repository is kept on our [Git Forge]. For
convenience, a mirror is kept on [GitHub].

[Git Forge]: https://src.notgull.net/notgull/theo
[GitHub]: https://github.com/notgull/theo

## Usage Example

First, users must create a `Display`, which represents the root display of the
system. From here, users should create `Surface`s, which represent drawing
areas. Finally, a `Surface` can be used to create the `RenderContext` type,
which is used to draw.

```rust
use piet::{RenderContext as _, kurbo::Circle};
use theo::{Display, Surface, RenderContext};

// Create a display using a display handle from your windowing framework.
// `my_display` is used as a stand-in for the root of your display system.
// It must implement `raw_window_handle::HasRawDisplayHandle`.
let mut display = unsafe {
    Display::builder()
        .build(&my_display)
        .expect("failed to create display")
};

// Create a surface using a window handle from your windowing framework.
// `window` is used as a stand-in for a window in your display system.
// It must implement `raw_window_handle::HasRawWindowHandle`.
let surface_future = unsafe {
    display.make_surface(
        &window,
        window.width(),
        window.height()
    )
};

// make_surface returns a future that needs to be polled.
let mut surface = surface_future.await.expect("failed to create surface");

// Set up drawing logic.
surface.on_draw(move || async move {
    // Create the render context.
    let mut ctx = RenderContext::new(
        &mut display,
        &mut surface,
        window.width(),
        window.height()
    ).expect("failed to create render context");

    // Clear the screen and draw a circle.
    ctx.clear(None, piet::Color::WHITE);
    ctx.fill(
        &Circle::new((200.0, 200.0), 50.0),
        &piet::Color::RED
    );

    // Finish drawing.
    ctx.finish().expect("failed to finish drawing");

    // If you don't have any other windows to draw, make sure the windows are
    // presented.
    display.present().await;
});
```

See the documentation for the [`piet`] crate for more information on how to use
the drawing API.

## Backends

As of the time of writing, `theo` supports the following backends:

- [`wgpu`] backend (enabled with the `wgpu` feature), which uses the
  [`piet-wgpu`] crate to render to the window. This backend supports all of the
  graphics APIs that `wgpu` supports, including Vulkan, Metal, and DirectX 11/12.
- [`glow`] backend (enabled with the `gl` feature), which uses the [`piet-glow`]
  crate to render to the window. [`glutin`] is used on desktop platforms to
  create the OpenGL context, and [`glow`] is used to interact with the OpenGL
  API. This backend supports OpenGL 3.2 and above.
- A software rasterization backend. [`tiny-skia`] is used to render to a bitmap,
  and then [`softbuffer`] is used to copy the bitmap to the window. This backend
  is enabled by default and is used when no other backend is available.

## Performance

As `theo` implements most of its own rendering logic, this can lead to serious
performance degradations if used improperly, especially on the software
rasterization backend. In some cases, compiling `theo` on Debug Mode rather than
Release Mode can half the frame rate of the application. If you are experiencing
low frame rates with `theo`, make sure that you are compiling it on Release Mode.

In addition, gradient brushes are optimized in such a way that the actual
gradient needs to be computed only once. However, this means that, if you
re-instantiate the brush every time, the gradient will be re-computed every
time. This can lead to serious performance degradations even on
hardware-accelerated backends. The solution is to cache the brushes that you
use. For instance, instead of doing this:

```rust
let gradient = /* ... */;
surface.on_draw(|| {
    let mut ctx = /* ... */;
    ctx.fill(&Circle::new((200.0, 200.0), 50.0), &gradient);
})
```

Do this, making sure to cache the gradient brush:

```rust
let gradient = /* ... */;
let mut gradient_brush = None;
surface.on_draw(|| {
    let mut ctx = /* ... */;
    let gradient_brush = gradient_brush.get_or_insert_with(|| {
        ctx.gradient_brush(gradient.clone()).unwrap()
    });
    ctx.fill(&Circle::new((200.0, 200.0), 50.0), gradient_brush);
})
```

`theo` explicitly opts into a thread-unsafe model. Not only is thread-unsafe code
more performant, but these API types are usually thread-unsafe anyways.

[`cairo`]: https://www.cairographics.org/
[`softbuffer`]: https://crates.io/crates/softbuffer
[`tiny-skia`]: https://crates.io/crates/tiny-skia
[`piet-wgpu`]: https://crates.io/crates/piet-wgpu
[`piet-glow`]: https://crates.io/crates/piet-glow
[`glutin`]: https://crates.io/crates/glutin
[`piet`]: https://crates.io/crates/piet
[`piet-common`]: https://crates.io/crates/piet-common
[`winit`]: https://crates.io/crates/winit
[`wgpu`]: https://crates.io/crates/wgpu
[`glow`]: https://crates.io/crates/glow
[`theo`]: https://crates.io/crates/theo

## License

`theo` is free software: you can redistribute it and/or modify it under the
terms of either:

- GNU Lesser General Public License as published by the Free Software Foundation,
either version 3 of the License, or (at your option) any later version.
- Mozilla Public License as published by the Mozilla Foundation, version 2.

`theo` is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR
PURPOSE. See the GNU Lesser General Public License or the Mozilla Public License
for more details.

You should have received a copy of the GNU Lesser General Public License and the
Mozilla Public License along with `theo`. If not, see
<https://www.gnu.org/licenses/>.
