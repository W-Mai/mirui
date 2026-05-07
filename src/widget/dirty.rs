use crate::types::Rect;
use alloc::vec::Vec;

/// Dirty flag component — marks an entity as needing redraw
pub struct Dirty;

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
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        for r in &self.rects {
            if r.x < min_x {
                min_x = r.x;
            }
            if r.y < min_y {
                min_y = r.y;
            }
            let rx = r.x + r.w as i32;
            let ry = r.y + r.h as i32;
            if rx > max_x {
                max_x = rx;
            }
            if ry > max_y {
                max_y = ry;
            }
        }
        Some(Rect {
            x: min_x,
            y: min_y,
            w: (max_x - min_x) as u16,
            h: (max_y - min_y) as u16,
        })
    }
}
