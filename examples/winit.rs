//! Basic usage of `theo`, using `winit` as the windowing system.

use std::time::{Duration, Instant};

use piet::RenderContext as _;
use piet::kurbo::{BezPath, Point, Affine};
use theo::{Display, RenderContext};

use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

fn main() -> ! {
    env_logger::init();

    // Function that creates a window builder for our window.
    let window_builder = || {
        WindowBuilder::new()
            .with_title("theo winit example")
            .with_inner_size(LogicalSize::new(800.0, 600.0))
            .with_transparent(true)
    };

    // Create the event loop.
    let event_loop = EventLoop::new();
    let mut window = None;

    // Create the display.
    let mut display = {
        let mut display = Display::builder().transparent(true);

        // Uncomment this to enforce software rendering.
        //display = display.force_swrast(true);

        // On Windows, we should set up a window first. Otherwise, the GL features
        // we want to use won't be available.
        #[cfg(windows)]
        {
            let start = window_builder()
                .build(&event_loop)
                .expect("Failed to create window");
            display = display.window(&start);
            window = Some(start);
        }

        // On X11, make sure to set the error handling context. theo prefers EGL over
        // GLX, but if we fall back to GLX we'll need to set up a context.
        #[cfg(x11_platform)]
        {
            display = display.glx_error_hook(winit::platform::x11::register_xlib_error_hook);
        }

        unsafe {
            display
                .build(&*event_loop)
                .expect("Failed to create display")
        }
    };

    let mut state = None;
    let framerate = Duration::from_millis({
        let fraction = 1.0 / 60.0;
        let millis_per_frame = fraction * 1_000.0;
        millis_per_frame as u64
    });
    let mut next_frame = Instant::now() + framerate;

    // Consistent drawing properties.
    let star = generate_five_pointed_star(
        (0.0, 0.0).into(),
        75.0,
        150.0
    ); 
    let mut fill_brush = None;
    let mut stroke_brush = None;
    let mut tick = 0;

    // Function for drawing, called every frame.
    let mut draw = move |render_context: &mut RenderContext<'_, '_>| {
        use piet::Color;

        // Clear the screen.
        render_context.clear(None, Color::SILVER);

        // Fill in the star.
        let fill_brush = fill_brush.get_or_insert_with(|| {
            render_context.solid_brush(Color::RED)
        });

        let rotation = {
            let angle_rad = (tick as f64) * 0.02;
            Affine::rotate(angle_rad)
        };
        let translation = Affine::translate((200.0, 200.0));

        render_context.with_save(|render_context| {
            render_context.transform(translation * rotation);
            render_context.fill(&star, fill_brush);

            // Stroke the star.
            let stroke_brush = stroke_brush.get_or_insert_with(|| {
                render_context.solid_brush(Color::BLACK)
            });

            render_context.stroke(&star, stroke_brush, 5.0);

            Ok(())
        })?;

        // Propogate any errors.
        tick += 1;
        render_context.finish()?;
        render_context.status()
    };

    event_loop.run(move |event, elwt, control_flow| {
        control_flow.set_wait_until(next_frame);

        match event {
            Event::Resumed => {
                // Create a window (if we haven't already) and a theo surface.
                let window = window.take().unwrap_or_else(|| {
                    let mut window_builder = window_builder();

                    // Use the context we created earlier to figure out parameters.
                    if !display.supports_transparency() {
                        window_builder = window_builder.with_transparent(false);
                    }

                    #[cfg(x11_platform)]
                    {
                        use winit::platform::x11::WindowBuilderExtX11;

                        if let Some(visual) = display.x11_visual() {
                            window_builder = window_builder.with_x11_visual(visual.as_ptr());
                        }
                    }

                    window_builder.build(elwt).expect("Failed to create window")
                });

                // Create a new theo surface.
                let size = window.inner_size();
                let surface = unsafe {
                    display
                        .make_surface(&window, size.width, size.height)
                        .expect("Failed to create surface")
                };

                // Save the state.
                state = Some((window, surface));
            }

            Event::Suspended => {
                // On Android, this means that we have to destroy the surface.
                state.take();
            }

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => control_flow.set_exit(),

            Event::RedrawEventsCleared => {
                // Use the surface to draw.
                if let Some((window, surface)) = &mut state {
                    let size = window.inner_size();
                    let mut render_context =
                        RenderContext::new(&mut display, surface, size.width, size.height)
                            .expect("Failed to create render context");

                    // Call to the actual drawing function.
                    draw(&mut render_context).expect("Failed to draw");
                }

                next_frame += framerate;
                control_flow.set_wait_until(next_frame);
            }

            _ => {}
        }
    });
}

fn generate_five_pointed_star(center: Point, inner_radius: f64, outer_radius: f64) -> BezPath {
    let point_from_polar = |radius: f64, angle: f64| {
        let x = center.x + radius * angle.cos();
        let y = center.y + radius * angle.sin();
        Point::new(x, y)
    };

    let one_fifth_circle = std::f64::consts::PI * 2.0 / 5.0;

    let outer_points = (0..5).map(|i| point_from_polar(outer_radius, one_fifth_circle * i as f64));
    let inner_points = (0..5).map(|i| {
        point_from_polar(
            inner_radius,
            one_fifth_circle * i as f64 + one_fifth_circle / 2.0,
        )
    });
    let mut points = outer_points.zip(inner_points).flat_map(|(a, b)| [a, b]);

    // Set up the path.
    let mut path = BezPath::new();
    path.move_to(points.next().unwrap());

    // Add the points to the path.
    for point in points {
        path.line_to(point);
    }

    // Close the path.
    path.close_path();
    path
}