use byteorder::{LittleEndian, ReadBytesExt};
use crossbeam::channel::Sender;
use image::jpeg::JPEGDecoder;
use image::ImageDecoder;
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

impl Camera {
    pub fn new(sender: Sender<CameraData>) -> Self {
        Self { sender }
    }

    pub fn start(mut self, ip: SocketAddr) -> io::Result<()> {
        let listener = TcpListener::bind(&ip).unwrap();
        for stream in listener.incoming() {
            self.handle_image_stream(stream?)?;
        }
        Ok(())
    }

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
