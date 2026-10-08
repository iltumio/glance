use rand::prelude::*;

use super::{empty_grid, Game, GameInput, GameKind, GameStatus, Grid, HEIGHT, WIDTH};

const COLS: usize = WIDTH;
const ROWS: usize = HEIGHT;

/// Tetromino shapes as 2D arrays (row-major).
const SHAPES: &[&[&[bool]]] = &[
    // I
    &[&[true, true, true, true]],
    // O
    &[&[true, true], &[true, true]],
    // T
    &[&[false, true, false], &[true, true, true]],
    // S
    &[&[true, true, false], &[false, true, true]],
    // Z
    &[&[false, true, true], &[true, true, false]],
    // L
    &[&[true, true, true], &[true, false, false]],
    // J
    &[&[true, true, true], &[false, false, true]],
];

pub struct LedrisGame {
    board: [[bool; COLS]; ROWS],
    current_piece: Vec<Vec<bool>>,
    current_pos: (i32, i32), // (x, y)
    score: usize,
    grid: Grid,
    rng: StdRng,
    status: GameStatus,
}

impl LedrisGame {
    pub fn new() -> Self {
        Self::with_rng(StdRng::from_entropy())
    }

    fn with_rng(mut rng: StdRng) -> Self {
        let piece = random_piece(&mut rng);
        let pos = spawn_position(&piece);
        let mut game = Self {
            board: [[false; COLS]; ROWS],
            current_piece: piece,
            current_pos: pos,
            score: 0,
            grid: empty_grid(),
            rng,
            status: GameStatus::Running,
        };
        game.rebuild_grid();
        game
    }

    fn lock_piece(&mut self) -> GameStatus {
        self.merge_piece();
        self.score += self.clear_rows();
        let piece = random_piece(&mut self.rng);
        let pos = spawn_position(&piece);
        if self.check_collision(&piece, pos) {
            self.status = GameStatus::Over;
        } else {
            self.current_piece = piece;
            self.current_pos = pos;
        }
        self.rebuild_grid();
        self.status
    }

