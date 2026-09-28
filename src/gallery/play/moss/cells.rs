pub(crate) const GRID_WIDTH: u8 = 20;
pub(crate) const GRID_HEIGHT: u8 = 12;
const CELL_COUNT: usize = GRID_WIDTH as usize * GRID_HEIGHT as usize;
const RANDOM_THRESHOLD: u32 = 1_245_540_515;
pub(super) const GLIDER: [(i8, i8); 5] = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MossCells {
    ages: [u8; CELL_COUNT],
}

impl MossCells {
    pub(super) const EMPTY: Self = Self {
        ages: [0; CELL_COUNT],
    };

    const fn index(x: u8, y: u8) -> Option<usize> {
        if x >= GRID_WIDTH || y >= GRID_HEIGHT {
            return None;
        }
        Some(y as usize * GRID_WIDTH as usize + x as usize)
    }

    pub(crate) fn get(&self, x: u8, y: u8) -> u8 {
        Self::index(x, y).map_or(0, |index| self.ages[index])
    }

    pub(super) fn set(&mut self, x: u8, y: u8, age: u8) -> bool {
        let Some(index) = Self::index(x, y) else {
            return false;
        };
        let changed = self.ages[index] != age;
        self.ages[index] = age;
        changed
    }

    pub(crate) fn live_count(&self) -> u16 {
        self.ages.iter().filter(|age| **age != 0).count() as u16
    }

    pub(crate) fn seed(kind: u8) -> Option<Self> {
        if kind > 2 {
            return None;
        }
        let mut cells = Self::EMPTY;
        let mut set = |x: i8, y: i8| {
            if (0..GRID_WIDTH as i8).contains(&x) && (0..GRID_HEIGHT as i8).contains(&y) {
                cells.set(x as u8, y as u8, 1);
            }
        };
        match kind {
            0 => {
                for (x, y) in GLIDER {
                    set(x + 2, y + 2);
                    set(17 - x, 9 - y);
                }
                for x in 8..=10 {
                    set(x, 5);
                }
                set(9, 4);
                set(9, 6);
            }
            1 => {
                for (center_x, center_y) in [(5, 4), (14, 7)] {
                    set(center_x - 1, center_y);
                    set(center_x, center_y);
                    set(center_x + 1, center_y);
                }
                for (x, y) in [(9, 8), (10, 8), (9, 9), (10, 9)] {
                    set(x, y);
                }
            }
            _ => {
                let mut state = 2026_u32;
                for y in 2..10 {
                    for x in 2..18 {
                        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        if state < RANDOM_THRESHOLD {
                            set(x, y);
                        }
                    }
                }
            }
        }
        Some(cells)
    }

    pub(super) fn evolve(&self) -> Self {
        let mut next = Self::EMPTY;
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                let mut neighbours = 0_u8;
                for dy in -1_i8..=1 {
                    for dx in -1_i8..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let sample_x = x as i8 + dx;
                        let sample_y = y as i8 + dy;
                        if sample_x >= 0
                            && sample_x < GRID_WIDTH as i8
                            && sample_y >= 0
                            && sample_y < GRID_HEIGHT as i8
                            && self.get(sample_x as u8, sample_y as u8) != 0
                        {
                            neighbours += 1;
                        }
                    }
                }
                let age = self.get(x, y);
                if neighbours == 3 || (age != 0 && neighbours == 2) {
                    next.set(x, y, if age == 0 { 1 } else { age.saturating_add(1) });
                }
            }
        }
        next
    }
}
