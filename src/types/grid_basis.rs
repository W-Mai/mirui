use super::Fixed;

/// Maps an integer grid to three fixed-point coordinates using column and row steps.
/// Each axis uses the caller's origin. Coordinates and intermediate products must fit in the fixed-point range.
#[derive(Clone, Copy, Debug)]
pub struct GridBasis {
    column: [Fixed; 3],
    row: [Fixed; 3],
}

impl GridBasis {
    pub const fn new(column: [Fixed; 3], row: [Fixed; 3]) -> Self {
        Self { column, row }
    }

    /// Projects a single axis.
    #[inline]
    pub fn axis(&self, origin: Fixed, axis: usize, x: i32, y: i32) -> Fixed {
        origin + self.column[axis] * x + self.row[axis] * y
    }

    #[inline]
    pub fn at(&self, origin: [Fixed; 3], x: i32, y: i32) -> [Fixed; 3] {
        core::array::from_fn(|axis| self.axis(origin[axis], axis, x, y))
    }

    /// Returns the extrema along one axis over a grid rectangle.
    #[inline]
    pub fn axis_range(&self, origin: Fixed, axis: usize, x: [i32; 2], y: [i32; 2]) -> [Fixed; 2] {
        let [a, b] = x.map(|v| self.column[axis] * v);
        let [c, d] = y.map(|v| self.row[axis] * v);
        [origin + a.min(b) + c.min(d), origin + a.max(b) + c.max(d)]
    }
}
