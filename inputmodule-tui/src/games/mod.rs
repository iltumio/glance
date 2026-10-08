pub mod game_of_life;
pub mod ledris;
pub mod snake;

pub use inputmodule_host::{empty_grid, Grid, HEIGHT, WIDTH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameKind {
    Snake,
    Ledris,
}
impl GameKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Snake => "Snake",
            Self::Ledris => "Ledris",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameInput {
    Up,
    Down,
    Left,
    Right,
    RotateCW,
    HardDrop,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStatus {
    Running,
    Over,
    Won,
}

pub trait Game {
    fn kind(&self) -> GameKind;
    fn name(&self) -> &str {
        self.kind().name()
    }
    fn tick(&mut self) -> GameStatus;
    fn handle_input(&mut self, input: GameInput) -> GameStatus;
    fn grid(&self) -> &Grid;
    fn score(&self) -> usize;
    fn tick_interval_ms(&self) -> u64;
}
