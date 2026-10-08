use inputmodule_host::protocol::{decode_version, encode_command};
pub use inputmodule_host::protocol::{Command, GameControlArg, GameId, GameOfLifePattern};
use inputmodule_host::{encode_bw_grid, Grid, BW_BYTES};
use serialport::{ClearBuffer, SerialPort};
use std::io::{self, Read, Write};
use std::time::Duration;

const SERIAL_TIMEOUT: Duration = Duration::from_millis(100);

trait Transport: Read + Write {
    fn clear_input(&mut self) -> io::Result<()>;
}
struct SerialTransport(Box<dyn SerialPort>);
impl Read for SerialTransport {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}
impl Write for SerialTransport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl Transport for SerialTransport {
    fn clear_input(&mut self) -> io::Result<()> {
        self.0.clear(ClearBuffer::Input).map_err(io::Error::from)
    }
}

#[derive(Clone, Copy)]
struct Settings {
    brightness: u8,
    animate: u8,
}

pub struct DeviceConnection {
    port: Option<Box<dyn Transport>>,
    pub port_name: Option<String>,
    pub version: Option<String>,
    last_grid: Option<[u8; BW_BYTES]>,
    last_brightness: Option<u8>,
    saved: Option<Settings>,
    embedded_owned: bool,
}

impl DeviceConnection {
    pub fn new() -> Self {
        Self {
            port: None,
            port_name: None,
            version: None,
            last_grid: None,
            last_brightness: None,
            saved: None,
            embedded_owned: false,
        }
    }
    pub fn is_connected(&self) -> bool {
        self.port.is_some()
    }

    pub fn connect(&mut self, name: &str) -> io::Result<()> {
        self.release()?;
        self.disconnect();
        let port = serialport::new(name, 115_200)
            .timeout(SERIAL_TIMEOUT)
            .open()
            .map_err(io::Error::from)?;
        self.port = Some(Box::new(SerialTransport(port)));
        self.port_name = Some(name.into());
        // Stop any old firmware game before querying: it emits unsolicited serial text.
        self.send_game_control(GameControlArg::Exit)?;
        let response = self.query(Command::Version)?;
        self.version = Some(decode_version(&response));
        Ok(())
    }

    fn disconnect(&mut self) {
        self.port = None;
        self.port_name = None;
        self.version = None;
        self.last_grid = None;
        self.last_brightness = None;
        self.saved = None;
        self.embedded_owned = false;
    }

