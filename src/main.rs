use crate::camera::CameraData;
use crossbeam::{unbounded, Receiver};
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
use std::io;
use std::rc::Rc;
use std::thread;

mod camera;

pub struct System {
    pub events_loop: glutin::EventsLoop,
    pub display: Display,
    pub imgui: Context,
    pub platform: WinitPlatform,
    pub renderer: Renderer,
    pub font_size: f32,
}

fn init() -> System {
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

    System {
        events_loop,
        display,
        imgui,
        platform,
        renderer,
        font_size,
    }
}

struct SensorData {
    camera: Option<Receiver<CameraData>>,
}

impl SensorData {
    pub fn new() -> Self {
        Self { camera: None }
    }
}

fn render(mut sensor_data: SensorData) {
    let System {
        mut events_loop,
        display,
        mut imgui,
        mut platform,
        mut renderer,
        ..
    } = init();

    let gl_window = display.gl_window();
    let window = gl_window.window();
    let mut texture_id = None;
    let mut run = true;

    while run {
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
        if let Some(ref mut camera) = sensor_data.camera {
            render_camera(
                &ui,
                &display,
                &mut renderer,
                &camera.try_recv().ok(),
                &mut texture_id,
            );
        }

        let mut target = display.draw();
        target.clear_color_srgb(0.8, 0.8, 0.8, 1.0);
        platform.prepare_render(&ui, &window);
        let draw_data = ui.render();
        renderer
            .render(&mut target, draw_data)
            .expect("Couldn't render");
        target.finish().expect("Failed to swap buffers");
    }
}

pub fn render_camera(
    ui: &Ui,
    display: &Display,
    renderer: &mut Renderer,
    camera_data: &Option<CameraData>,
    texture_id: &mut Option<TextureId>,
) {
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
            *texture_id = Some(renderer.textures().insert(Rc::new(gl_texture)));
        }
    }

    if let Some(tex_id) = texture_id {
        Window::new(im_str!("Camera"))
            .size([800.0, 600.0], Condition::FirstUseEver)
            .build(ui, || {
                ui.text(im_str!("Camera"));
                Image::new(*tex_id, [640.0, 480.0]).build(&ui);
            });
    }
}

fn main() -> io::Result<()> {
    let (camera_tx, camera_rx) = unbounded();
    let mut sensor_data = SensorData::new();
    sensor_data.camera = Some(camera_rx);

    thread::spawn(move || -> io::Result<()> {
        let camera = camera::Camera::new(camera_tx);
        camera.start("0.0.0.0:8001".parse().unwrap())?;
        Ok(())
    });

    render(sensor_data);
    Ok(())
}
