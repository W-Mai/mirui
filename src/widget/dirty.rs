use crate::types::{Fixed, Rect};
use alloc::vec::Vec;

/// Dirty flag component — marks an entity as needing redraw
pub struct Dirty;

/// Stores the previous rect before a position change
pub struct PrevRect(pub Rect);

/// Tracks dirty regions for partial refresh
#[derive(Default)]
pub struct DirtyRegions {
    pub rects: Vec<Rect>,
}

impl DirtyRegions {
    pub fn new() -> Self {
        Self { rects: Vec::new() }
    }

    pub fn mark(&mut self, rect: Rect) {
        self.rects.push(rect);
    }

    pub fn clear(&mut self) {
        self.rects.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// Merge all dirty rects into one bounding rect
    pub fn bounding_rect(&self) -> Option<Rect> {
        if self.rects.is_empty() {
            return None;
        }
        let mut min_x = Fixed::from_int(i32::MAX >> 8);
        let mut min_y = Fixed::from_int(i32::MAX >> 8);
        let mut max_x = Fixed::from_int(i32::MIN >> 8);
        let mut max_y = Fixed::from_int(i32::MIN >> 8);
        for r in &self.rects {
            let rx = r.x;
            let ry = r.y;
            let rx2 = (r.x + r.w).ceil();
            let ry2 = (r.y + r.h).ceil();
            if rx < min_x {
                min_x = rx;
            }
            if ry < min_y {
                min_y = ry;
            }
            if rx2 > max_x {
                max_x = rx2;
            }
            if ry2 > max_y {
                max_y = ry2;
            }
        }
        Some(Rect {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        })
    }
}
