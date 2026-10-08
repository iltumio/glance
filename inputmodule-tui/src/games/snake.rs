use std::collections::VecDeque;

use rand::prelude::*;

use super::{empty_grid, Game, GameInput, GameKind, GameStatus, Grid, HEIGHT, WIDTH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

pub struct SnakeGame {
    head: (usize, usize),
    body: VecDeque<(usize, usize)>,
    food: (usize, usize),
    direction: Direction,
    next_direction: Direction,
    wrap: bool,
    grid: Grid,
    score: usize,
    rng: StdRng,
    status: GameStatus,
}

impl SnakeGame {
    pub fn new() -> Self {
        Self::with_rng(StdRng::from_entropy())
    }

    fn with_rng(mut rng: StdRng) -> Self {
        let head = (0, 0);
        let mut food = (0, 0);
        while food == head {
            food = (rng.gen_range(0..WIDTH), rng.gen_range(0..HEIGHT));
        }

        let mut game = Self {
            head,
            body: VecDeque::new(),
            food,
            direction: Direction::Down,
            next_direction: Direction::Down,
            wrap: false,
            grid: empty_grid(),
            score: 0,
            rng,
            status: GameStatus::Running,
        };
        game.rebuild_grid();
        game
    }

    fn spawn_food(&mut self) -> bool {
        let free: Vec<_> = (0..WIDTH)
            .flat_map(|x| (0..HEIGHT).map(move |y| (x, y)))
            .filter(|&p| p != self.head && !self.body.contains(&p))
            .collect();
        if let Some(&food) = free.choose(&mut self.rng) {
            self.food = food;
            true
        } else {
            false
        }
    }

    fn rebuild_grid(&mut self) {
        self.grid = empty_grid();
        let (hx, hy) = self.head;
        self.grid[hx][hy] = true;
        let (fx, fy) = self.food;
        if self.status != GameStatus::Won {
            self.grid[fx][fy] = true;
        }
        for &(bx, by) in &self.body {
            self.grid[bx][by] = true;
        }
    }
}

impl Game for SnakeGame {
    fn kind(&self) -> GameKind {
        GameKind::Snake
    }

    fn tick(&mut self) -> GameStatus {
        if self.status != GameStatus::Running {
            return self.status;
        }
        // Apply buffered direction
        self.direction = self.next_direction;

        let (x, y) = self.head;
        let old_head = self.head;

        // Compute new head position
        let new_head: (i32, i32) = match self.direction {
            Direction::Up => (x as i32, y as i32 - 1),
            Direction::Down => (x as i32, y as i32 + 1),
            Direction::Left => (x as i32 - 1, y as i32),
            Direction::Right => (x as i32 + 1, y as i32),
        };

        let (mut nx, mut ny) = new_head;

        // Wall check
        if nx < 0 || nx >= WIDTH as i32 || ny < 0 || ny >= HEIGHT as i32 {
            if self.wrap {
                if nx < 0 {
                    nx = WIDTH as i32 - 1;
                } else if nx >= WIDTH as i32 {
                    nx = 0;
                }
                if ny < 0 {
                    ny = HEIGHT as i32 - 1;
                } else if ny >= HEIGHT as i32 {
                    ny = 0;
                }
            } else {
                self.status = GameStatus::Over;
                return self.status;
            }
        }

        let new_head = (nx as usize, ny as usize);

        // Self-collision
        let growing = new_head == self.food;
        let tail_moves = !growing && self.body.back() == Some(&new_head);
        if self.body.contains(&new_head) && !tail_moves {
            self.status = GameStatus::Over;
            return self.status;
        }

        self.head = new_head;

        // Food check
        if new_head == self.food {
            self.body.push_front(old_head);
            self.score += 1;
            if !self.spawn_food() {
                self.status = GameStatus::Won;
            }
        } else if !self.body.is_empty() {
            self.body.pop_back();
            self.body.push_front(old_head);
        }

        self.rebuild_grid();
        self.status
    }

    fn handle_input(&mut self, input: GameInput) -> GameStatus {
        let new_dir = match input {
            GameInput::Up => Direction::Up,
            GameInput::Down => Direction::Down,
            GameInput::Left => Direction::Left,
            GameInput::Right => Direction::Right,
            _ => return self.status,
        };

        // Prevent reversal when snake has a body
        if !self.body.is_empty() && new_dir == self.direction.opposite() {
            return self.status;
        }

        self.next_direction = new_dir;
        self.status
    }

    fn grid(&self) -> &Grid {
        &self.grid
    }

    fn score(&self) -> usize {
        self.score
    }

    fn tick_interval_ms(&self) -> u64 {
        200 // Match Python MOVE_PERIOD
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn game() -> SnakeGame {
        SnakeGame::with_rng(StdRng::seed_from_u64(42))
    }
    #[test]
    fn filling_board_wins_and_keeps_complete_final_frame() {
        let mut g = game();
        g.head = (0, 0);
        g.food = (0, 1);
        g.body = (0..WIDTH)
            .flat_map(|x| (0..HEIGHT).map(move |y| (x, y)))
            .filter(|&p| p != g.head && p != g.food)
            .collect();
        assert_eq!(g.tick(), GameStatus::Won);
        assert!(g.grid.iter().flatten().all(|&p| p));
        assert_eq!(g.tick(), GameStatus::Won);
    }
    #[test]
    fn tail_can_move_out_of_the_way_but_reversal_is_buffered() {
        let mut g = game();
        g.head = (1, 1);
        g.body = VecDeque::from([(1, 0), (0, 0), (0, 1)]);
        g.direction = Direction::Down;
        g.next_direction = Direction::Down;
        g.food = (8, 33);
        g.handle_input(GameInput::Up);
        assert_eq!(g.next_direction, Direction::Down);
        g.handle_input(GameInput::Left);
        assert_eq!(g.tick(), GameStatus::Running);
        assert_eq!(g.head, (0, 1));
        assert_eq!(g.body.len(), 3);
    }
    #[test]
    fn wall_collision_and_growth_are_reproducible() {
        let mut g = game();
        g.food = (0, 1);
        assert_eq!(g.tick(), GameStatus::Running);
        assert_eq!(g.score(), 1);
        assert!(!g.body.contains(&g.food));
        assert_ne!(g.head, g.food);
        g.handle_input(GameInput::Left);
        assert_eq!(g.tick(), GameStatus::Over);
    }
}
