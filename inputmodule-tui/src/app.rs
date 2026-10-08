use crate::composer::Composer;
use crate::features::{
    breathing::BreathingFeature, clock::ClockFeature, eq::EqFeature,
    system_monitor::SystemMonitorFeature, wpm::WpmFeature, Feature, FeatureKind,
};
use crate::games::{
    game_of_life::PATTERNS, ledris::LedrisGame, snake::SnakeGame, Game, GameInput, GameKind,
    GameStatus, Grid, HEIGHT, WIDTH,
};
use crate::serial::protocol::{Command, GameControlArg, GameId};
use crate::serial::{self, DeviceConnection};
use crate::widgets::Template;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io;
use std::time::{Duration, Instant};

const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const MAX_CATCH_UP: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    Game(GameKind),
    Pong,
    Life,
    Feature(FeatureKind),
}
pub const MENU_ITEMS: [MenuItem; 10] = [
    MenuItem::Game(GameKind::Snake),
    MenuItem::Game(GameKind::Ledris),
    MenuItem::Pong,
    MenuItem::Life,
    MenuItem::Feature(FeatureKind::SystemMonitor),
    MenuItem::Feature(FeatureKind::Clock),
    MenuItem::Feature(FeatureKind::SevenSegmentClock),
    MenuItem::Feature(FeatureKind::Equalizer),
    MenuItem::Feature(FeatureKind::Breathing),
    MenuItem::Feature(FeatureKind::Wpm),
];
impl MenuItem {
    pub fn label(self) -> &'static str {
        match self {
            Self::Game(g) => g.name(),
            Self::Pong => "Pong (embedded)",
            Self::Life => "Game of Life (embedded)",
            Self::Feature(f) => f.name(),
        }
    }
}

pub enum AppState {
    Menu {
        selected: usize,
    },
    Playing {
        game: Box<dyn Game>,
        next_tick: Instant,
    },
    GameOver {
        game: Box<dyn Game>,
        won: bool,
        started: Instant,
    },
    EmbeddedGame {
        game: GameId,
    },
    EmbeddedGameOfLife {
        selected_pattern: usize,
    },
    Feature {
        feature: Box<dyn Feature>,
        next_tick: Instant,
    },
}

pub struct App {
    pub running: bool,
    pub state: AppState,
    pub device: DeviceConnection,
    pub error: Option<String>,
    pub now: Instant,
    pub composer: Option<Composer>,
    no_device: bool,
    requested: Option<String>,
    next_reconnect: Instant,
    last_keepalive: Instant,
}

impl App {
    pub fn new(no_device: bool, requested: Option<String>, now: Instant) -> Self {
        Self {
            running: true,
            state: AppState::Menu { selected: 0 },
            device: DeviceConnection::new(),
            error: None,
            now,
            composer: None,
            no_device,
            requested,
            next_reconnect: now,
            last_keepalive: now,
        }
    }

    pub fn enable_composer(
        &mut self,
        template: Template,
        path: std::path::PathBuf,
    ) -> io::Result<()> {
        self.composer = Some(Composer::new(template, path, self.now)?);
        self.prepare_host();
        Ok(())
    }

