pub mod progress;
pub mod progress_editor;
mod simulations;
pub mod template;

use crate::features::{
    breathing::BreathingFeature, clock::ClockFeature, eq::EqFeature,
    system_monitor::SystemMonitorFeature, wpm::WpmFeature, Feature,
};
use crate::games::{
    empty_grid, ledris::LedrisGame, snake::SnakeGame, Game, GameInput, GameStatus, Grid, HEIGHT,
    WIDTH,
};
use crate::inputs::{Binding, InputHub, InputService};
use crossterm::event::KeyCode;
use progress::ProgressConfig;
use serde::{Deserialize, Serialize};
use simulations::{Life, Pong};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};
pub use template::{Bounds, Placement, Size, Template};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    Snake,
    Ledris,
    Pong,
    #[serde(rename = "game_of_life")]
    Life,
    SystemMonitor,
    Clock,
    SevenSegmentClock,
    Equalizer,
    Breathing,
    Wpm,
    ProgressBar,
}

pub const CATALOG: [WidgetKind; 11] = [
    WidgetKind::Snake,
    WidgetKind::Ledris,
    WidgetKind::Pong,
    WidgetKind::Life,
    WidgetKind::SystemMonitor,
    WidgetKind::Clock,
    WidgetKind::SevenSegmentClock,
    WidgetKind::Equalizer,
    WidgetKind::Breathing,
    WidgetKind::Wpm,
    WidgetKind::ProgressBar,
];

