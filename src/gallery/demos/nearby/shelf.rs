use crate::anim::{Tween, TweenConfig, ease};
use crate::prelude::*;

pub(super) const INSET: i32 = 12;
pub(super) const GAP: i32 = 12;
pub(super) const CARD_INSET: i32 = 2;
pub(super) const LABEL_HEIGHT: i32 = 30;
pub(super) const LABEL_ACTOR_SPACE: i32 = 24;
pub(super) const SHADOW: i32 = 2;
const MIN_CARD_WIDTH: i32 = 160;
const MAX_CARD_WIDTH: i32 = 280;
const IMAGE_CHROME_WIDTH: i32 = 28;
const IMAGE_CHROME_HEIGHT: i32 = 52;
const PAGE_MARK_HEIGHT: i32 = 4;
const PAGE_MARK_GAP: i32 = 4;

pub(super) const WINDOW_BUDGET: usize = 128;
const PAGE_MARKERS: usize = 7;
const SHORT_MARK: Fixed = Fixed::from_int(6);
const LONG_MARK: Fixed = Fixed::from_int(22);
const PAGE_MOTION: TweenConfig = TweenConfig::new(260, &ease::OUT_CUBIC);

#[derive(Clone, Copy)]
struct MarkerMotion {
    page: usize,
    tween: Tween,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub columns: usize,
    pub rows: usize,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            columns: 3,
            rows: 1,
        }
    }
}

impl Grid {
    pub fn valid(self) -> bool {
        self.columns > 0
            && self.rows > 0
            && self
                .columns
                .checked_mul(self.rows)
                .is_some_and(|size| size <= WINDOW_BUDGET)
    }

    pub fn capacity(self) -> usize {
        self.columns * self.rows
    }

    pub fn first(self, selected: usize) -> usize {
        selected / self.capacity() * self.capacity()
    }

