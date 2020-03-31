use crate::window::{Modal, Renderable};
use byteorder::{LittleEndian, WriteBytesExt};
use gilrs::{Axis, Button, Event, EventType, Gilrs};
use imgui::{self, im_str, ImString, Ui, WindowFlags};
use serde::Serialize;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::net::TcpStream;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// A gamepad event enumeration identical to the EventType enumeration in
/// `gilrs` except with the Code field removed.
#[derive(Debug, Serialize)]
pub enum GpEvent {
    ButtonPressed(Button),
    ButtonRepeated(Button),
    ButtonReleased(Button),
    ButtonChanged(Button, f32),
    AxisChanged(Axis, f32),
    Connected,
    Disconnected,
    Dropped,
}

pub struct Controller;

impl Controller {
    pub fn start(ip: SocketAddr) -> JoinHandle<io::Result<()>> {
        thread::spawn(move || {
            let mut gilrs = Gilrs::new().unwrap();
            let mut stream = loop {
                match TcpStream::connect(ip) {
                    Ok(conn) => break conn,
                    Err(_) => thread::sleep(Duration::from_secs(10)),
                }
            };
            loop {
                while let Some(Event { event, .. }) = gilrs.next_event() {
                    // Most of the fields in gilrs are serializable except
                    // for the Code on each event. Since we don't need it and
                    // we want to send events directly, we translate between
                    // gilrs's events to custom events without the code so we
                    // can send it over the wire.
                    let gp_event = match event {
                        EventType::ButtonPressed(btn, ..) => {
                            GpEvent::ButtonPressed(btn)
                        }
                        EventType::ButtonRepeated(btn, ..) => {
                            GpEvent::ButtonRepeated(btn)
                        }
                        EventType::ButtonReleased(btn, ..) => {
                            GpEvent::ButtonReleased(btn)
                        }
                        EventType::ButtonChanged(btn, val, ..) => {
                            GpEvent::ButtonChanged(btn, val)
                        }
                        EventType::AxisChanged(axis, val, ..) => {
                            GpEvent::AxisChanged(axis, val)
                        }
                        EventType::Connected => GpEvent::Connected,
                        EventType::Disconnected => GpEvent::Disconnected,
                        EventType::Dropped => GpEvent::Dropped,
                    };
                    let data = serde_cbor::to_vec(&gp_event).unwrap();
                    stream.write_u32::<LittleEndian>(data.len() as u32)?;
                    stream.write_all(&data)?;
                    stream.flush()?;
                }
            }
        })
    }
}

pub struct ControllerConfig {
    send_ip: ImString,
}

impl ControllerConfig {
    pub fn new() -> Self {
        let mut send_ip = ImString::new("");
        send_ip.reserve_exact(21);
        Self { send_ip }
    }
}

impl Modal for ControllerConfig {
    fn render_modal(
        &mut self,
        ui: &Ui,
        join_handles: &mut Vec<JoinHandle<io::Result<()>>>,
        _sensor_windows: &mut Vec<Box<dyn Renderable>>,
    ) {
        // TODO: right now this just creates the modal and then just silently
        // sends controller events out. It'd be nice to have a window maybe
        // representing the controller?
        ui.popup_modal(im_str!("Controller Configuration"))
            .flags(WindowFlags::ALWAYS_AUTO_RESIZE)
            .build(|| {
                ui.input_text(im_str!("Send Address"), &mut self.send_ip)
                    .build();

                if ui.button(im_str!("Send Controller State"), [0.0, 0.0]) {
                    join_handles.push(Controller::start(
                        self.send_ip
                            .to_string()
                            .parse()
                            .expect("couldn't parse IP address"),
                    ));
                    ui.close_current_popup();
                }
            });
    }
}
