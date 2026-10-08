use super::LifePattern;
use crate::games::{empty_grid, Grid, HEIGHT, WIDTH};
use crossterm::event::KeyCode;
use rand::Rng;

pub struct Life {
    grid: Grid,
    generation: usize,
}

impl Life {
    pub const HEIGHT: usize = 12;
    pub fn new(pattern: LifePattern) -> Self {
        let mut grid = empty_grid();
        let points: &[(usize, usize)] = match pattern {
            LifePattern::Glider => &[(2, 2), (3, 3), (1, 4), (2, 4), (3, 4)],
            LifePattern::Blinker => &[(3, 5), (4, 5), (5, 5)],
            LifePattern::Toad => &[(3, 4), (4, 4), (5, 4), (2, 5), (3, 5), (4, 5)],
            LifePattern::Beacon => &[
                (2, 3),
                (3, 3),
                (2, 4),
                (3, 4),
                (4, 5),
                (5, 5),
                (4, 6),
                (5, 6),
            ],
            LifePattern::BeaconToadBlinker => &[
                (0, 0),
                (1, 0),
                (0, 1),
                (1, 1),
                (2, 2),
                (3, 2),
                (2, 3),
                (3, 3),
                (5, 5),
                (6, 5),
                (7, 5),
                (4, 6),
                (5, 6),
                (6, 6),
                (1, 10),
                (2, 10),
                (3, 10),
            ],
            LifePattern::Random => &[],
            LifePattern::EasterEgg => &[
                (2, 3),
                (3, 3),
                (4, 3),
                (5, 3),
                (6, 3),
                (2, 4),
                (2, 5),
                (3, 5),
                (4, 5),
                (2, 6),
                (2, 7),
            ],
        };
        for &(x, y) in points {
            grid[x][y] = true;
        }
        if pattern == LifePattern::Random {
            let mut rng = rand::thread_rng();
            for column in &mut grid {
                for cell in &mut column[..Self::HEIGHT] {
                    *cell = rng.gen_bool(0.3);
                }
            }
        }
        Self {
            grid,
            generation: 0,
        }
    }

    pub fn tick(&mut self) {
        let mut next = empty_grid();
        for (x, column) in next.iter_mut().enumerate() {
            for (y, cell) in column[..Self::HEIGHT].iter_mut().enumerate() {
                let mut neighbors = 0;
                for dx in -1isize..=1 {
                    for dy in -1isize..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        if let (Some(nx), Some(ny)) =
                            (x.checked_add_signed(dx), y.checked_add_signed(dy))
                        {
                            if nx < WIDTH && ny < Self::HEIGHT && self.grid[nx][ny] {
                                neighbors += 1;
                            }
                        }
                    }
                }
                *cell = neighbors == 3 || (self.grid[x][y] && neighbors == 2);
            }
        }
        self.grid = next;
        self.generation += 1;
    }
    pub fn grid(&self) -> Grid {
        self.grid
    }
    pub fn status(&self) -> String {
        format!(
            "Game of Life | Generation {} | n: next pattern",
            self.generation
        )
    }
}

pub struct Pong {
    ball: (i32, i32),
    velocity: (i32, i32),
    paddles: [i32; 2],
    scores: [usize; 2],
}

impl Pong {
    pub fn new() -> Self {
        Self {
            ball: (4, 17),
            velocity: (1, 1),
            paddles: [3, 3],
            scores: [0, 0],
        }
    }

    pub fn tick(&mut self, interactive: bool) {
        if !interactive {
            for paddle in &mut self.paddles {
                *paddle = (self.ball.0 - 1).clamp(0, WIDTH as i32 - 3);
            }
        }
        let (mut x, mut y) = (self.ball.0 + self.velocity.0, self.ball.1 + self.velocity.1);
        if x < 0 || x >= WIDTH as i32 {
            self.velocity.0 = -self.velocity.0;
            x = (self.ball.0 + self.velocity.0).clamp(0, WIDTH as i32 - 1);
        }
        let paddle = if y <= 0 {
            Some(0)
        } else if y >= HEIGHT as i32 - 1 {
            Some(1)
        } else {
            None
        };
        if let Some(index) = paddle {
            if (self.paddles[index]..self.paddles[index] + 3).contains(&x) {
                self.velocity.1 = -self.velocity.1;
                y = self.ball.1 + self.velocity.1;
            } else {
                self.scores[1 - index] += 1;
                self.ball = (4, 17);
                self.velocity.1 = if index == 0 { -1 } else { 1 };
                return;
            }
        }
        self.ball = (x, y);
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Left | KeyCode::Char('h') => self.paddles[0] -= 1,
            KeyCode::Right | KeyCode::Char('l') => self.paddles[0] += 1,
            KeyCode::Char('a') => self.paddles[1] -= 1,
            KeyCode::Char('d') => self.paddles[1] += 1,
            _ => {}
        }
        for paddle in &mut self.paddles {
            *paddle = (*paddle).clamp(0, WIDTH as i32 - 3);
        }
    }
    pub fn grid(&self) -> Grid {
        let mut grid = empty_grid();
        for (index, &paddle) in self.paddles.iter().enumerate() {
            for x in paddle..paddle + 3 {
                grid[x as usize][if index == 0 { 0 } else { HEIGHT - 1 }] = true;
            }
        }
        grid[self.ball.0 as usize][self.ball.1 as usize] = true;
        grid
    }
    pub fn status(&self) -> String {
        format!(
            "Pong | {}:{} | arrows: top | a/d: bottom",
            self.scores[0], self.scores[1]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn life_blinker_oscillates_and_stays_within_its_widget() {
        let mut life = Life::new(LifePattern::Blinker);
        let first = life.grid();
        life.tick();
        assert_ne!(life.grid(), first);
        assert!(life.grid()[4][4] && life.grid()[4][5] && life.grid()[4][6]);
        life.tick();
        assert_eq!(life.grid(), first);
        assert!(life
            .grid()
            .iter()
            .all(|column| column[Life::HEIGHT..].iter().all(|&cell| !cell)));
    }
    #[test]
    fn pong_auto_play_remains_in_bounds_and_manual_paddles_are_clamped() {
        let mut pong = Pong::new();
        for _ in 0..500 {
            pong.tick(false);
            let _ = pong.grid();
        }
        for _ in 0..20 {
            pong.handle_key(KeyCode::Left);
            pong.handle_key(KeyCode::Char('d'));
        }
        assert_eq!(pong.paddles, [0, 6]);
        pong.ball = (1, 1);
        pong.velocity = (1, -1);
        pong.paddles[0] = 5;
        pong.tick(true);
        assert_eq!(pong.scores[1], 1);
        assert_eq!(pong.ball, (4, 17));
    }
}
