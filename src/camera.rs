use byteorder::{LittleEndian, ReadBytesExt};
use crossbeam::channel::Sender;
use image::jpeg::JPEGDecoder;
use image::ImageDecoder;
use imgui::TextureId;
use std::io::{self, Cursor, Read};
use std::net::SocketAddr;
use std::net::{TcpListener, TcpStream};

pub struct Camera {
    sender: Sender<CameraData>,
}

pub struct CameraData {
    pub image_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct CameraSettings {
    pub rotation: u8,
    pub window_width: f32,
    pub window_height: f32,
    pub texture_id: Option<TextureId>,
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
