use crate::window::Renderable;
use byteorder::{LittleEndian, ReadBytesExt};
use crossbeam::channel::{unbounded, Receiver, Sender};
use glium::Display;
use glium::{
    backend::Facade,
    texture::{ClientFormat, RawImage2d},
    Texture2d,
};
use image::jpeg::JPEGDecoder;
use image::ImageDecoder;
use imgui::TextureId;
use imgui::{self, im_str, ImString, Image, Ui, Window, WindowFlags};
use imgui_glium_renderer::Renderer;
use std::borrow::Cow;
use std::io::{self, Cursor, Read};
use std::net::SocketAddr;
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::str::FromStr;
use std::thread::{self, JoinHandle};
use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, EnumIter, EnumString};

#[derive(AsRefStr, EnumIter, EnumString, Clone, Copy, Debug)]
/// A list of allowed formats for the camera. Currently we only support
/// MJPEG, but the boilerplate for allowing the user to select different
/// formats is set up.
pub enum VideoFormat {
    MJPEG,
    H264,
}

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
    pub fn start(
        mut self,
        ip: SocketAddr,
        video_format: VideoFormat,
    ) -> JoinHandle<io::Result<()>> {
        println!("Starting a camera on {} with format {:?}", ip, video_format);
        thread::spawn(move || {
            let listener = TcpListener::bind(&ip).unwrap();
            for stream in listener.incoming() {
                self.handle_image_stream(stream?, video_format)?;
            }
            Ok(())
        })
    }

    /// Receives bytes and decodes them to bytes. Currently only supports
    /// MJPEG, though the boilerplate for H264 exists.
    pub fn handle_image_stream(
        &mut self,
        stream: TcpStream,
        video_format: VideoFormat,
    ) -> io::Result<()> {
        match video_format {
            VideoFormat::MJPEG => self.handle_mjpeg(stream),
            VideoFormat::H264 => Ok(()),
        }
    }

    /// Handles receiving MJPEG data and sending frames to the camera window.
    /// This function assumes the data format of the Raspberry Pi Camera
    /// (which is a u32 containing the data length followed by n bytes).
    fn handle_mjpeg(&mut self, mut stream: TcpStream) -> io::Result<()> {
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
    pub receiver: Receiver<CameraData>,
}

impl CameraWindow {
    pub fn new(receiver: Receiver<CameraData>) -> Self {
        Self {
            rotation: 0,
            window_width: 0.0,
            window_height: 0.0,
            texture_id: None,
            receiver,
        }
    }
}

impl Renderable for CameraWindow {
    /// Renders the data received from the camera sensor. This currently
    /// assumes RGB data format.
    fn render(&mut self, ui: &Ui, display: &Display, renderer: &mut Renderer) {
        // If we've received new camera data, update the texture. We also need
        // to check if there is an existing texture ahead of time so we can
        // reuse the texture instead of creating a new one each time.
        if let Ok(camera_data) = self.receiver.try_recv() {
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
            Window::new(im_str!("Camera"))
                .flags(WindowFlags::ALWAYS_AUTO_RESIZE)
                .build(ui, || {
                    Image::new(tex_id, camera_dims)
                        .uv0([1.0, 1.0])
                        .uv1([0.0, 0.0])
                        .build(&ui);
                });
        } else {
            Window::new(im_str!("Camera")).build(ui, || {
                ui.text(im_str!("Waiting for camera data..."));
            });
        }
    }
}

pub struct CameraConfig {
    camera_ip: ImString,
    video_format_list: Vec<ImString>,
    video_format_item: usize,
}

impl CameraConfig {
    pub fn new() -> Self {
        let mut camera_ip = ImString::new("0.0.0.0:8001");
        let video_format_list: Vec<ImString> = VideoFormat::iter()
            .map(|vf| {
                let vf_str: &str = vf.as_ref();
                ImString::new(vf_str)
            })
            .collect();
        camera_ip.reserve_exact(10);
        Self {
            camera_ip,
            video_format_item: 0,
            video_format_list,
        }
    }

    pub fn render_camera_modal(
        &mut self,
        ui: &Ui,
        join_handles: &mut Vec<JoinHandle<io::Result<()>>>,
        sensor_windows: &mut Vec<Box<dyn Renderable>>,
    ) {
        ui.popup_modal(im_str!("Camera Configuration"))
            .flags(WindowFlags::ALWAYS_AUTO_RESIZE)
            .build(|| {
                ui.input_text(im_str!("Listen Address"), &mut self.camera_ip)
                    .build();

                // For some reason, the combo box takes a slice of references, so
                // we need to make a new Vec of references.
                let video_slices: Vec<&ImString> =
                    self.video_format_list.iter().map(|vf| vf).collect();
                imgui::ComboBox::new(im_str!("Video Format"))
                    .build_simple_string(
                        ui,
                        &mut self.video_format_item,
                        &video_slices,
                    );

                let video_format: VideoFormat = VideoFormat::from_str(
                    &self.video_format_list[self.video_format_item].to_string(),
                )
                .unwrap();

                if ui.button(im_str!("Create Sensor Window"), [0.0, 0.0]) {
                    let (camera_tx, camera_rx) = unbounded();
                    let camera = Camera::new(camera_tx);
                    join_handles.push(
                        camera.start(
                            self.camera_ip
                                .to_string()
                                .parse()
                                .expect("couldn't parse IP address"),
                            video_format,
                        ),
                    );
                    sensor_windows.push(Box::new(CameraWindow::new(camera_rx)));
                    ui.close_current_popup();
                }
            });
    }
}
