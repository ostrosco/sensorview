use crate::window::{Modal, Renderable};
use crossbeam::channel::{unbounded, Receiver, Sender};
use glium::Display;
use glium::{
    backend::Facade,
    texture::{ClientFormat, RawImage2d},
    Texture2d,
};
use image::png::PNGDecoder;
use image::ImageDecoder;
use imgui::TextureId;
use imgui::{self, im_str, ImString, Image, Ui, Window, WindowFlags};
use imgui_glium_renderer::Renderer;
use std::borrow::Cow;
use std::error::Error;
use std::f32::consts::PI;
use std::io::{self, Cursor};
use std::net::SocketAddr;
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::thread::{self, JoinHandle};

pub struct GPS {
    sender: Sender<GPSData>,
}

impl GPS {
    pub fn new(sender: Sender<GPSData>) -> Self {
        Self { sender }
    }

    /// Starts a TCP listener to receive data from the GPS. This supports
    /// multiple connections, though multiple connections aren't handled
    /// correctly at the moment.
    ///
    pub fn start(mut self, ip: SocketAddr) -> JoinHandle<io::Result<()>> {
        thread::spawn(move || {
            let listener = TcpListener::bind(&ip).unwrap();
            for stream in listener.incoming() {
                self.handle_gps(stream?)?;
            }
            Ok(())
        })
    }

    pub fn handle_gps(&mut self, mut _stream: TcpStream) -> io::Result<()> {
        // TODO:
        //
        // Handle the NMEA data we're receiving from the GPS. From there,
        // pull out the lat/lon (as that's all we probably care about right now)
        // and send it to the window.
        Ok(())
    }
}

#[derive(Clone)]
pub struct GPSData {
    lat: f32,
    lon: f32,
}

pub struct GPSWindow {
    pub texture_id: Option<TextureId>,
    pub tile: Vec<u8>,
    pub receiver: Receiver<GPSData>,
    pub points: Vec<GPSData>,
    pub x_tile: u32,
    pub y_tile: u32,
    pub zoom: u32,
    pub width: u32,
    pub height: u32,
}

impl GPSWindow {
    pub fn new(receiver: Receiver<GPSData>) -> Self {
        Self {
            texture_id: None,
            tile: Vec::new(),
            receiver,
            x_tile: 0,
            y_tile: 0,
            zoom: 0,
            points: Vec::new(),
            width: 0,
            height: 0,
        }
    }

    fn query_osm(&mut self, lat: f32, lon: f32) -> Result<(), Box<dyn Error>> {
        self.x_tile =
            ((lon + 180.0) / 360.0 * (1 << self.zoom) as f32).floor() as u32;
        let lat_rad = lat * PI / 180.0;
        self.y_tile = ((1.0 - (lat_rad.tan().asinh()) / PI) / 2.0
            * (1 << self.zoom) as f32)
            .floor() as u32;
        let mut resp = reqwest::get(&format!(
            "http://a.tile.openstreetmap.org/{}/{}/{}.png",
            self.zoom, self.x_tile, self.y_tile
        ))?;
        let mut bytes: Vec<u8> = Vec::new();
        resp.copy_to(&mut bytes)?;
        let bytes = Cursor::new(bytes);
        let decoder = PNGDecoder::new(bytes).expect("couldn't make decoder");
        let (width, height) = decoder.dimensions();
        self.width = width as u32;
        self.height = height as u32;
        self.tile =
            decoder.read_image().expect("couldn't parse image").to_vec();

        Ok(())
    }
}

impl Renderable for GPSWindow {
    /// Renders the data received from the gps sensor. This currently
    /// assumes RGB data format.
    fn render(&mut self, ui: &Ui, display: &Display, renderer: &mut Renderer) {
        if self.tile.is_empty() {
            self.query_osm(0.0, 0.0).expect("Couldn't get tiles");

            let image_frame = Some(RawImage2d {
                data: Cow::Owned(self.tile.clone()),
                width: self.width as u32,
                height: self.height as u32,
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

        if let Ok(gps_data) = self.receiver.try_recv() {
            // TODO: consider _not_ adding the point if the point hasn't
            // changed between measurements. A stationary object shouldn't
            // overwrite the entire track of points thus far.
            self.points.push(gps_data.clone());

            // TODO: this needs to be filled in to do the following things:
            //
            // Check if the current tile will fit all of the current points.
            //    a. If so, use the current tile and draw the points on top.
            //    b. If not, get a new tile and draw the points on top.
            //
            let image_frame = Some(RawImage2d {
                data: Cow::Owned(self.tile.clone()),
                width: self.width as u32,
                height: self.height as u32,
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
        // sure we draw the window even if we didn't receive camera data on
        // this iteration. However, we currently do not draw a window unless
        // we've received our first sample from the camera.
        if let Some(tex_id) = self.texture_id {
            let dims = [self.width as f32, self.height as f32];
            Window::new(im_str!("GPS"))
                .flags(WindowFlags::ALWAYS_AUTO_RESIZE)
                .build(ui, || {
                    Image::new(tex_id, dims)
                        .uv0([1.0, 1.0])
                        .uv1([0.0, 0.0])
                        .build(&ui);
                });
        } else {
            Window::new(im_str!("GPS")).build(ui, || {
                ui.text(im_str!("Waiting for GPS data..."));
            });
        }
    }
}

pub struct GPSConfig {
    gps_ip: ImString,
}

impl GPSConfig {
    pub fn new() -> Self {
        let mut gps_ip = ImString::new("0.0.0.0:8003");
        gps_ip.reserve_exact(10);
        Self { gps_ip }
    }
}

impl Modal for GPSConfig {
    fn render_modal(
        &mut self,
        ui: &Ui,
        join_handles: &mut Vec<JoinHandle<io::Result<()>>>,
        sensor_windows: &mut Vec<Box<dyn Renderable>>,
    ) {
        ui.popup_modal(im_str!("GPS Configuration"))
            .flags(WindowFlags::ALWAYS_AUTO_RESIZE)
            .build(|| {
                ui.input_text(im_str!("Listen Address"), &mut self.gps_ip)
                    .build();
                if ui.button(im_str!("Create Sensor Window"), [0.0, 0.0]) {
                    let (gps_tx, gps_rx) = unbounded();
                    let gps = GPS::new(gps_tx);
                    join_handles.push(
                        gps.start(
                            self.gps_ip
                                .to_string()
                                .parse()
                                .expect("couldn't parse IP address"),
                        ),
                    );
                    sensor_windows.push(Box::new(GPSWindow::new(gps_rx)));
                    ui.close_current_popup();
                }
            });
    }
}
