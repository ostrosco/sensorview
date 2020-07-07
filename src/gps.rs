use crate::window::{Modal, Renderable};
use byteorder::{LittleEndian, ReadBytesExt};
use crossbeam::channel::{unbounded, Receiver, Sender};
use glium::Display;
use glium::{
    backend::Facade,
    texture::{ClientFormat, RawImage2d},
    Texture2d,
};
use image::png::PngDecoder;
use image::ImageDecoder;
use image::{Rgb, RgbImage};
use imageproc::drawing::draw_filled_circle_mut;
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

// Defines the meters per pixel by zoom level from 0 to 20.
static METERS_PER_PIXEL: [f32; 21] = [
    156_412.0, 78206.0, 39103.0, 19551.0, 9776.0, 4888.0, 2444.0, 1222.0, 610.984, 305.492,
    152.746, 76.373, 38.187, 19.093, 9.547, 4.773, 2.387, 1.193, 0.596, 0.298, 0.149,
];

pub struct Gps {
    sender: Sender<GpsData>,
}

impl Gps {
    pub fn new(sender: Sender<GpsData>) -> Self {
        Self { sender }
    }

    /// Starts a TCP listener to receive data from the GPS. This supports multiple connections,
    /// though multiple connections aren't handled correctly at the moment.
    pub fn start(mut self, ip: SocketAddr) -> JoinHandle<io::Result<()>> {
        thread::spawn(move || {
            let listener = TcpListener::bind(&ip).unwrap();
            for stream in listener.incoming() {
                self.handle_gps(stream?)?;
            }
            Ok(())
        })
    }

    pub fn handle_gps(&mut self, mut stream: TcpStream) -> io::Result<()> {
        loop {
            let lat = stream.read_f32::<LittleEndian>()?;
            let lon = stream.read_f32::<LittleEndian>()?;
            let data = GpsData { lat, lon };
            self.sender.send(data).unwrap();
        }
    }
}

#[derive(Clone)]
pub struct GpsData {
    lat: f32,
    lon: f32,
}

pub struct GpsWindow {
    pub texture_id: Option<TextureId>,
    pub image: RgbImage,
    pub receiver: Receiver<GpsData>,
    pub points: Vec<(i32, i32)>,
    pub query_lat: f32,
    pub query_lon: f32,
    pub lat_meters: f32,
    pub lon_meters: f32,
    pub nw_lat: f32,
    pub nw_lon: f32,
    pub x_tile: u32,
    pub y_tile: u32,
    pub zoom: u32,
    pub width: u32,
    pub height: u32,
}

struct OsmTile {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

impl GpsWindow {
    pub fn new(receiver: Receiver<GpsData>) -> Self {
        Self {
            texture_id: None,
            image: RgbImage::from_raw(0, 0, Vec::new()).unwrap(),
            receiver,
            x_tile: 0,
            y_tile: 0,
            query_lat: 0.0,
            query_lon: 0.0,
            lon_meters: 0.0,
            lat_meters: 0.0,
            nw_lat: 0.0,
            nw_lon: 0.0,
            zoom: 0,
            points: Vec::new(),
            width: 0,
            height: 0,
        }
    }

    fn meters_per_pixel(&self) -> f32 {
        METERS_PER_PIXEL[self.zoom as usize] * (self.query_lat * PI / 180.0).cos()
    }

    /// Converts a set of GPS coordinates to pixel coordinates relative to the northwestern
    /// coordinates of the tiles being drawn.
    fn coords_to_pixel(&self, coords: &GpsData) -> (i32, i32) {
        let meters_per_pixel = self.meters_per_pixel();
        let lon_diff = self.lon_meters * (coords.lon - self.nw_lon).abs() / meters_per_pixel;
        let lat_diff = self.lat_meters * (coords.lat - self.nw_lat).abs() / meters_per_pixel;
        (lon_diff.floor() as i32, lat_diff.floor() as i32)
    }

