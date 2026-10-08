use std::time::{Duration, Instant};
use sysinfo::{MemoryRefreshKind, System, IS_SUPPORTED_SYSTEM, MINIMUM_CPU_UPDATE_INTERVAL};

use super::{Feature, FeatureKind};
use crate::games::{empty_grid, Grid, HEIGHT, WIDTH};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

#[derive(Default)]
struct Sample {
    cpu_percent: Option<f32>,
    used_memory: u64,
    total_memory: u64,
}

impl Sample {
    fn memory_percent(&self) -> Option<f32> {
        (self.total_memory > 0).then(|| {
            (self.used_memory.min(self.total_memory) as f64 / self.total_memory as f64 * 100.0)
                as f32
        })
    }

    fn grid(&self) -> Grid {
        let mut grid = empty_grid();
        for (columns, percent) in [
            (0..WIDTH / 2, self.cpu_percent),
            (WIDTH / 2 + 1..WIDTH, self.memory_percent()),
        ] {
            let height = percent
                .filter(|value| value.is_finite())
                .map(|value| (value.clamp(0.0, 100.0) / 100.0 * HEIGHT as f32).round() as usize)
                .unwrap_or(0);
            for column in columns {
                grid[column][HEIGHT - height..].fill(true);
            }
        }
        grid
    }
}

pub struct SystemMonitorFeature {
    system: System,
    sample: Sample,
    grid: Grid,
    last_refresh: Instant,
}

impl SystemMonitorFeature {
    pub fn new() -> Self {
        let mut system = System::new();
        // Establish the CPU baseline; its first reading is not a usage measurement.
        system.refresh_cpu_usage();
        system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let sample = Sample {
            cpu_percent: None,
            used_memory: system.used_memory(),
            total_memory: system.total_memory(),
        };
        Self {
            grid: sample.grid(),
            sample,
            system,
            last_refresh: Instant::now(),
        }
    }

    fn interval() -> Duration {
        Duration::from_secs(1).max(MINIMUM_CPU_UPDATE_INTERVAL)
    }
}

impl Feature for SystemMonitorFeature {
    fn kind(&self) -> FeatureKind {
        FeatureKind::SystemMonitor
    }

    fn tick(&mut self, now: Instant) {
        // Avoid immediate repeated refreshes when the app catches up missed ticks.
        if now.saturating_duration_since(self.last_refresh)
            < MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(1))
        {
            return;
        }
        self.system.refresh_cpu_usage();
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let cpu = self.system.global_cpu_usage();
        self.sample = Sample {
            cpu_percent: (!self.system.cpus().is_empty() && cpu.is_finite())
                .then_some(cpu.clamp(0.0, 100.0)),
            used_memory: self.system.used_memory(),
            total_memory: self.system.total_memory(),
        };
        self.grid = self.sample.grid();
        self.last_refresh = now;
    }

    fn grid(&self) -> Option<&Grid> {
        Some(&self.grid)
    }

    fn status_text(&self) -> String {
        if !IS_SUPPORTED_SYSTEM {
            return "System monitoring is unavailable on this operating system".into();
        }
        let cpu = match self.sample.cpu_percent {
            Some(percent) => format!("{percent:.0}%"),
            None if self.system.cpus().is_empty() => "unavailable".into(),
            None => "warming up".into(),
        };
        let ram = match self.sample.memory_percent() {
            Some(percent) => format!(
                "{percent:.0}% ({:.1} / {:.1} GiB)",
                self.sample.used_memory as f64 / GIB,
                self.sample.total_memory as f64 / GIB,
            ),
            None => "unavailable".into(),
        };
        format!("CPU (left): {cpu} | RAM (right): {ram}")
    }

    fn tick_interval_ms(&self) -> u64 {
        Self::interval().as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_are_bottom_aligned_with_cpu_left_ram_right_and_a_gap() {
        let grid = Sample {
            cpu_percent: Some(50.0),
            used_memory: 100,
            total_memory: 100,
        }
        .grid();
        for column in &grid[..4] {
            assert!(column[..17].iter().all(|&pixel| !pixel));
            assert!(column[17..].iter().all(|&pixel| pixel));
        }
        assert!(grid[4].iter().all(|&pixel| !pixel));
        for column in &grid[5..] {
            assert!(column.iter().all(|&pixel| pixel));
        }
    }

    #[test]
    fn missing_invalid_and_out_of_range_metrics_do_not_overflow_the_grid() {
        for cpu in [None, Some(f32::NAN), Some(f32::INFINITY), Some(-10.0)] {
            assert_eq!(
                Sample {
                    cpu_percent: cpu,
                    ..Sample::default()
                }
                .grid(),
                empty_grid()
            );
        }
        let sample = Sample {
            cpu_percent: Some(150.0),
            used_memory: 200,
            total_memory: 100,
        };
        assert_eq!(sample.memory_percent(), Some(100.0));
        let grid = sample.grid();
        assert_eq!(
            grid.iter().flatten().filter(|&&pixel| pixel).count(),
            8 * HEIGHT
        );
    }

    #[test]
    fn cpu_baseline_is_hidden_and_early_ticks_preserve_the_sample() {
        let mut feature = SystemMonitorFeature::new();
        assert!(feature.sample.cpu_percent.is_none());
        let grid = feature.grid;
        feature.tick(feature.last_refresh + MINIMUM_CPU_UPDATE_INTERVAL / 2);
        assert!(feature.sample.cpu_percent.is_none());
        assert_eq!(feature.grid, grid);
    }
}
