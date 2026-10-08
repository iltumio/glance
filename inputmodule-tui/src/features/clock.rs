use chrono::{Local, NaiveTime, Timelike};
use std::time::Instant;

use super::{Feature, FeatureKind};
use crate::games::{empty_grid, Grid, HEIGHT, WIDTH};
pub use inputmodule_host::render_string as render_string_to_grid;

pub struct ClockFeature {
    grid: Grid,
    style: ClockStyle,
    time: NaiveTime,
}

#[derive(Clone, Copy)]
enum ClockStyle {
    Standard,
    SevenSegment,
}

impl ClockFeature {
    pub fn new() -> Self {
        Self::with_style(ClockStyle::Standard)
    }

    pub fn seven_segment() -> Self {
        Self::with_style(ClockStyle::SevenSegment)
    }

    fn with_style(style: ClockStyle) -> Self {
        let mut f = Self {
            grid: empty_grid(),
            style,
            time: Local::now().time(),
        };
        f.update_time();
        f
    }

    fn update_time(&mut self) {
        self.time = Local::now().time();
        self.grid = match self.style {
            ClockStyle::Standard => render_string_to_grid(&self.time.format("%H:%M").to_string()),
            ClockStyle::SevenSegment => seven_segment_grid(self.time),
        };
    }
}

// Conventional a/b/c/d/e/f/g segment masks for digits 0..9.
const DIGIT_SEGMENTS: [u8; 10] = [0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f];
const SEGMENT_PIXELS: [[(usize, usize); 3]; 7] = [
    [(0, 0), (1, 0), (2, 0)], // a: top
    [(2, 0), (2, 1), (2, 2)], // b: upper right
    [(2, 2), (2, 3), (2, 4)], // c: lower right
    [(0, 4), (1, 4), (2, 4)], // d: bottom
    [(0, 2), (0, 3), (0, 4)], // e: lower left
    [(0, 0), (0, 1), (0, 2)], // f: upper left
    [(0, 2), (1, 2), (2, 2)], // g: middle
];
const COMPACT_HEIGHT: usize = 11;
const COMPACT_TOP: usize = (HEIGHT - COMPACT_HEIGHT) / 2;

fn draw_digit(grid: &mut Grid, digit: u32, x: usize, y: usize) {
    let mask = DIGIT_SEGMENTS[digit as usize];
    for (segment, pixels) in SEGMENT_PIXELS.iter().enumerate() {
        if mask & (1 << segment) != 0 {
            for &(dx, dy) in pixels {
                grid[x + dx][y + dy] = true;
            }
        }
    }
}

fn seven_segment_grid(time: NaiveTime) -> Grid {
    let mut grid = empty_grid();
    // Two horizontal digits fit in seven columns; hours above, minutes below.
    let left = (WIDTH - 7) / 2;
    for (value, y) in [(time.hour(), COMPACT_TOP), (time.minute(), COMPACT_TOP + 6)] {
        draw_digit(&mut grid, value / 10, left, y);
        draw_digit(&mut grid, value % 10, left + 4, y);
    }
    grid
}

impl Feature for ClockFeature {
    fn kind(&self) -> FeatureKind {
        match self.style {
            ClockStyle::Standard => FeatureKind::Clock,
            ClockStyle::SevenSegment => FeatureKind::SevenSegmentClock,
        }
    }

    fn tick(&mut self, _now: Instant) {
        self.update_time();
    }

    fn grid(&self) -> Option<&Grid> {
        Some(&self.grid)
    }

    fn status_text(&self) -> String {
        format!(
            "Current time: {} | local, 24h",
            self.time.format("%H:%M:%S")
        )
    }

    fn tick_interval_ms(&self) -> u64 {
        1000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(grid: &Grid, x: usize, y: usize, width: usize, height: usize) -> Vec<String> {
        (y..y + height)
            .map(|row| {
                (x..x + width)
                    .map(|col| if grid[col][row] { '#' } else { '.' })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn all_ten_digits_have_the_expected_seven_segment_shape() {
        let expected = [
            ["###", "#.#", "#.#", "#.#", "###"],
            ["..#", "..#", "..#", "..#", "..#"],
            ["###", "..#", "###", "#..", "###"],
            ["###", "..#", "###", "..#", "###"],
            ["#.#", "#.#", "###", "..#", "..#"],
            ["###", "#..", "###", "..#", "###"],
            ["###", "#..", "###", "#.#", "###"],
            ["###", "..#", "..#", "..#", "..#"],
            ["###", "#.#", "###", "#.#", "###"],
            ["###", "#.#", "###", "..#", "###"],
        ];
        for (digit, shape) in expected.iter().enumerate() {
            let mut grid = empty_grid();
            draw_digit(&mut grid, digit as u32, 0, 0);
            assert_eq!(rows(&grid, 0, 0, 3, 5), *shape, "digit {digit}");
        }
    }

    #[test]
    fn compact_clock_is_centered_with_a_blank_row_and_no_blinking() {
        let on = seven_segment_grid(NaiveTime::from_hms_opt(12, 34, 0).unwrap());
        let off = seven_segment_grid(NaiveTime::from_hms_opt(12, 34, 1).unwrap());
        assert_eq!(
            rows(&on, 1, COMPACT_TOP, 7, 5),
            ["..#.###", "..#...#", "..#.###", "..#.#..", "..#.###"]
        );
        assert_eq!(
            rows(&on, 1, COMPACT_TOP + 6, 7, 5),
            ["###.#.#", "..#.#.#", "###.###", "..#...#", "###...#"]
        );
        assert_eq!(on, off);
        assert!(on.iter().all(|column| !column[COMPACT_TOP + 5]));
        for (x, column) in on.iter().enumerate() {
            for (y, &pixel) in column.iter().enumerate() {
                if !(1..8).contains(&x) || !(COMPACT_TOP..COMPACT_TOP + COMPACT_HEIGHT).contains(&y)
                {
                    assert!(!pixel);
                }
            }
        }
    }

    #[test]
    fn midnight_keeps_leading_zeros_and_last_minute_fits() {
        let midnight = seven_segment_grid(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let expected = ["###.###", "#.#.#.#", "#.#.#.#", "#.#.#.#", "###.###"];
        assert_eq!(rows(&midnight, 1, COMPACT_TOP, 7, 5), expected);
        assert_eq!(rows(&midnight, 1, COMPACT_TOP + 6, 7, 5), expected);
        let last = seven_segment_grid(NaiveTime::from_hms_opt(23, 59, 59).unwrap());
        assert_eq!(
            rows(&last, 1, COMPACT_TOP + 6, 7, 5),
            ["###.###", "#...#.#", "###.###", "..#...#", "###.###"]
        );
    }
}
