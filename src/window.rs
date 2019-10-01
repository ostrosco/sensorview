use crate::camera::{CameraData, CameraWindow};
use crossbeam::Receiver;
use glium::glutin::{self, Event, WindowEvent};
use glium::{Display, Surface};
use imgui::{self, Context, FontConfig, FontSource, Ui};
use imgui_glium_renderer::Renderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};

/// A trait for sensor windows so that eventually the main window can simply
/// keep a list of all active sensor windows and update them without having
/// to care about the types of sensors.
pub trait Renderable {
    type Item;

    fn render(&mut self, ui: &Ui, display: &Display, renderer: &mut Renderer,
              receiver: &mut Receiver<Self::Item>);
}

/// A list of receivers for sensor data. Currently we can only receive Camera
/// information.
pub struct SensorData {
    pub camera: Option<Receiver<CameraData>>,
}

impl SensorData {
    pub fn new() -> Self {
        Self { camera: None }
    }
}

pub struct SensorWindow {
    pub events_loop: glutin::EventsLoop,
    pub display: Display,
    pub imgui: Context,
    pub platform: WinitPlatform,
    pub renderer: Renderer,
    pub font_size: f32,
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
            font_size,
        }
    }

    /// Starts the rendering loop for the window. This will check for
    /// any new data received from the sensors and update any windows
    /// with new information.
    pub fn render(self, mut sensor_data: SensorData) {
        let SensorWindow {
            mut events_loop,
            mut platform,
            display,
            mut imgui,
            mut renderer,
            ..
        } = self;
        let gl_window = display.gl_window();
        let window = gl_window.window();
        let mut run = true;
        let mut camera_window = CameraWindow::new();

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

            // Check for camera data if there is a receiver set up. 
            //
            // TODO: Currently this assumes that there is only one camera
            // sensor. This should allow an arbitrary number of sensors of
            // any type.
            if let Some(ref mut camera) = sensor_data.camera {
                camera_window.render(
                    &ui,
                    &display,
                    &mut renderer,
                    camera,
                );
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
