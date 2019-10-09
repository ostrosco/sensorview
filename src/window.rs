use crate::camera::CameraConfig;
use crate::lidar::LidarConfig;
use glium::glutin::{self, Event, WindowEvent};
use glium::{Display, Surface};
use imgui::{self, im_str, Context, FontConfig, FontSource, Ui, Window};
use imgui_glium_renderer::Renderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::io;
use std::thread::JoinHandle;

/// A trait for sensor windows so that eventually the main window can simply
/// keep a list of all active sensor windows and update them without having
/// to care about the types of sensors.
pub trait Renderable {
    fn render(&mut self, ui: &Ui, display: &Display, renderer: &mut Renderer);
}

pub struct SensorWindow {
    events_loop: glutin::EventsLoop,
    display: Display,
    imgui: Context,
    platform: WinitPlatform,
    renderer: Renderer,
    sensor_windows: Vec<Box<dyn Renderable>>,
    join_handles: Vec<JoinHandle<io::Result<()>>>,
    camera_config: CameraConfig,
    lidar_config: LidarConfig,
}

impl SensorWindow {
    /// Initializes a blank window for displaying multiple sensor windows
    pub fn new() -> Self {
        let events_loop = glutin::EventsLoop::new();
        let context = glutin::ContextBuilder::new().with_vsync(true);

        // TODO: Query screen resolution so the window size isn't hardcoded.
        let builder = glutin::WindowBuilder::new()
            .with_dimensions(glutin::dpi::LogicalSize::new(1920f64, 1080f64));
        let display = Display::new(builder, context, &events_loop)
            .expect("Could not create display.");
        let mut imgui = Context::create();
        imgui.set_ini_filename(None);
        let mut platform = WinitPlatform::init(&mut imgui);
        {
            let gl_window = display.gl_window();
            let window = gl_window.window();
            platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Rounded);
        }
        let hidpi_factor = platform.hidpi_factor();
        let font_size = (13.0 * hidpi_factor) as f32;
        imgui.fonts().add_font(&[
            FontSource::DefaultFontData {
                config: Some(FontConfig {
                    size_pixels: font_size,
                    ..FontConfig::default()
                }),
            },
            FontSource::TtfData {
                data: ttf_noto_sans::REGULAR,
                size_pixels: font_size,
                config: Some(FontConfig {
                    rasterizer_multiply: 1.75,
                    ..FontConfig::default()
                }),
            },
        ]);

        imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

        let renderer = Renderer::init(&mut imgui, &display)
            .expect("Failed to initialize renderer");

        Self {
            events_loop,
            display,
            imgui,
            platform,
            renderer,
            sensor_windows: Vec::new(),
            join_handles: Vec::new(),
            camera_config: CameraConfig::new(),
            lidar_config: LidarConfig::new(),
        }
    }

    /// Starts the rendering loop for the window. This will check for
    /// any new data received from the sensors and update any windows
    /// with new information.
    pub fn render(self) {
        let SensorWindow {
            mut events_loop,
            mut platform,
            display,
            mut imgui,
            mut renderer,
            mut sensor_windows,
            mut join_handles,
            mut camera_config,
            mut lidar_config,
            ..
        } = self;
        let gl_window = display.gl_window();
        let window = gl_window.window();
        let mut run = true;
        let mut selected_sensor = 0i32;

        // TODO: Right now we're manually creating a Camera window for testing
        // but eventually windows should be created by request from the user.
        // We need a way to create new windows from the user.

        while run {
            // Handle any close events for the window.
            events_loop.poll_events(|event| {
                platform.handle_event(imgui.io_mut(), &window, &event);

                if let Event::WindowEvent { event, .. } = event {
                    if let WindowEvent::CloseRequested = event {
                        run = false;
                    }
                }
            });

            let io = imgui.io_mut();
            platform
                .prepare_frame(io, &window)
                .expect("Failed to start frame.");
            let ui = imgui.frame();

            Window::new(im_str!("SensorView")).build(&ui, || {
                ui.text(im_str!("Create new:"));
                ui.list_box(
                    im_str!(""),
                    &mut selected_sensor,
                    &[im_str!("Camera"), im_str!("LIDAR"), im_str!("GPS")],
                    10,
                );
                camera_config.render_camera_modal(
                    &ui,
                    &mut join_handles,
                    &mut sensor_windows,
                );
                lidar_config.render_lidar_modal(
                    &ui,
                    &mut join_handles,
                    &mut sensor_windows,
                );
                if ui.button(im_str!("Configure sensor..."), [0.0, 0.0]) {
                    match selected_sensor {
                        0 => {
                            ui.open_popup(im_str!("Camera Configuration"));
                        }
                        1 => {
                            ui.open_popup(im_str!("LIDAR Configuration"));
                        }
                        _ => {
                            ui.text("Not supported yet");
                        }
                    }
                }
            });

            // Iterate over all created sensor windows and update them.
            for sensor_window in &mut sensor_windows {
                sensor_window.render(&ui, &display, &mut renderer);
            }

            // Once all the sensor windows are created and update them, we can
            // now draw them to the screen and start another iteration.
            let mut target = display.draw();
            target.clear_color_srgb(0.211, 0.223, 0.243, 1.0);
            platform.prepare_render(&ui, &window);
            let draw_data = ui.render();
            renderer
                .render(&mut target, draw_data)
                .expect("Couldn't render");
            target.finish().expect("Failed to swap buffers");
        }
    }
}
