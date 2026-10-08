pub mod breathing;
pub mod clock;
pub mod eq;
pub mod system_monitor;
pub mod wpm;

use crate::games::Grid;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureKind {
    Clock,
    SevenSegmentClock,
    Equalizer,
    Breathing,
    SystemMonitor,
    Wpm,
}
impl FeatureKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Clock => "Clock",
            Self::SevenSegmentClock => "Clock (7-segment)",
            Self::Equalizer => "Equalizer",
            Self::Breathing => "Breathing",
            Self::SystemMonitor => "System Monitor",
            Self::Wpm => "WPM Demo",
        }
    }
}

pub trait Feature {
    fn kind(&self) -> FeatureKind;
    fn name(&self) -> &str {
        self.kind().name()
    }
    fn tick(&mut self, now: Instant);
    fn handle_key(&mut self, _key: crossterm::event::KeyCode, _now: Instant) {}
    fn grid(&self) -> Option<&Grid>;
    fn status_text(&self) -> String;
    fn tick_interval_ms(&self) -> u64;
    fn brightness(&self) -> Option<u8> {
        None
    }
}
