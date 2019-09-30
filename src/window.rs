use crate::camera::CameraData;
use crossbeam::Receiver;
use glium::glutin::{self, Event, WindowEvent};
use glium::{
    backend::Facade,
    texture::{ClientFormat, RawImage2d},
    Texture2d,
};
use glium::{Display, Surface};
use imgui::{
    self, im_str, Condition, Context, FontConfig, FontSource, Image, TextureId,
    Ui, Window,
};
use imgui_glium_renderer::Renderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::borrow::Cow;
use std::rc::Rc;

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
    /// Initializes the window for displaying multiple sensors.
    pub fn new() -> Self {
        let events_loop = glutin::EventsLoop::new();
        let context = glutin::ContextBuilder::new().with_vsync(true);
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
        let mut texture_id = None;
        let mut run = true;

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

            // Check for camera data if there is a receiver set up. Currently
            // this assumes that there is only one sensor. This limitation
            // should be lifted eventually.
            if let Some(ref mut camera) = sensor_data.camera {
                SensorWindow::render_camera_window(
                    &ui,
                    &display,
                    &mut renderer,
                    &camera.try_recv().ok(),
                    &mut texture_id,
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

    /// Renders the data received from the camera sensor. This currently
    /// assumes RGB data format.
    fn render_camera_window(
        ui: &Ui,
        display: &Display,
        renderer: &mut Renderer,
        camera_data: &Option<CameraData>,
        texture_id: &mut Option<TextureId>,
    ) {
        // If we've received new camera data, update the texture. We also need
        // to check if there is an existing texture ahead of time so we can
        // reuse the texture instead of creating a new one each time.
        if let Some(camera_data) = camera_data {
            let image_frame = Some(RawImage2d {
                data: Cow::Owned(camera_data.image_bytes.clone()),
                width: camera_data.width as u32,
                height: camera_data.height as u32,
                format: ClientFormat::U8U8U8,
            })
            .unwrap();
            let gl_texture = Texture2d::new(display.get_context(), image_frame)
                .expect("Couldn't create new texture");
            if let Some(tex_id) = texture_id {
                renderer.textures().replace(*tex_id, Rc::new(gl_texture));
            } else {
                *texture_id =
                    Some(renderer.textures().insert(Rc::new(gl_texture)));
            }
        }

        // We call this each iteration of the SensorWindow, so we need to make
        // sure we draw the window even if we didn't receive camera data on
        // this iteration. However, we currently do not draw a window unless
        // we've received our first sample from the camera.
        if let Some(tex_id) = texture_id {
            Window::new(im_str!("Camera"))
                .size([800.0, 600.0], Condition::FirstUseEver)
                .build(ui, || {
                    Image::new(*tex_id, [640.0, 480.0])
                        .uv0([1.0, 1.0])
                        .uv1([0.0, 0.0])
                        .build(&ui);
                });
        }
    }
}