    /// Gathers tiles that contain and surround the given latitude and longitude. Also calculates
    /// the most northwestern coordinate and the number of meters per degree for latitude and
    /// longitude at this given latitude.
    fn query_osm(&mut self, lat: f32, lon: f32) -> Result<(), Box<dyn Error>> {
        self.query_lat = lat;
        self.query_lon = lon;
        let n = (1 << self.zoom) as f32;

        // First, calculate the right tile to query that contains this input coordinate. Taken from:
        // https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames
        self.x_tile = ((lon + 180.0) / 360.0 * n as f32).floor() as u32;
        let lat_rad = lat * PI / 180.0;
        self.y_tile = ((1.0 - (lat_rad.tan().asinh()) / PI) / 2.0 * n).floor() as u32;

        let (nw_xtile, nw_ytile) = if self.zoom > 0 {
            let image_bytes = self.query_tiles()?;
            self.image = RgbImage::from_raw(self.width, self.height, image_bytes).unwrap();
            // TODO: for the moment, the map is hardcoded to query a 3x3 grid for the map, so we
            // know for certain which tile is the northwestern tile. In theory though, this
            // shouldn't be hardcoded.
            (self.x_tile - 1, self.y_tile - 1)
        } else {
            let tile = self.query_tile(self.x_tile, self.y_tile)?;
            self.width = tile.width;
            self.height = tile.height;
            self.image = RgbImage::from_raw(self.width, self.height, tile.data).unwrap();
            (self.x_tile, self.y_tile)
        };

        // Now, work backwards to calculate the lat/lon of the northwestern corner of the tile.
        // Taken from: https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames
        self.nw_lon = nw_xtile as f32 / n * 360.0 - 180.0;
        let n = PI - 2.0 * PI * nw_ytile as f32 / n;
        self.nw_lat = 180.0 / PI * (0.5 * (n.exp() - (-n).exp())).atan();

        // Lastly, calculate the number of meters to move one degree north or south from the
        // corner. Taken from: https://en.wikipedia.org/wiki/Geographic_coordinate_system
        let query_lat_rad = self.query_lat * PI / 180.0;
        self.lat_meters = 111_132.92 - 559.82 * (2.0 * query_lat_rad).cos()
            + 1.175 * (4.0 * query_lat_rad).cos()
            - 0.0023 * (6.0 * query_lat_rad).cos();
        self.lon_meters = 111_412.84 * query_lat_rad.cos() - 93.5 * (3.0 * query_lat_rad).cos()
            + 0.118 * (5.0 * query_lat_rad).cos();
        Ok(())
    }

    /// Queries nine tiles used for drawing data onto the map.
    fn query_tiles(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        // First off, we need to query nine tiles. The tile for our starting point will be the
        // center tile and we'll query all the other tiles around it.
        let mut image_bytes = Vec::new();
        for y in -1_i32..=1 {
            let mut row =
                self.query_map_row(self.x_tile - 1, (self.y_tile as i32 + y) as u32, 3)?;
            image_bytes.append(&mut row);
        }
        self.height *= 3;
        Ok(image_bytes)
    }

    /// Queries a row of tiles and stitches them together.
    fn query_map_row(
        &mut self,
        x_tile: u32,
        y_tile: u32,
        row_length: u32,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut tiles = Vec::new();
        for ix in 0..row_length {
            tiles.push(self.query_tile(x_tile + ix, y_tile)?);
        }

        let mut map_row = Vec::new();
        for row_num in 0..256 {
            for tile in &tiles {
                let start_byte = row_num * 256 * 3;
                let end_byte = start_byte + 256 * 3;
                map_row.extend_from_slice(&tile.data[start_byte..end_byte]);
            }
        }

        self.width = tiles[0].width * row_length;
        self.height = tiles[0].height;
        Ok(map_row)
    }

