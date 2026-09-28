use alloc::vec::Vec;

pub(super) const MAX_GRID_EDGE: i32 = 160;

pub(super) const GOSPER_GUN: &[(i32, i32)] = &[
    (0, 24),
    (1, 22),
    (1, 24),
    (2, 12),
    (2, 13),
    (2, 20),
    (2, 21),
    (2, 34),
    (2, 35),
    (3, 11),
    (3, 15),
    (3, 20),
    (3, 21),
    (3, 34),
    (3, 35),
    (4, 0),
    (4, 1),
    (4, 10),
    (4, 16),
    (4, 20),
    (4, 21),
    (5, 0),
    (5, 1),
    (5, 10),
    (5, 14),
    (5, 16),
    (5, 17),
    (5, 22),
    (5, 24),
    (6, 10),
    (6, 16),
    (6, 24),
    (7, 11),
    (7, 15),
    (8, 12),
    (8, 13),
];

pub(super) const ACORN: &[(i32, i32)] = &[(0, 1), (1, 3), (2, 0), (2, 1), (2, 4), (2, 5), (2, 6)];

pub(super) const GLIDER: &[(i32, i32)] = &[(0, 1), (1, 2), (2, 0), (2, 1), (2, 2)];

pub struct LifeBoard {
    pub cols: i32,
    pub rows: i32,
    pub cell: Vec<bool>,
    pub(super) scratch: Vec<bool>,
    rng: u32,
    next_drop: i32,
}

impl LifeBoard {
    pub(super) fn new(cols: i32, rows: i32) -> Self {
        let n = (cols * rows).max(0) as usize;
        let mut b = Self {
            cols,
            rows,
            cell: alloc::vec![false; n],
            scratch: alloc::vec![false; n],
            rng: 0x9e37_79b9,
            next_drop: 0,
        };
        b.next_drop = b.roll_interval();
        b
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn roll_interval(&mut self) -> i32 {
        25 + (self.rand() % 100) as i32
    }

    pub(super) fn set(&mut self, r: i32, c: i32) {
        let r = r.rem_euclid(self.rows);
        let c = c.rem_euclid(self.cols);
        self.cell[(r * self.cols + c) as usize] = true;
    }

    pub(super) fn seed(&mut self, origin: (i32, i32), pattern: &[(i32, i32)]) {
        for &(dr, dc) in pattern {
            self.set(origin.0 + dr, origin.1 + dc);
        }
    }

    // keep overlapping top-left cells so a running pattern survives a resize
    pub(super) fn resize(&mut self, cols: i32, rows: i32) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut next = alloc::vec![false; (cols * rows).max(0) as usize];
        for r in 0..rows.min(self.rows) {
            for c in 0..cols.min(self.cols) {
                if self.cell[(r * self.cols + c) as usize] {
                    next[(r * cols + c) as usize] = true;
                }
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.cell = next;
        self.scratch.clear();
    }

    pub(super) fn step(&mut self) {
        let (rows, cols) = (self.rows, self.cols);
        self.scratch.resize((rows * cols) as usize, false);
        for r in 0..rows {
            // single-step edge wrap, no modulo
            let up = if r == 0 { rows - 1 } else { r - 1 } * cols;
            let mid = r * cols;
            let down = if r == rows - 1 { 0 } else { r + 1 } * cols;
            for c in 0..cols {
                let left = if c == 0 { cols - 1 } else { c - 1 };
                let right = if c == cols - 1 { 0 } else { c + 1 };
                let cell = &self.cell;
                let live = cell[(up + left) as usize] as i32
                    + cell[(up + c) as usize] as i32
                    + cell[(up + right) as usize] as i32
                    + cell[(mid + left) as usize] as i32
                    + cell[(mid + right) as usize] as i32
                    + cell[(down + left) as usize] as i32
                    + cell[(down + c) as usize] as i32
                    + cell[(down + right) as usize] as i32;
                let idx = (mid + c) as usize;
                self.scratch[idx] = matches!((cell[idx], live), (true, 2) | (_, 3));
            }
        }
        core::mem::swap(&mut self.cell, &mut self.scratch);
    }

    // periodic glider injection keeps the soup from thinning out
    pub(super) fn advance(&mut self) {
        self.next_drop -= 1;
        if self.next_drop <= 0 {
            self.seed((1, 1), GLIDER);
            self.next_drop = self.roll_interval();
        }
        self.step();
    }

    #[cfg(test)]
    pub(super) fn alive_count(&self) -> usize {
        self.cell.iter().filter(|&&a| a).count()
    }
}

pub(in crate::gallery::demos) fn seeded_board(cols: i32, rows: i32) -> LifeBoard {
    let mut board = LifeBoard::new(cols, rows);
    board.seed((3, 2), GOSPER_GUN);
    board.seed((rows / 4, cols / 2), GOSPER_GUN);
    board.seed((rows * 2 / 3, cols / 2), ACORN);
    board.seed((rows / 2, cols / 5), ACORN);
    board.seed((rows / 3, cols * 4 / 5), GLIDER);
    board.seed((rows * 4 / 5, cols / 4), GLIDER);
    board
}
