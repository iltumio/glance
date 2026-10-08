use rand::prelude::*;
use std::time::Instant;

use super::{Feature, FeatureKind};
use crate::games::{empty_grid, Grid, HEIGHT, WIDTH};

pub struct EqFeature {
    grid: Grid,
    rng: ThreadRng,
}

impl EqFeature {
    pub fn new() -> Self {
        let mut f = Self {
            grid: empty_grid(),
            rng: thread_rng(),
        };
        f.generate_frame();
        f
    }

    fn generate_frame(&mut self) {
        self.grid = empty_grid();

        // Generate 9 bar heights with weighted distribution (lower values more likely)
        let population: Vec<u8> = (1..34).collect();
        let vals: Vec<u8> = (0..WIDTH)
            .map(|_| {
                *population
                    .choose_weighted(&mut self.rng, |item| {
                        let v = 34u32 - *item as u32;
                        v * v
                    })
                    .unwrap_or(&1)
            })
            .collect();

        // Render centered bars
        for (col, &val) in vals.iter().enumerate() {
            let row = HEIGHT / 2;
            let above = val as usize / 2;
            let below = val as usize - above;

            for i in 0..above {
                if row + i < HEIGHT {
                    self.grid[col][row + i] = true;
                }
            }
            for i in 0..below {
                if row > i {
                    self.grid[col][row - 1 - i] = true;
                }
            }
        }
    }
}

impl Feature for EqFeature {
    fn kind(&self) -> FeatureKind {
        FeatureKind::Equalizer
    }

    fn tick(&mut self, _now: Instant) {
        self.generate_frame();
    }

    fn grid(&self) -> Option<&Grid> {
        Some(&self.grid)
    }

    fn status_text(&self) -> String {
        "Random equalizer visualization".to_string()
    }

    fn tick_interval_ms(&self) -> u64 {
        200
    }
}