    /// Queries a single tile from OpenStreetMap.
    fn query_tile(&self, x_tile: u32, y_tile: u32) -> Result<OsmTile, Box<dyn Error>> {
        let mut resp = reqwest::get(&format!(
            "http://a.tile.openstreetmap.org/{}/{}/{}.png",
            self.zoom, x_tile, y_tile,
        ))?;
        let mut bytes: Vec<u8> = Vec::new();
        resp.copy_to(&mut bytes)?;
        let bytes = Cursor::new(bytes);
        let decoder = PngDecoder::new(bytes).expect("couldn't make decoder");
        let (width, height) = decoder.dimensions();
        let mut data: Vec<u8> = vec![0; decoder.total_bytes() as usize];
        decoder.read_image(&mut data).expect("couldn't parse image");
        Ok(OsmTile {
            data,
            width: width as u32,
            height: height as u32,
        })
    }
}

impl Renderable for GpsWindow {
    /// Renders the data received from the gps sensor. This currently assumes RGB data format.
    fn render(&mut self, ui: &Ui, display: &Display, renderer: &mut Renderer) {
        if self.image.is_empty() {
            self.query_osm(self.query_lat, self.query_lon)
                .expect("Couldn't get tiles");

            let image_frame = Some(RawImage2d {
                data: Cow::Owned(self.image.to_vec()),
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
                self.texture_id = Some(renderer.textures().insert(Rc::new(gl_texture)));
            }
        }

        if let Ok(gps_data) = self.receiver.try_recv() {
            // The zoom is zero until we receive our first point. Once the first point comes in,
            // query OSM for the tiles for this point.
            if self.zoom == 0 {
                self.zoom = 16;
                self.query_osm(gps_data.lat, gps_data.lon).unwrap();
            }

            // TODO: consider _not_ adding the point if the point hasn't changed between
            // measurements. A stationary object shouldn't overwrite the entire track of points
            // thus far.
            let pixel_coords = self.coords_to_pixel(&gps_data);
            self.points.push(pixel_coords);
            let color = Rgb([0u8, 0u8, 255u8]);
            draw_filled_circle_mut(&mut self.image, pixel_coords, 3, color);

            // TODO: this needs to be filled in to do the following things:
            //
            // Check if the current tile will fit all of the current points.  If not, get a new
            // tile and re-draw the points on top.
            let image_frame = Some(RawImage2d {
                data: Cow::Owned(self.image.to_vec()),
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
                self.texture_id = Some(renderer.textures().insert(Rc::new(gl_texture)));
            }
        }

        // We call this each iteration of the GpsWindow, so we need to make sure we draw the
        // window even if we didn't receive camera data on this iteration. However, we currently
        // do not draw a window unless we've received our first sample from the camera.
        if let Some(tex_id) = self.texture_id {
            let dims = [self.width as f32, self.height as f32];
            Window::new(im_str!("GPS"))
                .flags(WindowFlags::ALWAYS_AUTO_RESIZE)
                .build(ui, || {
                    Image::new(tex_id, dims).build(&ui);
                });
        } else {
            Window::new(im_str!("GPS")).build(ui, || {
                ui.text(im_str!("Waiting for GPS data..."));
            });
        }
    }
}

pub struct GpsConfig {
    gps_port: ImString,
}

impl GpsConfig {
    pub fn new() -> Self {
        let mut gps_port = ImString::new("8003");
        gps_port.reserve_exact(10);
        Self { gps_port }
    }
}

impl Modal for GpsConfig {
    fn render_modal(
        &mut self,
        ui: &Ui,
        join_handles: &mut Vec<JoinHandle<io::Result<()>>>,
        sensor_windows: &mut Vec<Box<dyn Renderable>>,
    ) {
        ui.popup_modal(im_str!("GPS Configuration"))
            .flags(WindowFlags::ALWAYS_AUTO_RESIZE)
            .build(|| {
                ui.input_text(im_str!("Listen Port"), &mut self.gps_port)
                    .build();
                if ui.button(im_str!("Create Sensor Window"), [0.0, 0.0]) {
                    let (gps_tx, gps_rx) = unbounded();
                    let gps = Gps::new(gps_tx);
                    join_handles.push(
                        gps.start(
                            format!("0.0.0.0:{}", self.gps_port.to_string())
                                .parse()
                                .expect("couldn't parse IP address"),
                        ),
                    );
                    sensor_windows.push(Box::new(GpsWindow::new(gps_rx)));
                    ui.close_current_popup();
                }
            });
    }
}
