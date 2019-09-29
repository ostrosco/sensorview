use byteorder::{LittleEndian, ReadBytesExt};
use glium::glutin;
use glium::{
    backend::Facade,
    texture::{ClientFormat, PixelValue, RawImage2d},
    Texture2d,
};
use glium::{Display, Surface};
use image::jpeg::JPEGDecoder;
use image::ImageDecoder;
use imgui::{
    self, im_str, Condition, Context, FontConfig, FontSource, Image, Window,
};
use imgui_glium_renderer::Renderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::borrow::Cow;
use std::io::{self, Cursor, Read};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

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

struct SensorData<'a, T>
where
    T: Clone + PixelValue,
{
    camera: Option<RawImage2d<'a, T>>,
}

impl<'a, T> SensorData<'a, T>
where
    T: Clone + PixelValue,
{
    pub fn new() -> Self {
        Self { camera: None }
    }
}

fn handle_image_stream<'a>(
    mut stream: TcpStream,
    sensor_data: Arc<Mutex<SensorData<'a, u8>>>,
) -> io::Result<()> {
    loop {
        let size = stream.read_u32::<LittleEndian>()? as usize;
        let mut bytes = vec![0; size];
        stream.read_exact(&mut bytes[..])?;
        let bytes = Cursor::new(bytes);
        let decoder = JPEGDecoder::new(bytes).expect("Couldn't make decoder");
        let (width, height) = decoder.dimensions();
        let image = decoder.read_image().expect("Couldn't read image");
        let raw = RawImage2d {
            data: Cow::Owned(image),
            width: width as u32,
            height: height as u32,
            format: ClientFormat::U8U8U8,
        };
        let mut sensor_data = sensor_data.lock().unwrap();
        sensor_data.camera = Some(raw);
    }
}

fn render<'a>(sensor_data: Arc<Mutex<SensorData<'a, u8>>>) {
    let System {
        display,
        mut imgui,
        platform,
        mut renderer,
        ..
    } = init();

    let gl_window = display.gl_window();
    let window = gl_window.window();
    let camera_enabled = true;
    let mut cam_tex_id = None;

    loop {
        let io = imgui.io_mut();
        platform
            .prepare_frame(io, &window)
            .expect("Failed to start frame.");
        let ui = imgui.frame();

        if camera_enabled {
            let camera;
            {
                let mut sensor_data = sensor_data.lock().unwrap();
                camera = sensor_data.camera.take();
            }
            if let Some(cam) = camera {
                let gl_texture = Texture2d::new(display.get_context(), cam)
                    .expect("Couldn't create new texture");
                if let Some(tex_id) = cam_tex_id {
                    renderer.textures().replace(tex_id, Rc::new(gl_texture));
                } else {
                    cam_tex_id =
                        Some(renderer.textures().insert(Rc::new(gl_texture)));
                }
            }

            if let Some(tex_id) = cam_tex_id {
                Window::new(im_str!("Camera"))
                    .size([800.0, 600.0], Condition::FirstUseEver)
                    .build(&ui, || {
                        ui.text(im_str!("Camera"));
                        Image::new(tex_id, [640.0, 480.0]).build(&ui);
                    });
            }
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

fn main() -> io::Result<()> {
    let sensor_data = Arc::new(Mutex::new(SensorData::new()));
    let sensor_render = sensor_data.clone();
    thread::spawn(move || {
        render(sensor_render);
    });
    let listener = TcpListener::bind("0.0.0.0:8001")?;
    for stream in listener.incoming() {
        handle_image_stream(stream?, sensor_data.clone())?;
    }
    Ok(())
}
