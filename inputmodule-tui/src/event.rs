use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEvent};
use std::io;
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Wake,
}
pub struct EventHandler;
impl EventHandler {
    pub fn next(&self, timeout: Duration) -> io::Result<AppEvent> {
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    return Ok(AppEvent::Key(key))
                }
                Event::Mouse(mouse) => return Ok(AppEvent::Mouse(mouse)),
                _ => {}
            }
        }
        // Resize and other events wake rendering too.
        Ok(AppEvent::Wake)
    }
}
