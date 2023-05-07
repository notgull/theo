//! Basic usage of `theo`, using `winit` as the windowing system.

use std::time::{Duration, Instant};

use piet::kurbo::{Affine, BezPath, Point, Rect, Vec2};
use piet::{FontFamily, GradientStop, RenderContext as _, Text, TextLayoutBuilder};
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
        let mut display = Display::builder();

        // Uncomment this to force software rendering.
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
    let star = generate_five_pointed_star((0.0, 0.0).into(), 75.0, 150.0);

    let mut fill_brush = None;
    let mut stroke_brush = None;
    let mut radial_gradient = None;
    let mut linear_gradient = None;
    let mut tick = 0;
    let mut image_handle = None;

    let mut last_second = Instant::now();
    let mut num_frames = 0;
    let mut current_fps = None;

    // Get the test image at $CRATE_ROOT/examples/assets/test-image.png
    let manifest_root = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest_root).join("examples/assets/test-image.png");
    let image = image::open(path).expect("Failed to open image").to_rgba8();
    let image_size = image.dimensions();
    let image_data = image.into_raw();

    // Function for drawing, called every frame.
    let mut draw = move |render_context: &mut RenderContext<'_, '_>| {
        use piet::Color;

        // Clear the screen.
        render_context.clear(None, Color::rgb8(0x87, 0xce, 0xeb));

        // Make the brushes.
        let fill_brush = fill_brush.get_or_insert_with(|| render_context.solid_brush(Color::RED));
        let stroke_brush =
            stroke_brush.get_or_insert_with(|| render_context.solid_brush(Color::BLACK));
        let radial_gradient = radial_gradient.get_or_insert_with(|| {
            let grad = piet::FixedRadialGradient {
                center: Point::new(0.0, 0.0),
                origin_offset: Vec2::new(0.0, 0.0),
                radius: 150.0,
                stops: vec![
                    GradientStop {
                        pos: 0.0,
                        color: piet::Color::LIME,
                    },
                    GradientStop {
                        pos: 0.5,
                        color: piet::Color::MAROON,
                    },
                    GradientStop {
                        pos: 1.0,
                        color: piet::Color::NAVY,
                    },
                ],
            };

            render_context.gradient(grad).unwrap()
        });
        let linear_gradient = linear_gradient.get_or_insert_with(|| {
            const RAINBOW: &[piet::Color] = &[
                piet::Color::RED,
                piet::Color::rgb8(0xff, 0x7f, 0x00),
                piet::Color::YELLOW,
                piet::Color::GREEN,
                piet::Color::BLUE,
                piet::Color::rgb8(0x4b, 0x00, 0x82),
                piet::Color::rgb8(0x94, 0x00, 0xd3),
            ];

            let grad = piet::FixedLinearGradient {
                start: Point::new(0.0, 0.0),
                end: Point::new(50.0, 150.0),
                stops: RAINBOW
                    .iter()
                    .enumerate()
                    .map(|(i, &color)| GradientStop {
                        pos: i as f32 / (RAINBOW.len() - 1) as f32,
                        color,
                    })
                    .collect(),
            };

            render_context.gradient(grad).unwrap()
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
            render_context.stroke(&star, stroke_brush, 5.0);

            Ok(())
        })?;

        let translation = Affine::translate((550.0, 200.0));
        let rotation = {
            let angle_rad = (tick as f64) * 0.04;
            Affine::rotate(angle_rad)
        };
        let scaling = Affine::scale(0.75);

        render_context.with_save(|render_context| {
            render_context.transform(translation * rotation * scaling);
            render_context.fill(&star, radial_gradient);

            // Stroke the star.
            render_context.stroke(&star, stroke_brush, 5.0);

            Ok(())
        })?;

        // Create an image and draw with it.
        let image_handle = image_handle
            .get_or_insert_with(|| {
                render_context.make_image(
                    image_size.0 as _,
                    image_size.1 as _,
                    &image_data,
                    piet::ImageFormat::RgbaSeparate,
                )
            })
            .as_ref()
            .unwrap();

        let scale = |x: f64| (x + 1.0) * 25.0;
        let posn_shift_x = scale(((tick as f64) / 25.0).cos());
        let posn_shift_y = scale(((tick as f64) / 25.0).sin());
        let posn_x = 400.0 + posn_shift_x;
        let posn_y = 400.0 + posn_shift_y;

        let size_shift_x = 50.0 + scale(((tick as f64) / 50.0).sin());
        let size_shift_y = 50.0 + scale(((tick as f64) / 50.0).cos());

        render_context.draw_image(
            image_handle,
            Rect::new(posn_x, posn_y, posn_x + size_shift_x, posn_y + size_shift_y),
            piet::InterpolationMode::Bilinear,
        );

        // Also draw a subregion of the image.
        let out_rect = Rect::new(100.0, 400.0, 200.0, 500.0);
        render_context.draw_image_area(
            image_handle,
            Rect::new(
                25.0 + posn_shift_x,
                25.0 + posn_shift_y,
                75.0 + posn_shift_x,
                75.0 + posn_shift_y,
            ),
            out_rect,
            piet::InterpolationMode::Bilinear,
        );
        render_context.stroke(out_rect, stroke_brush, 3.0);

        // Draw a linear gradient.
        let rect = Rect::new(0.0, 0.0, 50.0, 150.0);
        render_context
            .with_save(|render_context| {
                let transform = Affine::translate((650.0, 275.0));
                render_context.transform(transform);

                // Draw the gradient.
                render_context.fill(rect, linear_gradient);

                // Draw a border.
                render_context.stroke(rect, stroke_brush, 5.0);

                Ok(())
            })
            .unwrap();

        // Update the FPS counter.
        num_frames += 1;
        let now = Instant::now();
        if now - last_second >= Duration::from_secs(1) {
            let fps_string = format!("Frames per Second: {num_frames}");
            let fps_text = render_context
                .text()
                .new_text_layout(fps_string)
                .font(FontFamily::SERIF, 24.0)
                .text_color(piet::Color::rgb8(0x11, 0x22, 0x22))
                .build()
                .unwrap();

            current_fps = Some(fps_text);

            num_frames = 0;
            last_second = now;
        }

        // Draw the FPS counter.
        if let Some(fps_text) = &current_fps {
            render_context.draw_text(fps_text, Point::new(10.0, 10.0));
        }

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
                let surface = futures_lite::future::block_on(unsafe {
                    display.make_surface(&window, size.width, size.height)
                })
                .expect("Failed to create surface");

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
                if Instant::now() < next_frame {
                    return;
                }

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
