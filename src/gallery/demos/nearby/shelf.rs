use crate::prelude::*;

pub(super) const INSET: i32 = 12;
pub(super) const GAP: i32 = 12;
pub(super) const CARD_INSET: i32 = 2;
pub(super) const LABEL_HEIGHT: i32 = 18;
pub(super) const LABEL_ACTOR_SPACE: i32 = 24;
pub(super) const SHADOW: i32 = 2;
const MIN_CARD_WIDTH: i32 = 164;
const MAX_CARD_WIDTH: i32 = 280;
const IMAGE_CHROME_WIDTH: i32 = 28;
const IMAGE_CHROME_HEIGHT: i32 = 40;
const PAGE_MARK_HEIGHT: i32 = 4;
const PAGE_MARK_GAP: i32 = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grid {
    pub columns: usize,
    pub rows: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grid_fits_available_bounds_and_retains_landscape_artwork() {
        for width in [424, 560, 720] {
            for height in (190..=440).step_by(8) {
                for total in 1..=17 {
                    let plan = Plan::fit(width, height, total);
                    assert!((1..=2).contains(&plan.grid.rows));
                    assert!(plan.grid.columns * plan.grid.rows <= 8);
                    assert!(plan.width() <= width - INSET * 2);
                    assert!(
                        plan.card_height * plan.grid.rows as i32
                            + GAP * (plan.grid.rows as i32 - 1)
                            <= height - INSET * 2 - PAGE_MARK_HEIGHT - PAGE_MARK_GAP
                    );
                    if plan.card_width >= MIN_CARD_WIDTH {
                        assert!(
                            ((plan.card_width - IMAGE_CHROME_WIDTH) * 3
                                - (plan.card_height - IMAGE_CHROME_HEIGHT) * 4)
                                .abs()
                                < 4
                        );
                    }
                }
            }
        }
        assert_eq!(
            Plan::fit(560, 350, 6).grid,
            Grid {
                columns: 3,
                rows: 2
            }
        );
        assert_eq!(Plan::fit(424, 205, 6).grid.rows, 1);
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Plan {
    pub grid: Grid,
    pub card_width: i32,
    pub card_height: i32,
}

impl Plan {
    pub(super) fn fit(width: i32, height: i32, total: usize) -> Self {
        let available_width = (width - INSET * 2).max(1);
        let available_height = (height - INSET * 2 - PAGE_MARK_HEIGHT - PAGE_MARK_GAP).max(1);
        let mut best = Self {
            grid: Grid {
                columns: 1,
                rows: 1,
            },
            card_width: available_width.min(MAX_CARD_WIDTH),
            card_height: available_height,
        };
        let mut score = 0;
        for rows in 1..=2 {
            for columns in 1..=4 {
                if rows > 1 && total <= columns {
                    continue;
                }
                let row_height = (available_height - GAP * (rows as i32 - 1)) / rows as i32;
                let height_limit = (row_height - IMAGE_CHROME_HEIGHT) * 4 / 3 + IMAGE_CHROME_WIDTH;
                let card_width = ((available_width - GAP * (columns as i32 - 1)) / columns as i32)
                    .min(MAX_CARD_WIDTH)
                    .min(height_limit);
                if card_width < MIN_CARD_WIDTH {
                    continue;
                }
                let card_height = (card_width - IMAGE_CHROME_WIDTH) * 3 / 4 + IMAGE_CHROME_HEIGHT;
                let candidate =
                    (rows * columns).min(total.max(1)) as i32 * 1000 + card_width - rows as i32;
                if candidate > score {
                    score = candidate;
                    best = Self {
                        grid: Grid { columns, rows },
                        card_width,
                        card_height,
                    };
                }
            }
        }
        best
    }

    pub fn width(self) -> i32 {
        self.card_width * self.grid.columns as i32
            + GAP * self.grid.columns.saturating_sub(1) as i32
    }
}

#[model]
pub(super) struct Shelf {
    #[observe]
    pub plan: Plan,
    #[observe]
    pub page: usize,
    #[observe]
    pub count: usize,
    size: (i32, i32),
    total: usize,
    selected: usize,
}

impl Default for Shelf {
    fn default() -> Self {
        Self {
            plan: Plan::fit(560, 350, 6),
            page: 0,
            count: 1,
            size: (560, 350),
            total: 6,
            selected: 0,
        }
    }
}

impl Shelf {
    fn update(&mut self) {
        self.plan = Plan::fit(self.size.0, self.size.1, self.total);
        let capacity = self.plan.grid.rows * self.plan.grid.columns;
        self.page = self.selected / capacity;
        self.count = self.total.div_ceil(capacity);
    }
}

#[model]
impl Shelf {
    pub fn set_size(&mut self, width: Fixed, height: Fixed) {
        let size = (width.to_int(), height.to_int());
        if size != self.size {
            self.size = size;
            self.update();
        }
    }
    pub fn present(&mut self, total: usize, selected: usize) {
        let selected = selected.min(total.saturating_sub(1));
        if (total, selected) != (self.total, self.selected) {
            self.total = total;
            self.selected = selected;
            self.update();
        }
    }
    #[observe]
    pub fn pages(&self) -> (usize, usize, usize) {
        let first = self
            .page
            .saturating_sub(3)
            .min(self.count.saturating_sub(7));
        if self.count > 1 {
            (first, self.count.min(first + 7), self.page)
        } else {
            (0, 0, 0)
        }
    }
}
