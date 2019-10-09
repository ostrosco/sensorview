use crate::window::Renderable;
use byteorder::{BigEndian, ReadBytesExt};
use crossbeam::{unbounded, Receiver, Sender};
use glium::Display;
use glium::{
    backend::Facade,
    texture::{ClientFormat, RawImage2d},
    Texture2d,
};
use imgui::TextureId;
use imgui::{self, im_str, ImString, Image, Ui, Window, WindowFlags};
use imgui_glium_renderer::Renderer;
use image::{Rgb, RgbImage};
use imageproc::drawing::{draw_filled_circle_mut};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::f32::consts::PI;
use std::borrow::Cow;
use std::rc::Rc;
use std::time::Instant;


pub struct LidarData {
    distances: Vec<f32>,
}

pub struct Lidar {
    sender: Sender<LidarData>,
}

impl Lidar {
    pub fn new(sender: Sender<LidarData>) -> Self {
        Self { sender }
    }

    /// Starts a TCP listener to receive data from the LIDAR. This supports
    /// multiple connections, though multiple connections aren't handled
    /// correctly at the moment.
    ///
    pub fn start(mut self, ip: SocketAddr) -> JoinHandle<io::Result<()>> {
        thread::spawn(move || {
            let listener = TcpListener::bind(&ip).unwrap();
            for stream in listener.incoming() {
                self.handle_lidar_stream(stream?)?;
            }
            Ok(())
        })
    }

    pub fn handle_lidar_stream(
        &mut self,
        mut stream: TcpStream,
    ) -> io::Result<()> {
        loop {
            let mut distances = [0f32; 360];
            stream.read_f32_into::<BigEndian>(&mut distances)?;
            let lidar_data = LidarData { distances: distances.to_vec() };
            self.sender.send(lidar_data).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "camera channel disconnected",
                )
            })?;
        }
    }
}

pub struct LidarWindow {
    texture_id: Option<TextureId>,
    receiver: Receiver<LidarData>,
    lidar_data: Vec<f32>,
}

impl LidarWindow {
    pub fn new(receiver: Receiver<LidarData>) -> Self {
        Self {
            texture_id: None,
            receiver,
            lidar_data: Vec::new(),
        }
    }
}

impl Renderable for LidarWindow {
    fn render(&mut self, ui: &Ui, display: &Display, renderer: &mut Renderer) {
        let timer = Instant::now();
        if let Ok(lidar_data) = self.receiver.try_recv() {
            self.lidar_data = lidar_data.distances;
            let mut image = RgbImage::new(400, 400);
            let color = Rgb([255u8, 0u8, 0u8]);
            for (angle, distance) in self.lidar_data.iter().enumerate() {
                let angle = angle as f32 * PI / 180.0;
                let x = distance * angle.cos();
                let y = distance * angle.sin();
                draw_filled_circle_mut(&mut image, (x as i32, y as i32), 2, color);
            }
            let image_frame = Some(RawImage2d {
                data: Cow::Owned(image.into_vec()),
                width: 400,
                height: 400,
                format: ClientFormat::U8U8U8,
            })
            .unwrap();
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
        // sure we draw the window even if we didn't receive LIDAR data on
        // this iteration. However, we currently do not draw a window unless
        // we've received our first sample from the LIDAR.
        println!("Elapsed time: {}", timer.elapsed().as_micros());
        if let Some(tex_id) = self.texture_id {
            let image_dims = [400.0, 400.0];
            Window::new(im_str!("LIDAR"))
                .flags(WindowFlags::ALWAYS_AUTO_RESIZE)
                .build(ui, || {
                    Image::new(tex_id, image_dims)
                        .build(&ui);
                });
        } else {
            Window::new(im_str!("LIDAR")).build(ui, || {
                ui.text(im_str!("Waiting for LIDAR data..."));
            });
        }
    }
}

pub struct LidarConfig {
    lidar_ip: ImString,
}

impl LidarConfig {
    pub fn new() -> Self {
        Self {
            lidar_ip: ImString::with_capacity(20),
        }
    }

    pub fn render_lidar_modal(
        &mut self,
        ui: &Ui,
        join_handles: &mut Vec<JoinHandle<io::Result<()>>>,
        sensor_windows: &mut Vec<Box<dyn Renderable>>,
    ) {
        ui.popup_modal(im_str!("LIDAR Configuration")).build(|| {
            ui.input_text(im_str!("Listen Address"), &mut self.lidar_ip)
                .build();
            if ui.button(im_str!("Create Sensor Window"), [0.0, 0.0]) {
                let (lidar_tx, lidar_rx) = unbounded();
                let lidar = Lidar::new(lidar_tx);
                join_handles.push(
                    lidar.start(
                        self.lidar_ip
                            .to_string()
                            .parse()
                            .expect("couldn't parse IP address"),
                    ),
                );
                sensor_windows.push(Box::new(LidarWindow::new(lidar_rx)));
                ui.close_current_popup();
            }
        });
    }
}