    fn check_collision(&self, piece: &[Vec<bool>], offset: (i32, i32)) -> bool {
        let (off_x, off_y) = offset;
        for (y, row) in piece.iter().enumerate() {
            for (x, &cell) in row.iter().enumerate() {
                if cell {
                    let px = x as i32 + off_x;
                    let py = y as i32 + off_y;
                    if px < 0 || px >= COLS as i32 || py >= ROWS as i32 {
                        return true;
                    }
                    if py >= 0 && self.board[py as usize][px as usize] {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn merge_piece(&mut self) {
        let (off_x, off_y) = self.current_pos;
        for (y, row) in self.current_piece.iter().enumerate() {
            for (x, &cell) in row.iter().enumerate() {
                if cell {
                    let py = y as i32 + off_y;
                    let px = x as i32 + off_x;
                    if py >= 0 && py < ROWS as i32 && px >= 0 && px < COLS as i32 {
                        self.board[py as usize][px as usize] = true;
                    }
                }
            }
        }
    }

    fn clear_rows(&mut self) -> usize {
        let mut cleared = 0;
        let mut new_board = [[false; COLS]; ROWS];
        let mut write_row = ROWS - 1;

        for read_row in (0..ROWS).rev() {
            if self.board[read_row].iter().all(|&c| c) {
                cleared += 1;
            } else {
                new_board[write_row] = self.board[read_row];
                write_row = write_row.saturating_sub(1);
            }
        }

        self.board = new_board;
        cleared
    }

    fn rotate_cw(piece: &[Vec<bool>]) -> Vec<Vec<bool>> {
        let rows = piece.len();
        let cols = piece[0].len();
        let mut rotated = vec![vec![false; rows]; cols];
        for y in 0..rows {
            for x in 0..cols {
                rotated[x][rows - 1 - y] = piece[y][x];
            }
        }
        rotated
    }

    fn rebuild_grid(&mut self) {
        self.grid = empty_grid();

        // Draw settled board
        for y in 0..ROWS {
            for x in 0..COLS {
                if self.board[y][x] {
                    self.grid[x][y] = true;
                }
            }
        }

        // Draw current piece
        let (off_x, off_y) = self.current_pos;
        for (y, row) in self.current_piece.iter().enumerate() {
            for (x, &cell) in row.iter().enumerate() {
                if cell {
                    let py = y as i32 + off_y;
                    let px = x as i32 + off_x;
                    if py >= 0 && py < ROWS as i32 && px >= 0 && px < COLS as i32 {
                        self.grid[px as usize][py as usize] = true;
                    }
                }
            }
        }
        draw_score(&mut self.grid, self.score);
    }
}

// The top five rows are reserved for the on-device score, matching Python.
fn draw_score(grid: &mut Grid, score: usize) {
    const DIGITS: [[u8; 5]; 10] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 1, 1],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
    ];
    let text = score.min(99).to_string();
    let start = WIDTH - (text.len() * 4 - 1);
    for (index, digit) in text.bytes().enumerate() {
        for (y, row) in DIGITS[(digit - b'0') as usize].iter().enumerate() {
            for x in 0..3 {
                grid[start + index * 4 + x][y] = row & (1 << (2 - x)) != 0;
            }
        }
    }
}

fn random_piece(rng: &mut StdRng) -> Vec<Vec<bool>> {
    let idx = rng.gen_range(0..SHAPES.len());
    SHAPES[idx].iter().map(|row| row.to_vec()).collect()
}

fn spawn_position(piece: &[Vec<bool>]) -> (i32, i32) {
    let piece_width = piece[0].len() as i32;
    (COLS as i32 / 2 - piece_width / 2, 5) // Start below score area
}

impl Game for LedrisGame {
    fn kind(&self) -> GameKind {
        GameKind::Ledris
    }

    fn tick(&mut self) -> GameStatus {
        if self.status != GameStatus::Running {
            return self.status;
        }
        let new_pos = (self.current_pos.0, self.current_pos.1 + 1);

        if !self.check_collision(&self.current_piece, new_pos) {
            self.current_pos = new_pos;
        } else {
            return self.lock_piece();
        }

        self.rebuild_grid();
        GameStatus::Running
    }

    fn handle_input(&mut self, input: GameInput) -> GameStatus {
        if self.status != GameStatus::Running {
            return self.status;
        }
        match input {
            GameInput::Left => {
                let new_pos = (self.current_pos.0 - 1, self.current_pos.1);
                if !self.check_collision(&self.current_piece, new_pos) {
                    self.current_pos = new_pos;
                    self.rebuild_grid();
                }
            }
            GameInput::Right => {
                let new_pos = (self.current_pos.0 + 1, self.current_pos.1);
                if !self.check_collision(&self.current_piece, new_pos) {
                    self.current_pos = new_pos;
                    self.rebuild_grid();
                }
            }
            GameInput::Down => {
                let new_pos = (self.current_pos.0, self.current_pos.1 + 1);
                if !self.check_collision(&self.current_piece, new_pos) {
                    self.current_pos = new_pos;
                    self.rebuild_grid();
                }
            }
            GameInput::RotateCW => {
                let rotated = Self::rotate_cw(&self.current_piece);
                if !self.check_collision(&rotated, self.current_pos) {
                    self.current_piece = rotated;
                    self.rebuild_grid();
                }
            }
            GameInput::HardDrop => {
                while !self.check_collision(
                    &self.current_piece,
                    (self.current_pos.0, self.current_pos.1 + 1),
                ) {
                    self.current_pos.1 += 1;
                }
                return self.lock_piece();
            }
            _ => {}
        }
        self.status
    }

    fn grid(&self) -> &Grid {
        &self.grid
    }

    fn score(&self) -> usize {
        self.score
    }

    fn tick_interval_ms(&self) -> u64 {
        // Speed increases with score: max(100, 500 - score * 10)
        500u64
            .saturating_sub((self.score as u64).saturating_mul(10))
            .max(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn game() -> LedrisGame {
        LedrisGame::with_rng(StdRng::seed_from_u64(42))
    }
    #[test]
    fn hard_drop_locks_and_spawns_immediately() {
        let mut g = game();
        g.current_piece = vec![vec![true, true], vec![true, true]];
        g.current_pos = (3, 5);
        assert_eq!(g.handle_input(GameInput::HardDrop), GameStatus::Running);
        assert!(g.board[ROWS - 1][3]);
        assert!(g.board[ROWS - 2][4]);
        assert_eq!(g.current_pos.1, 5);
    }
    #[test]
    fn simultaneous_lines_shift_surviving_rows() {
        let mut g = game();
        g.board[ROWS - 1] = [true; COLS];
        g.board[ROWS - 2] = [true; COLS];
        g.board[ROWS - 3][2] = true;
        assert_eq!(g.clear_rows(), 2);
        assert!(g.board[ROWS - 1][2]);
        assert_eq!(g.board.iter().flatten().filter(|&&p| p).count(), 1);
    }
    #[test]
    fn four_rotations_preserve_every_shape_and_score_is_on_device() {
        let mut g = game();
        for shape in SHAPES {
            let original: Vec<Vec<_>> = shape.iter().map(|r| r.to_vec()).collect();
            let mut rotated = original.clone();
            for _ in 0..4 {
                rotated = LedrisGame::rotate_cw(&rotated);
            }
            assert_eq!(rotated, original);
        }
        g.score = 12;
        g.rebuild_grid();
        assert!(g.grid.iter().any(|col| col[..5].iter().any(|&p| p)));
        g.score = usize::MAX;
        assert_eq!(g.tick_interval_ms(), 100);
    }
    #[test]
    fn hard_drop_propagates_spawn_collision_as_game_over() {
        let mut g = game();
        g.current_piece = vec![vec![true]];
        g.current_pos = (0, ROWS as i32 - 1);
        for row in &mut g.board[5..9] {
            row[1..8].fill(true);
        }
        assert_eq!(g.handle_input(GameInput::HardDrop), GameStatus::Over);
        assert_eq!(g.tick(), GameStatus::Over);
    }
}
