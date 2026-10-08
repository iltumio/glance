use super::{template::invalid, Size};
use crate::games::{empty_grid, Grid, HEIGHT, WIDTH};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProgressConfig {
    pub orientation: Orientation,
    pub length: usize,
    pub width: usize,
    pub min: f64,
    pub max: f64,
    pub value: f64,
    pub reverse: bool,
}
impl Default for ProgressConfig {
    fn default() -> Self {
        Self {
            orientation: Orientation::Horizontal,
            length: 3,
            width: 1,
            min: 0.0,
            max: 100.0,
            value: 0.0,
            reverse: false,
        }
    }
}
impl ProgressConfig {
    pub fn size(&self) -> Size {
        match self.orientation {
            Orientation::Horizontal => Size {
                width: self.length,
                height: self.width,
            },
            Orientation::Vertical => Size {
                width: self.width,
                height: self.length,
            },
        }
    }
    pub fn validate(&self) -> io::Result<()> {
        let size = self.size();
        if self.length < 3 || self.width == 0 || size.width > WIDTH || size.height > HEIGHT {
            return Err(invalid(
                "Progress bar requires length >= 3, width >= 1 and a footprint within 9x34",
            ));
        }
        if !self.min.is_finite()
            || !self.max.is_finite()
            || !self.value.is_finite()
            || self.min >= self.max
            || !(self.max - self.min).is_finite()
        {
            return Err(invalid("Progress range must be finite with min < max and a finite span; value must be finite"));
        }
        Ok(())
    }
    pub fn fraction(&self, value: f64) -> f64 {
        (value.clamp(self.min, self.max) - self.min) / (self.max - self.min)
    }
    pub fn frame(&self, value: f64) -> Grid {
        let mut grid = empty_grid();
        let filled = (self.fraction(value) * self.length as f64).floor() as usize;
        for along in 0..filled {
            let offset = if self.reverse {
                self.length - 1 - along
            } else {
                along
            };
            for across in 0..self.width {
                let (x, y) = match self.orientation {
                    Orientation::Horizontal => (offset, across),
                    Orientation::Vertical => (across, self.length - 1 - offset),
                };
                grid[x][y] = true;
            }
        }
        grid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bars_fill_whole_leds_in_both_orientations_and_clamp_values() {
        let mut config = ProgressConfig {
            length: 6,
            width: 2,
            ..Default::default()
        };
        assert_eq!(config.frame(-1.0), empty_grid());
        let frame = config.frame(50.0);
        for (x, col) in frame.iter().enumerate() {
            for (y, &pixel) in col.iter().enumerate() {
                assert_eq!(pixel, x < 3 && y < 2);
            }
        }
        config.orientation = Orientation::Vertical;
        let frame = config.frame(50.0);
        assert!(frame[0][5] && frame[1][3] && !frame[0][2]);
        config.reverse = true;
        let frame = config.frame(50.0);
        assert!(frame[0][0] && frame[1][2] && !frame[0][3]);
        assert_eq!(
            config.frame(200.0).iter().flatten().filter(|&&v| v).count(),
            12
        );
    }
    #[test]
    fn rejects_bad_dimensions_and_ranges_before_rendering() {
        for config in [
            ProgressConfig {
                length: 2,
                ..Default::default()
            },
            ProgressConfig {
                width: 0,
                ..Default::default()
            },
            ProgressConfig {
                length: usize::MAX,
                ..Default::default()
            },
            ProgressConfig {
                min: 100.0,
                ..Default::default()
            },
            ProgressConfig {
                max: f64::INFINITY,
                ..Default::default()
            },
            ProgressConfig {
                value: f64::NAN,
                ..Default::default()
            },
            ProgressConfig {
                min: -f64::MAX,
                max: f64::MAX,
                ..Default::default()
            },
        ] {
            assert!(config.validate().is_err());
        }
    }
}
