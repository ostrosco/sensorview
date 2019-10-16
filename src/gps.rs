use crate::window::{Modal, Renderable};
use byteorder::{LittleEndian, ReadBytesExt};
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
use std::io::{self, Cursor, Read};
use std::net::SocketAddr;
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::str::FromStr;
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
    pub receiver: Receiver<GPSData>,
    pub points: Vec<GPSData>,
    pub x_tile: u32,
    pub y_tile: u32,
    pub zoom: u32,
}

impl GPSWindow {
    pub fn new(receiver: Receiver<GPSData>) -> Self {
        Self {
            texture_id: None,
            receiver,
            x_tile: 0,
            y_tile: 0,
            zoom: 0,
            points: Vec::new(),
        }
    }
}

impl Renderable for GPSWindow {
    /// Renders the data received from the gps sensor. This currently
    /// assumes RGB data format.
    fn render(&mut self, ui: &Ui, display: &Display, renderer: &mut Renderer) {
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
        }

        Window::new(im_str!("GPS")).build(ui, || {
            ui.text(im_str!("Waiting for GPS data..."));
        });
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
