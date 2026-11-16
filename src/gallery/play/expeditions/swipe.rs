//! Swipes over an expedition board: a drag that ends far enough from where
//! it began presses the arrow it points along.

use super::{Direction4, logical_point};
use crate::prelude::*;

/// A drag shorter than this, in board pixels, is not a swipe.
const SWIPE_REACH: i32 = 18;

/// Where a drag over a board began, until it ends.
#[crate::component]
#[derive(Default)]
pub(crate) struct SwipeStart(Option<(Fixed, Fixed)>);

impl SwipeStart {
    /// Ends the drag at `end`: the arrow it points along, if it went far
    /// enough. `rect` is the surface's rect, drawn at the `surface` size.
    pub(crate) fn finish(
        &mut self,
        rect: Rect,
        end: (Fixed, Fixed),
        surface: (i32, i32),
    ) -> Option<Direction4> {
        let start = self.0.take()?;
        let (x0, y0) = logical_point(rect, start, surface)?;
        let (x1, y1) = logical_point(rect, end, surface)?;
        Direction4::toward(x1 - x0, y1 - y0, Fixed::from_int(SWIPE_REACH))
    }
}

#[event]
pub(crate) fn swipe_start(
    event: event!(DragStart),
    node: entity!(),
    rows: query!(mut SwipeStart),
) -> bool {
    let Some(start) = rows.get(node) else {
        return false;
    };
    start.0 = Some((event.x, event.y));
    true
}

#[event]
pub(crate) fn swipe_cancel(
    _event: event!(DragCancel),
    node: entity!(),
    rows: query!(mut SwipeStart),
) -> bool {
    if let Some(start) = rows.get(node) {
        start.0 = None;
    }
    true
}
