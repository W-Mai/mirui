//! Camera-space steps along the columns and rows of an image layer.

use crate::types::Fixed;

/// A linear mapping from integer slice coordinates to three fixed-point camera axes.
/// The caller chooses the camera axes and supplies a separate origin for each layer.
/// Coordinates and their intermediate products must fit in the fixed-point range.
#[derive(Clone, Copy, Debug)]
pub struct GridBasis {
    column: [Fixed; 3],
    row: [Fixed; 3],
}

impl GridBasis {
    pub const fn new(column: [Fixed; 3], row: [Fixed; 3]) -> Self {
        Self { column, row }
    }

    /// Projects one axis independently, allowing depth rejection before computing screen position.
    #[inline]
    pub fn axis(&self, origin: Fixed, axis: usize, x: i32, y: i32) -> Fixed {
        origin + self.column[axis] * x + self.row[axis] * y
    }

    #[inline]
    pub fn at(&self, origin: [Fixed; 3], x: i32, y: i32) -> [Fixed; 3] {
        core::array::from_fn(|axis| self.axis(origin[axis], axis, x, y))
    }

    /// Extrema along one axis over a rectangle in the layer's coordinate system.
    #[inline]
    pub fn axis_range(&self, origin: Fixed, axis: usize, x: [i32; 2], y: [i32; 2]) -> [Fixed; 2] {
        let [a, b] = x.map(|v| self.column[axis] * v);
        let [c, d] = y.map(|v| self.row[axis] * v);
        [origin + a.min(b) + c.min(d), origin + a.max(b) + c.max(d)]
    }
}
