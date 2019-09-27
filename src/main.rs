use byteorder::{LittleEndian, ReadBytesExt};
use glium::glutin::{self, Event, WindowEvent};
use glium::{
    backend::Facade,
    texture::{ClientFormat, RawImage2d},
    Texture2d,
};
use glium::{Display, Surface};
use image::ImageDecoder;
use image::jpeg::JPEGDecoder;
use imgui::{
    self, im_str, Condition, Context, FontConfig, FontSource, Image, Ui, Window,
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
        .with_dimensions(glutin::dpi::LogicalSize::new(1024f64, 768f64));
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

fn handle_image_stream(mut stream: TcpStream) -> io::Result<()> {
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

    loop {
        let size = stream.read_u32::<LittleEndian>()? as usize;
        let mut bytes = vec![0; size];
        stream.read_exact(&mut bytes[..])?;
        let mut bytes = Cursor::new(bytes);
        let decoder = JPEGDecoder::new(bytes).expect("Couldn't make decoder");
        let (width, height) = decoder.dimensions();
        println!("Width and height: {} {}", width, height);
        let image = decoder.read_image().expect("Couldn't read image");
        let raw = RawImage2d {
            data: Cow::Owned(image),
            width: width as u32,
            height: height as u32,
            format: ClientFormat::U8U8U8,
        };

        let gl_texture = Texture2d::new(display.get_context(), raw)
            .expect("Couldn't create new texture");
        let texture_id = renderer.textures().insert(Rc::new(gl_texture));

        let io = imgui.io_mut();
        platform
            .prepare_frame(io, &window)
            .expect("Failed to start frame.");
        let mut ui = imgui.frame();

        Window::new(im_str!("Camera"))
            .size([800.0, 600.0], Condition::FirstUseEver)
            .build(&ui, || {
                ui.text(im_str!("Camera"));
                Image::new(texture_id, [640.0, 480.0]).build(&ui);
            });
        let mut target = display.draw();
        target.clear_color_srgb(0.8, 0.8, 0.8, 1.0);
        platform.prepare_render(&ui, &window);
        let draw_data = ui.render();
        renderer
            .render(&mut target, draw_data)
            .expect("Couldn't render");
        target.finish().expect("Failed to swap buffers");
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8001")?;
    for stream in listener.incoming() {
        handle_image_stream(stream?)?;
    }
    Ok(())
}
