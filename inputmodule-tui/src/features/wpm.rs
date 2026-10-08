use super::{Feature, FeatureKind};
use crate::features::clock::render_string_to_grid;
use crate::games::Grid;
use crossterm::event::KeyCode;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct WpmFeature {
    start: Instant,
    keypresses: VecDeque<Instant>,
    current_wpm: usize,
    grid: Grid,
}

impl WpmFeature {
    pub fn new(now: Instant) -> Self {
        let mut feature = Self {
            start: now,
            keypresses: VecDeque::new(),
            current_wpm: 0,
            grid: crate::games::empty_grid(),
        };
        feature.update(now);
        feature
    }

    fn update(&mut self, now: Instant) {
        while self
            .keypresses
            .front()
            .is_some_and(|&t| now.saturating_duration_since(t) >= Duration::from_secs(10))
        {
            self.keypresses.pop_front();
        }
        // A short warm-up denominator prevents an enormous estimate on the first key.
        let seconds = now
            .saturating_duration_since(self.start)
            .as_secs_f64()
            .clamp(1.0, 10.0);
        self.current_wpm = (self.keypresses.len() as f64 * 12.0 / seconds) as usize;
        self.grid = render_string_to_grid(&format!(" {:>3}", self.current_wpm.min(9999)));
    }
}

impl Feature for WpmFeature {
    fn kind(&self) -> FeatureKind {
        FeatureKind::Wpm
    }
    fn tick(&mut self, now: Instant) {
        self.update(now);
    }
    fn handle_key(&mut self, key: KeyCode, now: Instant) {
        if matches!(
            key,
            KeyCode::Char(_) | KeyCode::Enter | KeyCode::Tab | KeyCode::Backspace
        ) {
            self.keypresses.push_back(now);
            self.update(now);
        }
    }
    fn grid(&self) -> Option<&Grid> {
        Some(&self.grid)
    }
    fn status_text(&self) -> String {
        format!("WPM: {} | Type to measure", self.current_wpm)
    }
    fn tick_interval_ms(&self) -> u64 {
        100
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rolling_window_expires_and_warmup_is_finite() {
        let start = Instant::now();
        let mut f = WpmFeature::new(start);
        f.handle_key(KeyCode::Char('a'), start);
        assert_eq!(f.current_wpm, 12);
        for _ in 0..49 {
            f.handle_key(KeyCode::Char('b'), start + Duration::from_secs(1));
        }
        f.tick(start + Duration::from_secs(10));
        assert_eq!(f.current_wpm, 58);
        assert_eq!(f.keypresses.len(), 49);
        f.tick(start + Duration::from_secs(11));
        assert_eq!(f.current_wpm, 0);
        assert!(f.keypresses.is_empty());
    }
}
