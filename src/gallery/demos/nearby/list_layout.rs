use crate::prelude::*;

const WINDOW_BUDGET: usize = 8;

#[model]
pub(super) struct ListLayout {
    #[observe]
    pub rows: usize,
}

impl Default for ListLayout {
    fn default() -> Self {
        Self { rows: 4 }
    }
}

#[model]
impl ListLayout {
    pub fn resize(&mut self, height: Fixed, minimum: i32, gap: i32) {
        if height <= Fixed::ZERO {
            return;
        }
        self.rows =
            ((height.to_int() + gap) / (minimum + gap)).clamp(1, WINDOW_BUDGET as i32) as usize;
    }
}

pub(super) fn first(selected: usize, total: usize, rows: usize) -> usize {
    selected
        .saturating_sub(rows - 1)
        .min(total.saturating_sub(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_windows_keep_selection_visible_for_every_capacity_and_partial_page() {
        let mut layout = ListLayout::default();
        for height in 1..=1200 {
            layout.resize(height.into(), 52, 3);
            let rows = layout.rows;
            assert!((1..=8).contains(&rows));
            for total in 1..=25 {
                for selected in 0..total {
                    let start = first(selected, total, rows);
                    assert!(start <= selected && selected < (start + rows).min(total));
                    assert!(start <= total.saturating_sub(rows));
                }
            }
        }
        layout.resize(Fixed::ZERO, 52, 3);
        assert_eq!(layout.rows, 8);
    }
}