impl WidgetKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Snake => "Snake",
            Self::Ledris => "Ledris",
            Self::Pong => "Pong",
            Self::Life => "Game of Life",
            Self::SystemMonitor => "System Monitor",
            Self::Clock => "Clock",
            Self::SevenSegmentClock => "Clock (7-segment)",
            Self::Equalizer => "Equalizer",
            Self::Breathing => "Breathing",
            Self::Wpm => "WPM",
            Self::ProgressBar => "Progress Bar",
        }
    }
    pub fn size(self) -> Size {
        let (width, height) = match self {
            Self::Snake | Self::Ledris | Self::Pong => (9, 34),
            Self::Life | Self::SystemMonitor => (9, 12),
            Self::Clock => (5, 34),
            Self::SevenSegmentClock => (7, 11),
            Self::Equalizer => (9, 10),
            Self::Breathing => (9, 6),
            Self::Wpm => (5, 34),
            Self::ProgressBar => (3, 1),
        };
        Size { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LifePattern {
    #[default]
    Glider,
    Blinker,
    Toad,
    Beacon,
    BeaconToadBlinker,
    Random,
    EasterEgg,
}
impl LifePattern {
    pub fn next(self) -> Self {
        match self {
            Self::Glider => Self::Blinker,
            Self::Blinker => Self::Toad,
            Self::Toad => Self::Beacon,
            Self::Beacon => Self::BeaconToadBlinker,
            Self::BeaconToadBlinker => Self::Random,
            Self::Random => Self::EasterEgg,
            Self::EasterEgg => Self::Glider,
        }
    }
}

enum Engine {
    Game {
        game: Box<dyn Game>,
        ended: Option<(GameStatus, Instant)>,
    },
    Feature(Box<dyn Feature>),
    Life(Box<Life>),
    Pong(Pong),
    Progress {
        config: ProgressConfig,
        value: f64,
        error: Option<String>,
    },
}

struct LiveWidget {
    engine: Engine,
    next_tick: Instant,
}
impl LiveWidget {
    fn new(placement: &Placement, now: Instant) -> Self {
        let engine = match placement.kind {
            WidgetKind::Snake => Engine::Game {
                game: Box::new(SnakeGame::new()),
                ended: None,
            },
            WidgetKind::Ledris => Engine::Game {
                game: Box::new(LedrisGame::new()),
                ended: None,
            },
            WidgetKind::Pong => Engine::Pong(Pong::new()),
            WidgetKind::Life => {
                Engine::Life(Box::new(Life::new(placement.pattern.unwrap_or_default())))
            }
            WidgetKind::SystemMonitor => Engine::Feature(Box::new(SystemMonitorFeature::new())),
            WidgetKind::Clock => Engine::Feature(Box::new(ClockFeature::new())),
            WidgetKind::SevenSegmentClock => {
                Engine::Feature(Box::new(ClockFeature::seven_segment()))
            }
            WidgetKind::Equalizer => Engine::Feature(Box::new(EqFeature::new())),
            WidgetKind::Breathing => Engine::Feature(Box::new(BreathingFeature::new())),
            WidgetKind::Wpm => Engine::Feature(Box::new(WpmFeature::new(now))),
            WidgetKind::ProgressBar => {
                let config = placement.progress.clone().unwrap_or_default();
                Engine::Progress {
                    value: config.value,
                    config,
                    error: None,
                }
            }
        };
        let mut live = Self {
            engine,
            next_tick: now,
        };
        live.next_tick = now + live.interval();
        live
    }
    fn interval(&self) -> Duration {
        Duration::from_millis(match &self.engine {
            Engine::Game { game, ended } => {
                if ended.is_some() {
                    100
                } else {
                    game.tick_interval_ms()
                }
            }
            Engine::Feature(feature) => feature.tick_interval_ms(),
            Engine::Life(_) => 300,
            Engine::Pong(_) => 100,
            Engine::Progress { .. } => 100,
        })
    }
    fn tick(&mut self, now: Instant, interactive: bool) {
        match &mut self.engine {
            Engine::Game { game, ended } => {
                if ended.is_none() {
                    let result = game.tick();
                    if result != GameStatus::Running {
                        *ended = Some((result, now));
                    }
                }
            }
            Engine::Feature(feature) => feature.tick(now),
            Engine::Life(life) => life.tick(),
            Engine::Pong(pong) => pong.tick(interactive),
            Engine::Progress { .. } => {}
        }
    }
    fn key(&mut self, kind: WidgetKind, key: KeyCode, now: Instant) {
        match &mut self.engine {
            Engine::Feature(feature) => feature.handle_key(key, now),
            Engine::Pong(pong) => pong.handle_key(key),
            Engine::Game { game, ended } if ended.is_none() => {
                let input = match key {
                    KeyCode::Up | KeyCode::Char('k') if kind == WidgetKind::Ledris => {
                        Some(GameInput::RotateCW)
                    }
                    KeyCode::Up | KeyCode::Char('k') => Some(GameInput::Up),
                    KeyCode::Down | KeyCode::Char('j') => Some(GameInput::Down),
                    KeyCode::Left | KeyCode::Char('h') => Some(GameInput::Left),
                    KeyCode::Right | KeyCode::Char('l') => Some(GameInput::Right),
                    KeyCode::Char(' ') => Some(GameInput::HardDrop),
                    _ => None,
                };
                if let Some(input) = input {
                    let result = game.handle_input(input);
                    if result != GameStatus::Running {
                        *ended = Some((result, now));
                    }
                    if input == GameInput::HardDrop {
                        self.next_tick = now + self.interval();
                    }
                }
            }
            _ => {}
        }
    }
    fn frame(&self, kind: WidgetKind, now: Instant) -> Grid {
        let size = kind.size();
        match &self.engine {
            Engine::Game { game, ended } => {
                if let Some((_, started)) = ended {
                    let phase = now.saturating_duration_since(*started).as_millis() / 300;
                    if phase < 4 {
                        return [[phase % 2 == 1; HEIGHT]; WIDTH];
                    }
                }
                *game.grid()
            }
            Engine::Life(life) => life.grid(),
            Engine::Pong(pong) => pong.grid(),
            Engine::Progress { config, value, .. } => config.frame(*value),
            Engine::Feature(feature) => {
                let mut grid = empty_grid();
                if kind == WidgetKind::Breathing {
                    // Local monochrome density, never the matrix's global brightness.
                    let lit =
                        usize::from(feature.brightness().unwrap_or(0)) * size.width * size.height
                            / 255;
                    for index in 0..lit {
                        // Permute cell order to distribute the illuminated pixels.
                        let cell = index * 17 % (size.width * size.height);
                        grid[cell % size.width][cell / size.width] = true;
                    }
                } else if let Some(source) = feature.grid() {
                    if matches!(kind, WidgetKind::SystemMonitor | WidgetKind::Equalizer) {
                        for (x, column) in grid.iter_mut().enumerate().take(size.width) {
                            if kind == WidgetKind::SystemMonitor {
                                let height = (source[x].iter().filter(|&&cell| cell).count()
                                    * size.height
                                    + HEIGHT / 2)
                                    / HEIGHT;
                                column[size.height - height..size.height].fill(true);
                            } else {
                                for (y, cell) in column[..size.height].iter_mut().enumerate() {
                                    let start = y * HEIGHT / size.height;
                                    let end = (y + 1) * HEIGHT / size.height;
                                    *cell = source[x][start..end].iter().any(|&pixel| pixel);
                                }
                            }
                        }
                    } else {
                        let (sx, sy) = if kind == WidgetKind::SevenSegmentClock {
                            (1, 11)
                        } else {
                            (2, 0)
                        };
                        for (x, column) in grid.iter_mut().enumerate().take(size.width) {
                            column[..size.height]
                                .copy_from_slice(&source[x + sx][sy..sy + size.height]);
                        }
                    }
                }
                grid
            }
        }
    }
    fn status(&self) -> String {
        match &self.engine {
            Engine::Game { game, ended } => format!(
                "{} | Score: {}{}",
                game.name(),
                game.score(),
                match ended {
                    Some((GameStatus::Won, _)) => " | YOU WIN | r: restart",
                    Some(_) => " | GAME OVER | r: restart",
                    None => "",
                }
            ),
            Engine::Feature(feature) => {
                if feature.kind() == crate::features::FeatureKind::Breathing {
                    format!(
                        "Breathing intensity: {:.0}% | local pixel density",
                        f32::from(feature.brightness().unwrap_or(0)) * 100.0 / 255.0
                    )
                } else {
                    feature.status_text()
                }
            }
            Engine::Life(life) => life.status(),
            Engine::Pong(pong) => pong.status(),
            Engine::Progress {
                config,
                value,
                error,
            } => format!(
                "Progress: {:.1}% | value: {} [{}, {}]{}",
                config.fraction(*value) * 100.0,
                value,
                config.min,
                config.max,
                error
                    .as_ref()
                    .map(|e| format!(" | Input error: {e}"))
                    .unwrap_or_default()
            ),
        }
    }
}

/// One validated layout and one rendering/update engine, shared by TUI and headless playback.
pub struct Runtime {
    template: Template,
    widgets: Vec<LiveWidget>,
    now: Instant,
    inputs: InputService,
}
impl Runtime {
    #[cfg(test)]
    pub fn new(template: Template, now: Instant) -> io::Result<Self> {
        Self::with_base(template, now, Path::new("."))
    }
    pub fn with_base(template: Template, now: Instant, base: &Path) -> io::Result<Self> {
        template.validate()?;
        let mut inputs = InputService::new(base);
        inputs.reconcile(Self::bindings(&template))?;
        let widgets = template
            .widgets
            .iter()
            .map(|placement| LiveWidget::new(placement, now))
            .collect();
        Ok(Self {
            template,
            widgets,
            now,
            inputs,
        })
    }
    fn bindings(template: &Template) -> Vec<Binding> {
        template
            .widgets
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind == WidgetKind::ProgressBar)
            .map(|(index, p)| Binding {
                name: p.input_key(index),
                source: p.input.clone(),
                initial: p.progress.clone().unwrap_or_default().value,
            })
            .collect()
    }
    pub fn input_errors(&self) -> Vec<String> {
        let hub = self.inputs.hub();
        self.template
            .widgets
            .iter()
            .enumerate()
            .filter_map(|(index, p)| {
                let name = p.input_key(index);
                let error = hub.snapshot(&name)?.error?;
                Some(format!("Input {name}: {error}"))
            })
            .collect()
    }
    pub fn input_hub(&self) -> InputHub {
        self.inputs.hub()
    }
    pub fn configure(&mut self, index: usize, placement: Placement) -> io::Result<()> {
        if index >= self.widgets.len() {
            return Err(template::invalid("Unknown widget index"));
        }
        let mut candidate = self.template.clone();
        candidate.widgets[index] = placement.clone();
        candidate.validate()?;
        self.inputs.reconcile(Self::bindings(&candidate))?;
        self.widgets[index] = LiveWidget::new(&placement, self.now);
        self.template = candidate;
        self.update(self.now, None);
        Ok(())
    }
    pub fn template(&self) -> &Template {
        &self.template
    }
    pub fn add(&mut self, kind: WidgetKind) -> io::Result<usize> {
        let (x, y) = self
            .template
            .first_slot(kind)
            .ok_or_else(|| template::invalid("No free rectangle fits this widget"))?;
        let mut placement = Placement::new(kind, x, y);
        if kind == WidgetKind::ProgressBar {
            let mut number = 1;
            loop {
                let name = format!("progress-{number}");
                if !self
                    .template
                    .widgets
                    .iter()
                    .any(|p| p.name.as_ref() == Some(&name))
                {
                    placement.name = Some(name);
                    break;
                }
                number += 1;
            }
            placement.progress = Some(ProgressConfig::default());
        }
        let mut candidate = self.template.clone();
        candidate.widgets.push(placement.clone());
        candidate.validate()?;
        self.inputs.reconcile(Self::bindings(&candidate))?;
        self.widgets.push(LiveWidget::new(&placement, self.now));
        self.template.widgets.push(placement);
        Ok(self.widgets.len() - 1)
    }
    pub fn remove(&mut self, index: usize) {
        if index < self.widgets.len() {
            self.widgets.remove(index);
            self.template.widgets.remove(index);
            self.inputs
                .reconcile(Self::bindings(&self.template))
                .expect("removal cannot start a new input worker");
        }
    }
    pub fn move_to(&mut self, index: usize, x: usize, y: usize) -> bool {
        let Some(placement) = self.template.widgets.get(index) else {
            return false;
        };
        if !self.template.can_place(
            Bounds {
                x,
                y,
                size: placement.size(),
            },
            Some(index),
        ) {
            return false;
        }
        self.template.widgets[index].x = x;
        self.template.widgets[index].y = y;
        true
    }
    pub fn update(&mut self, now: Instant, playing: Option<usize>) {
        self.now = now;
        let hub = self.inputs.hub();
        for (index, widget) in self.widgets.iter_mut().enumerate() {
            if let Engine::Progress { value, error, .. } = &mut widget.engine {
                if let Some(snapshot) = hub.snapshot(&self.template.widgets[index].input_key(index))
                {
                    *value = snapshot.value;
                    *error = snapshot.error;
                }
            }
            if let Engine::Game {
                ended: Some((_, started)),
                ..
            } = &widget.engine
            {
                if playing != Some(index)
                    && now.saturating_duration_since(*started) >= Duration::from_secs(2)
                {
                    *widget = LiveWidget::new(&self.template.widgets[index], now);
                }
            }
            for _ in 0..8 {
                if now < widget.next_tick {
                    break;
                }
                widget.tick(now, playing == Some(index));
                widget.next_tick += widget.interval();
            }
            if now >= widget.next_tick {
                widget.next_tick = now + widget.interval();
            }
        }
    }
    pub fn poll_timeout(&self, now: Instant) -> Duration {
        self.widgets
            .iter()
            .fold(Duration::from_millis(100), |wait, widget| {
                wait.min(widget.next_tick.saturating_duration_since(now))
            })
    }
    pub fn frame(&self) -> Grid {
        let mut frame = empty_grid();
        for (placement, widget) in self.template.widgets.iter().zip(&self.widgets) {
            let local = widget.frame(placement.kind, self.now);
            let size = placement.size();
            for x in 0..size.width {
                frame[placement.x + x][placement.y..placement.y + size.height]
                    .copy_from_slice(&local[x][..size.height]);
            }
        }
        frame
    }
    pub fn restart(&mut self, index: usize) {
        if let Some(placement) = self.template.widgets.get(index) {
            self.widgets[index] = LiveWidget::new(placement, self.now);
        }
    }
    pub fn handle_key(&mut self, index: usize, key: KeyCode, now: Instant) {
        let Some(placement) = self.template.widgets.get_mut(index) else {
            return;
        };
        if placement.kind == WidgetKind::Life && key == KeyCode::Char('n') {
            placement.pattern = Some(placement.pattern.unwrap_or_default().next());
            self.widgets[index] = LiveWidget::new(placement, now);
        } else {
            self.widgets[index].key(placement.kind, key, now);
        }
    }
    pub fn status(&self, index: usize) -> String {
        self.widgets
            .get(index)
            .map(LiveWidget::status)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn api_values_render_and_invalid_resize_is_transactional() {
        let now = Instant::now();
        let mut runtime = Runtime::new(Template::default(), now).unwrap();
        let index = runtime.add(WidgetKind::ProgressBar).unwrap();
        let mut bar = runtime.template().widgets[index].clone();
        bar.progress = Some(ProgressConfig {
            length: 9,
            width: 2,
            ..Default::default()
        });
        bar.input = Some(crate::inputs::InputSource::Api);
        runtime.configure(index, bar.clone()).unwrap();
        let hub = runtime.input_hub();
        let name = bar.name.clone().unwrap();
        hub.api_update(&name, 50.0).unwrap();
        runtime.update(now + Duration::from_millis(100), None);
        assert_eq!(runtime.frame().iter().flatten().filter(|&&v| v).count(), 8);
        runtime.add(WidgetKind::ProgressBar).unwrap();
        let old = runtime.template().clone();
        let mut invalid = bar.clone();
        invalid.progress.as_mut().unwrap().width = 34;
        assert!(runtime.configure(index, invalid).is_err());
        assert_eq!(runtime.template(), &old);
        assert_eq!(hub.api_get(&name).unwrap().value, 50.0);
        bar.name = Some("renamed".into());
        runtime.configure(index, bar).unwrap();
        assert!(hub.api_get(&name).is_err());
        assert_eq!(hub.api_get("renamed").unwrap().value, 0.0);
        runtime.remove(index);
        assert!(hub.api_get("renamed").is_err());
    }

    #[test]
    fn composition_moves_without_reset_and_never_draws_outside_footprints() {
        let now = Instant::now();
        let mut runtime = Runtime::new(Template::default(), now).unwrap();
        let clock = runtime.add(WidgetKind::SevenSegmentClock).unwrap();
        let life = runtime.add(WidgetKind::Life).unwrap();
        assert!(!runtime.move_to(life, 0, 0));
        let initial = runtime.frame();
        assert!(runtime.move_to(clock, 2, 0));
        let moved = runtime.frame();
        for x in 0..7 {
            assert_eq!(&initial[x][..11], &moved[x + 2][..11]);
        }
        runtime.update(now + Duration::from_secs(1), None);
        for (x, column) in runtime.frame().iter().enumerate() {
            for (y, &cell) in column.iter().enumerate() {
                if cell {
                    assert!(runtime
                        .template
                        .widgets
                        .iter()
                        .any(|widget| widget.bounds().contains(x, y)));
                }
            }
        }
        runtime.remove(clock);
        assert!(runtime
            .template
            .first_slot(WidgetKind::SevenSegmentClock)
            .is_some());
    }
    #[test]
    fn every_catalog_widget_runs_within_its_declared_size() {
        let now = Instant::now();
        for kind in CATALOG {
            let mut runtime = Runtime::new(Template::default(), now).unwrap();
            runtime.add(kind).unwrap();
            runtime.update(now + Duration::from_secs(1), None);
            let size = kind.size();
            for (x, column) in runtime.frame().iter().enumerate() {
                for (y, &cell) in column.iter().enumerate() {
                    assert!(
                        !cell || (x < size.width && y < size.height),
                        "{kind:?}: {x},{y}"
                    );
                }
            }
        }
    }
}
