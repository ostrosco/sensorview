use crate::window::Renderable;
use byteorder::{LittleEndian, ReadBytesExt};
use crossbeam::channel::{Receiver, Sender};
use glium::Display;
use glium::{
    backend::Facade,
    texture::{ClientFormat, RawImage2d},
    Texture2d,
};
use image::jpeg::JPEGDecoder;
use image::ImageDecoder;
use imgui::TextureId;
use imgui::{self, im_str, Image, Ui, Window};
use imgui_glium_renderer::Renderer;
use std::borrow::Cow;
use std::io::{self, Cursor, Read};
use std::net::SocketAddr;
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;

pub struct Camera {
    sender: Sender<CameraData>,
}

pub struct CameraData {
    pub image_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Camera {
    pub fn new(sender: Sender<CameraData>) -> Self {
        Self { sender }
    }

    /// Starts a TCP listener to receive data from the camera. This supports
    /// multiple connections, though multiple connections aren't handled
    /// correctly at the moment.
    ///
    pub fn start(mut self, ip: SocketAddr) -> io::Result<()> {
        let listener = TcpListener::bind(&ip).unwrap();
        for stream in listener.incoming() {
            self.handle_image_stream(stream?)?;
        }
        Ok(())
    }

    /// Receives bytes and decodes them to bytes. This function makes a couple
    /// of assumptions:
    ///
    /// 1. This function assumes the data format of the Raspberry Pi Camera
    ///    (which is a u32 containing the data length followed by n bytes).
    /// 2. The data is assumed to be MJPEG.
    ///
    pub fn handle_image_stream(
        &mut self,
        mut stream: TcpStream,
    ) -> io::Result<()> {
        loop {
            let size = stream.read_u32::<LittleEndian>()? as usize;
            let mut bytes = vec![0; size];
            stream.read_exact(&mut bytes[..])?;
            let bytes = Cursor::new(bytes);
            let decoder =
                JPEGDecoder::new(bytes).expect("Couldn't make decoder");
            let (width, height) = decoder.dimensions();
            let image_bytes =
                decoder.read_image().expect("Couldn't read image");
            let camera_data = CameraData {
                image_bytes,
                width: width as u32,
                height: height as u32,
            };
            self.sender.send(camera_data).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "camera channel disconnected",
                )
            })?;
        }
    }
}

pub struct CameraWindow {
    pub rotation: u8,
    pub window_width: f32,
    pub window_height: f32,
    pub texture_id: Option<TextureId>,
}

impl CameraWindow {
    pub fn new() -> Self {
        Self {
            rotation: 0,
            window_width: 0.0,
            window_height: 0.0,
            texture_id: None,
        }
    }
}

impl Renderable for CameraWindow {
    type Item = CameraData;

    /// Renders the data received from the camera sensor. This currently
    /// assumes RGB data format.
    fn render(
        &mut self,
        ui: &Ui,
        display: &Display,
        renderer: &mut Renderer,
        receiver: &mut Receiver<Self::Item>,
    ) {
        // If we've received new camera data, update the texture. We also need
        // to check if there is an existing texture ahead of time so we can
        // reuse the texture instead of creating a new one each time.
        if let Ok(camera_data) = receiver.try_recv() {
            let image_frame = Some(RawImage2d {
                data: Cow::Owned(camera_data.image_bytes.clone()),
                width: camera_data.width as u32,
                height: camera_data.height as u32,
                format: ClientFormat::U8U8U8,
            })
            .unwrap();
            self.window_width = camera_data.width as f32;
            self.window_height = camera_data.height as f32;
            let gl_texture = Texture2d::new(display.get_context(), image_frame)
                .expect("Couldn't create new texture");
            if let Some(tex_id) = self.texture_id {
                renderer.textures().replace(tex_id, Rc::new(gl_texture));
            } else {
                self.texture_id =
                    Some(renderer.textures().insert(Rc::new(gl_texture)));
            }
        }

        // We call this each iteration of the CameraWindow, so we need to make
        // sure we draw the window even if we didn't receive camera data on
        // this iteration. However, we currently do not draw a window unless
        // we've received our first sample from the camera.
        if let Some(tex_id) = self.texture_id {
            let camera_dims = [self.window_width, self.window_height];
            Window::new(im_str!("Camera")).build(ui, || {
                Image::new(tex_id, camera_dims)
                    .uv0([1.0, 1.0])
                    .uv1([0.0, 0.0])
                    .build(&ui);
            });
        }
    }
}
