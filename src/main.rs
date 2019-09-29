mod camera;
mod window;

use camera::Camera;
use crossbeam::unbounded;
use std::io;
use std::thread;
use window::{SensorData, SensorWindow};

fn main() -> io::Result<()> {
    let (camera_tx, camera_rx) = unbounded();
    let mut sensor_data = SensorData::new();
    sensor_data.camera = Some(camera_rx);

    thread::spawn(move || -> io::Result<()> {
        let camera = Camera::new(camera_tx);
        camera.start("0.0.0.0:8001".parse().unwrap())?;
        Ok(())
    });

    let window = SensorWindow::new();
    window.render(sensor_data);
    Ok(())
}