    pub fn send_command(&mut self, command: Command, args: &[u8]) -> io::Result<()> {
        let bytes = encode_command(command, args)?;
        let result = self
            .port
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "No LED Matrix connected"))?
            .write_all(&bytes);
        if result.is_err() {
            self.disconnect();
        }
        result
    }

    fn query(&mut self, command: Command) -> io::Result<[u8; 32]> {
        let result = (|| {
            self.port
                .as_mut()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotConnected, "No LED Matrix connected")
                })?
                .clear_input()?;
            self.send_command(command, &[])?;
            let mut response = [0; 32];
            self.port
                .as_mut()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotConnected, "No LED Matrix connected")
                })?
                .read_exact(&mut response)?;
            Ok(response)
        })();
        if result.is_err() {
            self.disconnect();
        }
        result
    }

    /// Take over host rendering and preserve the user's device settings.
    pub fn prepare_host_mode(&mut self) -> io::Result<()> {
        if self.saved.is_none() {
            let brightness = self.query(Command::Brightness)?[0];
            let animate = self.query(Command::Animate)?[0];
            self.saved = Some(Settings {
                brightness,
                animate,
            });
        }
        self.send_game_control(GameControlArg::Exit)?;
        self.send_command(Command::Animate, &[0])?;
        self.last_grid = None;
        self.last_brightness = None;
        Ok(())
    }

    pub fn release(&mut self) -> io::Result<()> {
        if self.embedded_owned {
            self.send_game_control(GameControlArg::Exit)?;
            self.embedded_owned = false;
        }
        if let Some(settings) = self.saved {
            self.set_brightness(settings.brightness)?;
            self.send_command(Command::Animate, &[settings.animate])?;
            self.saved = None;
        }
        self.last_grid = None;
        self.last_brightness = None;
        Ok(())
    }

    pub fn send_grid(&mut self, grid: &Grid) -> io::Result<()> {
        let bytes = encode_bw_grid(grid);
        if self.last_grid != Some(bytes) {
            self.send_command(Command::DisplayBwImage, &bytes)?;
            self.last_grid = Some(bytes);
        }
        Ok(())
    }
    pub fn set_brightness(&mut self, brightness: u8) -> io::Result<()> {
        if self.last_brightness != Some(brightness) {
            self.send_command(Command::Brightness, &[brightness])?;
            self.last_brightness = Some(brightness);
        }
        Ok(())
    }
    pub fn start_game(&mut self, game: GameId) -> io::Result<()> {
        self.release()?;
        self.prepare_host_mode()?;
        self.send_command(Command::StartGame, &[game as u8])?;
        self.embedded_owned = true;
        Ok(())
    }
    pub fn start_game_of_life(&mut self, pattern: GameOfLifePattern) -> io::Result<()> {
        self.release()?;
        self.prepare_host_mode()?;
        self.send_command(
            Command::StartGame,
            &[GameId::GameOfLife as u8, pattern as u8],
        )?;
        self.embedded_owned = true;
        Ok(())
    }
    pub fn send_game_control(&mut self, control: GameControlArg) -> io::Result<()> {
        self.send_command(Command::GameControl, &[control as u8])
    }
}
impl Drop for DeviceConnection {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    #[derive(Default)]
    struct State {
        writes: Vec<Vec<u8>>,
        response: VecDeque<u8>,
        fail: bool,
    }
    struct Fake(Rc<RefCell<State>>);
    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut s = self.0.borrow_mut();
            // Simulate fragmented replies, not a single 32-byte read.
            let n = buf.len().min(3).min(s.response.len());
            if n == 0 {
                return Err(io::ErrorKind::TimedOut.into());
            }
            for b in &mut buf[..n] {
                *b = s.response.pop_front().unwrap();
            }
            Ok(n)
        }
    }
    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut s = self.0.borrow_mut();
            if s.fail {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            s.writes.push(buf.to_vec());
            if buf.len() == 3 {
                let mut response = [0; 32];
                response[0] = if buf[2] == Command::Brightness as u8 {
                    51
                } else {
                    1
                };
                s.response.extend(response);
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Transport for Fake {
        fn clear_input(&mut self) -> io::Result<()> {
            self.0.borrow_mut().response.clear();
            Ok(())
        }
    }
    fn connected() -> (DeviceConnection, Rc<RefCell<State>>) {
        let state = Rc::new(RefCell::new(State::default()));
        let mut d = DeviceConnection::new();
        d.port = Some(Box::new(Fake(state.clone())));
        (d, state)
    }
    #[test]
    fn unchanged_frames_are_skipped_and_failure_invalidates_cache() {
        let (mut d, s) = connected();
        let grid = inputmodule_host::empty_grid();
        d.send_grid(&grid).unwrap();
        d.send_grid(&grid).unwrap();
        d.set_brightness(50).unwrap();
        d.set_brightness(50).unwrap();
        assert_eq!(s.borrow().writes.len(), 2);
        s.borrow_mut().fail = true;
        assert!(d.set_brightness(51).is_err());
        assert!(!d.is_connected());
        assert!(d.last_grid.is_none());
        s.borrow_mut().fail = false;
        d.port = Some(Box::new(Fake(s.clone())));
        d.send_grid(&grid).unwrap();
        assert_eq!(s.borrow().writes.len(), 3);
    }
    #[test]
    fn host_mode_restores_brightness_and_animation_on_drop() {
        let (mut d, s) = connected();
        d.prepare_host_mode().unwrap();
        d.set_brightness(200).unwrap();
        drop(d);
        let s = s.borrow();
        assert_eq!(s.writes[s.writes.len() - 2], [0x32, 0xac, 0, 51]);
        assert_eq!(s.writes[s.writes.len() - 1], [0x32, 0xac, 4, 1]);
    }
    #[test]
    fn disconnected_commands_and_oversized_payloads_fail_explicitly() {
        let mut d = DeviceConnection::new();
        assert_eq!(
            d.start_game(GameId::Pong).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            d.send_command(Command::Pattern, &[0; 62])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