    pub fn navigate(
        self,
        selected: usize,
        count: usize,
        horizontal: isize,
        vertical: isize,
    ) -> usize {
        if count == 0 {
            return 0;
        }
        let selected = selected.min(count - 1);
        let first = self.first(selected);
        let local = selected - first;
        let row = local / self.columns;
        let column = local % self.columns;
        let next = match (horizontal.signum(), vertical.signum()) {
            (0, -1) if row > 0 => Some(selected - self.columns),
            (0, -1) if first > 0 => Some(first - self.columns + column),
            (0, 1) if row + 1 < self.rows && selected + self.columns < count => {
                Some(selected + self.columns)
            }
            (0, 1) if first + self.capacity() < count => Some(first + self.capacity() + column),
            (0, 1) if row + 1 < self.rows => Some(count - 1),
            (-1, 0) if column > 0 => Some(selected - 1),
            (1, 0) if column + 1 < self.columns => (selected + 1 < count).then_some(selected + 1),
            (-1, 0) if first > 0 => {
                Some(first - self.capacity() + row * self.columns + self.columns - 1)
            }
            (1, 0) if first + self.capacity() < count => {
                Some(first + self.capacity() + row * self.columns)
            }
            _ => None,
        };
        next.unwrap_or(selected).min(count - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_markers_retarget_the_existing_tweens_without_restarting_same_page_motion() {
        let mut shelf = Shelf::default();
        shelf.present(48, 0);
        shelf.advance(260);
        assert_eq!(shelf.marker_lengths[0], LONG_MARK);
        assert!(shelf.markers_settled());
        let capacity = shelf.plan.grid.capacity();
        shelf.present(48, capacity);
        shelf.advance(80);
        let middle = shelf.marker_lengths;
        assert!(middle[0] > SHORT_MARK && middle[0] < LONG_MARK);
        assert!(middle[1] > SHORT_MARK && middle[1] < LONG_MARK);
        let elapsed = shelf.markers[1].tween.elapsed_ms;
        shelf.present(48, capacity + 1);
        assert_eq!(shelf.markers[1].tween.elapsed_ms, elapsed);
        shelf.present(48, 0);
        assert_eq!(shelf.marker_lengths, middle);
        shelf.advance(260);
        assert_eq!(shelf.marker_lengths[0], LONG_MARK);
        assert_eq!(shelf.marker_lengths[1], SHORT_MARK);
        assert!(shelf.markers_settled());
    }

    #[test]
    fn bounded_grids_reach_every_entry_across_partial_pages() {
        use alloc::collections::{BTreeSet, VecDeque};
        for columns in 1..=4 {
            for rows in 1..=7 {
                let grid = Grid { columns, rows };
                assert!(grid.valid());
                for count in 1..=35 {
                    let mut seen = BTreeSet::from([0]);
                    let mut queue = VecDeque::from([0]);
                    while let Some(selected) = queue.pop_front() {
                        for (horizontal, vertical) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                            let next = grid.navigate(selected, count, horizontal, vertical);
                            assert!(next < count);
                            assert!(next - grid.first(next) < grid.capacity());
                            if seen.insert(next) {
                                queue.push_back(next);
                            }
                        }
                    }
                    assert_eq!(seen.len(), count);
                }
            }
        }
        assert!(
            !Grid {
                columns: 0,
                rows: 1
            }
            .valid()
        );
        assert!(
            !Grid {
                columns: usize::MAX,
                rows: 2
            }
            .valid()
        );
        assert!(
            !Grid {
                columns: 3,
                rows: WINDOW_BUDGET
            }
            .valid()
        );
        let grid = Grid {
            columns: 3,
            rows: 2,
        };
        assert_eq!(grid.navigate(2, 8, 1, 0), 6);
        assert_eq!(grid.navigate(5, 8, 1, 0), 7);
        assert_eq!(grid.navigate(6, 8, -1, 0), 2);
        assert_eq!(grid.navigate(6, 8, 0, -1), 3);
        assert_eq!(grid.navigate(6, 8, 0, 1), 7);
    }

    #[test]
    fn grid_fits_available_bounds_and_retains_landscape_artwork() {
        for width in [424, 560, 720] {
            for height in (190..=1040).step_by(17) {
                for total in 1..=17 {
                    let plan = Plan::fit(width, height, total);
                    assert!(plan.grid.valid());
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
        assert!(Plan::fit(600, 900, 24).grid.rows > 2);
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
        let minimum_height = (MIN_CARD_WIDTH - IMAGE_CHROME_WIDTH) * 3 / 4 + IMAGE_CHROME_HEIGHT;
        let row_limit = ((available_height + GAP) / (minimum_height + GAP)).max(1) as usize;
        let column_limit = ((available_width + GAP) / (MIN_CARD_WIDTH + GAP)).max(1) as usize;
        for rows in 1..=row_limit.min(WINDOW_BUDGET) {
            for columns in 1..=column_limit.min(WINDOW_BUDGET) {
                if rows * columns > WINDOW_BUDGET || rows > 1 && (rows - 1) * columns >= total {
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
        debug_assert!(best.grid.valid());
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
    #[observe]
    pub marker_lengths: [Fixed; PAGE_MARKERS],
    markers: [MarkerMotion; PAGE_MARKERS],
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
            marker_lengths: [SHORT_MARK; PAGE_MARKERS],
            markers: [MarkerMotion {
                page: usize::MAX,
                tween: Tween::preset(SHORT_MARK, SHORT_MARK, PAGE_MOTION),
            }; PAGE_MARKERS],
            size: (560, 350),
            total: 6,
            selected: 0,
        }
    }
}

impl Shelf {
    fn update(&mut self) {
        let previous_pages = self.pages();
        self.plan = Plan::fit(self.size.0, self.size.1, self.total);
        let capacity = self.plan.grid.rows * self.plan.grid.columns;
        self.page = self.selected / capacity;
        self.count = self.total.div_ceil(capacity);
        let pages = self.pages();
        if previous_pages != pages {
            let previous = self.markers;
            for (slot, motion) in self.markers.iter_mut().enumerate() {
                let page = pages.0 + slot;
                let target = if page == pages.2 {
                    LONG_MARK
                } else {
                    SHORT_MARK
                };
                *motion = previous
                    .iter()
                    .find(|old| old.page == page)
                    .copied()
                    .unwrap_or(MarkerMotion {
                        page,
                        tween: Tween::preset(SHORT_MARK, SHORT_MARK, PAGE_MOTION),
                    });
                motion.tween.retarget(target, PAGE_MOTION);
            }
            self.marker_lengths = self.markers.map(|marker| marker.tween.value());
        }
    }
}

#[model]
impl Shelf {
    pub fn advance(&mut self, delta: u16) {
        if self.markers_settled() {
            return;
        }
        for marker in &mut self.markers {
            marker.tween.tick(delta);
        }
        self.marker_lengths = self.markers.map(|marker| marker.tween.value());
    }

    pub fn markers_settled(&self) -> bool {
        self.markers.iter().all(|marker| marker.tween.is_finished())
    }

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

impl ShelfHandle {
    pub fn marker_width(&self, page: usize) -> Fixed {
        self.marker_lengths()
            .get(page.saturating_sub(self.pages().0))
            .copied()
            .unwrap_or(SHORT_MARK)
    }
}
