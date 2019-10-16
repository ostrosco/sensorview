mod camera;
mod gps;
mod lidar;
mod window;

use std::io;
use window::SensorWindow;

fn main() -> io::Result<()> {
    let window = SensorWindow::new();
    window.render();
    Ok(())
}