    pub fn discover_device(&mut self) {
        if self.no_device || self.device.is_connected() {
            return;
        }
        self.next_reconnect = self.now + RECONNECT_INTERVAL;
        let result = serial::find_device(self.requested.as_deref()).and_then(|port| {
            if let Some(port) = port {
                self.device.connect(&port)?;
                Ok(true)
            } else {
                Ok(false)
            }
        });
        match result {
            Ok(true) => {
                self.error = None;
                self.last_keepalive = self.now;
                if self.composer.is_some() {
                    self.prepare_host();
                }
            }
            Ok(false) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn record(&mut self, result: io::Result<()>) -> bool {
        if let Err(error) = result {
            self.error = Some(error.to_string());
            false
        } else {
            true
        }
    }

    fn menu(&mut self) -> AppState {
        let result = self.device.release();
        self.record(result);
        self.next_reconnect = self.now;
        AppState::Menu { selected: 0 }
    }

    fn new_game(&mut self, kind: GameKind) -> AppState {
        self.prepare_host();
        let game: Box<dyn Game> = match kind {
            GameKind::Snake => Box::new(SnakeGame::new()),
            GameKind::Ledris => Box::new(LedrisGame::new()),
        };
        let next_tick = self.now + Duration::from_millis(game.tick_interval_ms());
        AppState::Playing { game, next_tick }
    }

    fn prepare_host(&mut self) {
        if self.device.is_connected() {
            let result = self.device.prepare_host_mode();
            self.record(result);
        }
    }

    fn launch(&mut self, item: MenuItem, selected: usize) -> AppState {
        match item {
            MenuItem::Game(kind) => self.new_game(kind),
            MenuItem::Feature(kind) => {
                self.prepare_host();
                let feature: Box<dyn Feature> = match kind {
                    FeatureKind::Clock => Box::new(ClockFeature::new()),
                    FeatureKind::SevenSegmentClock => Box::new(ClockFeature::seven_segment()),
                    FeatureKind::Equalizer => Box::new(EqFeature::new()),
                    FeatureKind::Breathing => Box::new(BreathingFeature::new()),
                    FeatureKind::Wpm => Box::new(WpmFeature::new(self.now)),
                    FeatureKind::SystemMonitor => Box::new(SystemMonitorFeature::new()),
                };
                let next_tick = self.now + Duration::from_millis(feature.tick_interval_ms());
                AppState::Feature { feature, next_tick }
            }
            MenuItem::Pong => {
                let result = self.device.start_game(GameId::Pong);
                if self.record(result) {
                    AppState::EmbeddedGame { game: GameId::Pong }
                } else {
                    AppState::Menu { selected }
                }
            }
            MenuItem::Life => {
                if self.device.is_connected() {
                    AppState::EmbeddedGameOfLife {
                        selected_pattern: 0,
                    }
                } else {
                    self.error = Some("Game of Life requires a connected LED Matrix".into());
                    AppState::Menu { selected }
                }
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, now: Instant) {
        self.now = now;
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return;
        }
        if let Some(composer) = &mut self.composer {
            if composer.handle_key(key.code, now) {
                self.running = false;
            }
            return;
        }
        let state = std::mem::replace(&mut self.state, AppState::Menu { selected: 0 });
        self.state = match state {
            AppState::Menu { selected } => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.running = false;
                    AppState::Menu { selected }
                }
                KeyCode::Up => AppState::Menu {
                    selected: (selected + MENU_ITEMS.len() - 1) % MENU_ITEMS.len(),
                },
                KeyCode::Down => AppState::Menu {
                    selected: (selected + 1) % MENU_ITEMS.len(),
                },
                KeyCode::Enter => self.launch(MENU_ITEMS[selected], selected),
                _ => AppState::Menu { selected },
            },
            AppState::Playing {
                mut game,
                mut next_tick,
            } => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                    self.menu()
                } else {
                    let input = match key.code {
                        KeyCode::Up | KeyCode::Char('k') if game.kind() == GameKind::Ledris => {
                            Some(GameInput::RotateCW)
                        }
                        KeyCode::Up | KeyCode::Char('k') => Some(GameInput::Up),
                        KeyCode::Down | KeyCode::Char('j') => Some(GameInput::Down),
                        KeyCode::Left | KeyCode::Char('h') => Some(GameInput::Left),
                        KeyCode::Right | KeyCode::Char('l') => Some(GameInput::Right),
                        KeyCode::Char(' ') => Some(GameInput::HardDrop),
                        _ => None,
                    };
                    let status = input
                        .map(|i| game.handle_input(i))
                        .unwrap_or(GameStatus::Running);
                    if input == Some(GameInput::HardDrop) && game.kind() == GameKind::Ledris {
                        next_tick = now + Duration::from_millis(game.tick_interval_ms());
                    }
                    if status != GameStatus::Running {
                        AppState::GameOver {
                            game,
                            won: status == GameStatus::Won,
                            started: now,
                        }
                    } else {
                        AppState::Playing { game, next_tick }
                    }
                }
            }
            AppState::GameOver { game, won, started } => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.menu(),
                KeyCode::Char('r') => self.new_game(game.kind()),
                _ => AppState::GameOver { game, won, started },
            },
            AppState::Feature {
                mut feature,
                next_tick,
            } => {
                if key.code == KeyCode::Esc
                    || (key.code == KeyCode::Char('q') && feature.kind() != FeatureKind::Wpm)
                {
                    self.menu()
                } else {
                    feature.handle_key(key.code, now);
                    AppState::Feature { feature, next_tick }
                }
            }
            AppState::EmbeddedGame { game } => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                    self.menu()
                } else {
                    let control = match (game, key.code) {
                        (GameId::Pong, KeyCode::Left) => Some(GameControlArg::Left),
                        (GameId::Pong, KeyCode::Right) => Some(GameControlArg::Right),
                        (GameId::Pong, KeyCode::Char('a')) => Some(GameControlArg::SecondLeft),
                        (GameId::Pong, KeyCode::Char('d')) => Some(GameControlArg::SecondRight),
                        _ => None,
                    };
                    if let Some(control) = control {
                        let result = self.device.send_game_control(control);
                        self.record(result);
                    }
                    if self.device.is_connected() {
                        AppState::EmbeddedGame { game }
                    } else {
                        self.menu()
                    }
                }
            }
            AppState::EmbeddedGameOfLife { selected_pattern } => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.menu(),
                KeyCode::Up => AppState::EmbeddedGameOfLife {
                    selected_pattern: (selected_pattern + PATTERNS.len() - 1) % PATTERNS.len(),
                },
                KeyCode::Down => AppState::EmbeddedGameOfLife {
                    selected_pattern: (selected_pattern + 1) % PATTERNS.len(),
                },
                KeyCode::Enter => {
                    let result = self.device.start_game_of_life(PATTERNS[selected_pattern].0);
                    if self.record(result) {
                        AppState::EmbeddedGame {
                            game: GameId::GameOfLife,
                        }
                    } else {
                        self.menu()
                    }
                }
                _ => AppState::EmbeddedGameOfLife { selected_pattern },
            },
        };
    }

    /// Called after every input or timeout; key traffic cannot starve the simulation.
    pub fn update(&mut self, now: Instant) {
        self.now = now;
        if (self.composer.is_some() || matches!(self.state, AppState::Menu { .. }))
            && !self.device.is_connected()
            && now >= self.next_reconnect
        {
            self.discover_device();
        }
        if let Some(composer) = &mut self.composer {
            composer.runtime.update(now, composer.playing());
        }
        let mut finished = None;
        match &mut self.state {
            AppState::Playing { game, next_tick } => {
                for _ in 0..MAX_CATCH_UP {
                    if now < *next_tick {
                        break;
                    }
                    let status = game.tick();
                    *next_tick += Duration::from_millis(game.tick_interval_ms());
                    if status != GameStatus::Running {
                        finished = Some(status);
                        break;
                    }
                }
                if now >= *next_tick {
                    *next_tick = now + Duration::from_millis(game.tick_interval_ms());
                }
            }
            AppState::Feature { feature, next_tick } => {
                for _ in 0..MAX_CATCH_UP {
                    if now < *next_tick {
                        break;
                    }
                    feature.tick(now);
                    *next_tick += Duration::from_millis(feature.tick_interval_ms());
                }
                if now >= *next_tick {
                    *next_tick = now + Duration::from_millis(feature.tick_interval_ms());
                }
            }
            _ => {}
        }
        if let Some(status) = finished {
            let AppState::Playing { game, .. } =
                std::mem::replace(&mut self.state, AppState::Menu { selected: 0 })
            else {
                unreachable!()
            };
            self.state = AppState::GameOver {
                game,
                won: status == GameStatus::Won,
                started: now,
            };
        }
        // Firmware sleeps after 60s without commands, even when the image is unchanged.
        if self.device.is_connected()
            && (self.composer.is_some() || !matches!(self.state, AppState::Menu { .. }))
            && now.saturating_duration_since(self.last_keepalive) >= KEEPALIVE_INTERVAL
        {
            let result = self.device.send_command(Command::Sleeping, &[0]);
            self.record(result);
            self.last_keepalive = now;
        }
        self.send_frame_to_device();
    }

    pub fn poll_timeout(&self, now: Instant) -> Duration {
        let mut wait = Duration::from_millis(100);
        let deadline = match &self.state {
            AppState::Playing { next_tick, .. } | AppState::Feature { next_tick, .. } => {
                Some(*next_tick)
            }
            AppState::GameOver { started, .. } => {
                let phase = now.saturating_duration_since(*started).as_millis() / 300;
                (phase < 4).then(|| *started + Duration::from_millis((phase as u64 + 1) * 300))
            }
            AppState::Menu { .. } if !self.no_device && !self.device.is_connected() => {
                Some(self.next_reconnect)
            }
            _ => None,
        };
        if let Some(deadline) = deadline {
            wait = wait.min(deadline.saturating_duration_since(now));
        }
        if let Some(composer) = &self.composer {
            wait = wait.min(composer.runtime.poll_timeout(now));
        }
        wait
    }

    pub fn display_grid(&self) -> Option<Grid> {
        if let Some(composer) = &self.composer {
            return Some(composer.runtime.frame());
        }
        match &self.state {
            AppState::Playing { game, .. } => Some(*game.grid()),
            AppState::Feature { feature, .. } => feature.grid().copied(),
            AppState::GameOver { game, started, .. } => {
                let phase = self.now.saturating_duration_since(*started).as_millis() / 300;
                if phase < 4 {
                    Some([[phase % 2 == 1; HEIGHT]; WIDTH])
                } else {
                    Some(*game.grid())
                }
            }
            _ => None,
        }
    }

    fn send_frame_to_device(&mut self) {
        if !self.device.is_connected() {
            return;
        }
        let result = if let AppState::Feature { feature, .. } = &self.state {
            if let Some(brightness) = feature.brightness() {
                self.device.set_brightness(brightness)
            } else if let Some(grid) = feature.grid() {
                self.device.send_grid(grid)
            } else {
                Ok(())
            }
        } else if let Some(grid) = self.display_grid() {
            self.device.send_grid(&grid)
        } else {
            Ok(())
        };
        self.record(result);
    }

    pub fn device_status(&self) -> String {
        if let Some(error) = &self.error {
            return format!("Device: {error}");
        }
        if self.no_device {
            return "No device (--no-device)".into();
        }
        if let Some(port) = &self.device.port_name {
            format!(
                "{} v{}",
                port,
                self.device.version.as_deref().unwrap_or("?")
            )
        } else {
            "No device | retrying in menu".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn continuous_input_does_not_starve_ticks() {
        let now = Instant::now();
        let mut app = App::new(true, None, now);
        app.handle_key(key(KeyCode::Enter), now);
        let initial = app.display_grid().unwrap();
        for ms in 1..=201 {
            let time = now + Duration::from_millis(ms);
            app.handle_key(key(KeyCode::Char('x')), time);
            app.update(time);
        }
        assert_ne!(initial, app.display_grid().unwrap());
        if let AppState::Playing { next_tick, .. } = app.state {
            assert_eq!(next_tick, now + Duration::from_millis(400));
        } else {
            panic!("unexpected game over");
        }
    }
    #[test]
    fn breathing_deadline_can_be_ten_milliseconds() {
        let now = Instant::now();
        let mut app = App::new(true, None, now);
        app.state = app.launch(MenuItem::Feature(FeatureKind::Breathing), 0);
        for step in 1..=40 {
            app.update(now + Duration::from_millis(step * 25));
        }
        assert_eq!(
            app.poll_timeout(now + Duration::from_secs(1)),
            Duration::from_millis(10)
        );
    }
    #[test]
    fn embedded_games_require_device_and_ctrl_c_exits_wpm() {
        let now = Instant::now();
        let mut app = App::new(true, None, now);
        app.state = app.launch(MenuItem::Pong, 2);
        assert!(matches!(app.state, AppState::Menu { .. }));
        assert!(app.error.is_some());
        app.state = app.launch(MenuItem::Feature(FeatureKind::Wpm), 7);
        app.handle_key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            now,
        );
        assert!(!app.running);
    }
    #[test]
    fn game_over_flashes_twice_then_restores_final_grid() {
        let now = Instant::now();
        let mut app = App::new(true, None, now);
        let game: Box<dyn Game> = Box::new(SnakeGame::new());
        let final_grid = *game.grid();
        app.state = AppState::GameOver {
            game,
            won: false,
            started: now,
        };
        for phase in 0..4 {
            app.update(now + Duration::from_millis(phase * 300));
            assert_eq!(
                app.display_grid().unwrap(),
                [[phase % 2 == 1; HEIGHT]; WIDTH]
            );
        }
        app.update(now + Duration::from_millis(1200));
        assert_eq!(app.display_grid().unwrap(), final_grid);
    }
}
