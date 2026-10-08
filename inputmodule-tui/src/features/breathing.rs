use std::time::Instant;

use super::{Feature, FeatureKind};
use crate::games::Grid;

/// Breathing animation — cycles the device brightness smoothly.
/// This feature doesn't draw a grid; it only controls brightness.
pub struct BreathingFeature {
    /// Current position in the breathing cycle.
    step: usize,
    current_brightness: u8,
}

impl BreathingFeature {
    pub fn new() -> Self {
        Self {
            step: 0,
            current_brightness: 250,
        }
    }

    fn cycle_len(&self) -> usize {
        40 + 50 + 50 + 40
    }

    fn brightness_at_step(&self, step: usize) -> u8 {
        if step < 40 {
            // 250 → 50
            (250 - step * 5) as u8
        } else if step < 90 {
            // 50 → 0
            let s = step - 40;
            (50 - s) as u8
        } else if step < 140 {
            // 0 → 50
            let s = step - 90;
            s as u8
        } else {
            // 50 → 250
            let s = step - 140;
            (50 + s * 5) as u8
        }
    }

    fn interval_at_step(&self, step: usize) -> u64 {
        if !(40..140).contains(&step) {
            25
        } else {
            10
        }
    }
}

impl Feature for BreathingFeature {
    fn kind(&self) -> FeatureKind {
        FeatureKind::Breathing
    }

    fn tick(&mut self, _now: Instant) {
        self.step = (self.step + 1) % self.cycle_len();
        self.current_brightness = self.brightness_at_step(self.step);
    }

    fn grid(&self) -> Option<&Grid> {
        None // Breathing doesn't draw a grid
    }

    fn status_text(&self) -> String {
        let bar_len = (self.current_brightness as usize) * 30 / 255;
        let bar: String = "█".repeat(bar_len);
        let empty: String = "░".repeat(30 - bar_len);
        format!(
            "Brightness: {:>3}  [{}{}]",
            self.current_brightness, bar, empty
        )
    }

    fn tick_interval_ms(&self) -> u64 {
        self.interval_at_step(self.step)
    }

    fn brightness(&self) -> Option<u8> {
        Some(self.current_brightness)
    }
}
