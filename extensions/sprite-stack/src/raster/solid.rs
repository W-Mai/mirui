//! Closed voxel surfaces derived from the full-resolution cells of a stack.

use super::{Projection, SceneCanvas, quads::draw_quad, sprites::StackTransform};
use crate::PreparedSpriteStack;
use alloc::vec::Vec;
use core::{array, mem::size_of};
use mirui::{render::RenderError, types::Fixed};

type Point = [Fixed; 3];

/// A prepared stack with occupancy for drawing opaque voxel surfaces.
///
/// Cell centers follow the source stack's pivot and spacing. Each cell extends
/// half a cell in each direction. Empty cells and intentional gaps remain open.
pub struct SolidStack<'a> {
    source: &'a PreparedSpriteStack<'a>,
    rows: Vec<u64>,
    row_words: usize,
}

impl<'a> SolidStack<'a> {
    /// Prepare occupancy within a byte budget. Drawing makes no allocations.
    ///
    /// Each row occupies eight bytes per 64 source columns, rounded up.
    pub fn new(
        source: &'a PreparedSpriteStack<'a>,
        budget_bytes: usize,
    ) -> Result<Self, RenderError> {
        let [width, height, layers] = source.extent().map(usize::from);
        let row_words = width.div_ceil(64);
        let words = row_words * height * layers;
        let bytes = words * size_of::<u64>();
        if bytes > budget_bytes {
            return Err(RenderError::InsufficientWorkspace {
                required_bytes: bytes,
                capacity_bytes: budget_bytes,
            });
        }
        let mut rows = Vec::new();
        rows.try_reserve_exact(words)
            .map_err(|_| RenderError::MissingWorkspace)?;
        rows.resize(words, 0);
        let base = source.levels()[0];
        for z in 0..layers {
            for cell in base.layer(z).expect("validated layer index") {
                let x = usize::from(cell.x);
                let row = z * height + usize::from(cell.y);
                rows[row * row_words + x / 64] |= 1 << (x % 64);
            }
        }
        Ok(Self {
            source,
            rows,
            row_words,
        })
    }

    pub fn palette(&self) -> &[[u8; 4]] {
        self.source.palette()
    }

    pub const fn spacing_q8(&self) -> i32 {
        self.source.spacing_q8()
    }

    fn occupied(&self, [x, y, z]: [usize; 3]) -> bool {
        let row = z * usize::from(self.source.extent()[1]) + y;
        self.rows[row * self.row_words + x / 64] & (1 << (x % 64)) != 0
    }

    /// Draw exposed voxel faces with the scene's shared depth buffer.
    ///
    /// Orthographic and perspective views use the same cell boundaries. The
    /// color callback supplies RGB; the model's occupied cells remain opaque.
    pub fn draw(
        &self,
        tile: &mut SceneCanvas<'_, '_>,
        projection: &Projection,
        transform: &StackTransform,
        mut shade: impl FnMut([u8; 4], usize) -> u32,
    ) {
        let extent = self.source.extent().map(usize::from);
        let [right, forward, up] = transform.axes.map(|axis| axis.map(|v| v * transform.scale));
        let steps = [
            right,
            forward.map(|v| -v),
            up.map(|v| v * transform.layer_spacing),
        ];
        let offset = self
            .source
            .pivot_q8()
            .map(|p| -Fixed::from_ratio(p, 256) - Fixed::HALF);
        let origin: Point = array::from_fn(|i| {
            transform.origin[i]
                + steps[0][i] * offset[0]
                + steps[1][i] * offset[1]
                + steps[2][i] * offset[2]
        });
        let palette = self.source.palette();
        let level = self.source.levels()[0];
        for z in 0..extent[2] {
            for sample in level.layer(z).expect("validated layer index") {
                let cell = [usize::from(sample.x), usize::from(sample.y), z];
                let index = usize::from(sample.color);
                let color = shade(palette[index], index) | 0xff00_0000;
                let base = array::from_fn(|i| {
                    origin[i]
                        + steps[0][i] * i32::from(sample.x)
                        + steps[1][i] * i32::from(sample.y)
                        + steps[2][i] * z as i32
                });
                for (axis, step) in steps.into_iter().enumerate() {
                    let u = steps[(axis + 1) % 3];
                    let v = steps[(axis + 2) % 3];
                    for side in [false, true] {
                        let next = if side {
                            Some(cell[axis] + 1).filter(|&n| n < extent[axis])
                        } else {
                            cell[axis].checked_sub(1)
                        };
                        if let Some(next) = next {
                            let mut neighbor = cell;
                            neighbor[axis] = next;
                            if self.occupied(neighbor) {
                                continue;
                            }
                        }
                        let a = if side { add(base, step) } else { base };
                        let b = add(a, u);
                        draw_quad(tile, projection, [a, b, add(b, v), add(a, v)], color);
                    }
                }
            }
        }
    }
}

fn add(a: Point, b: Point) -> Point {
    array::from_fn(|i| a[i] + b[i])
}

#[cfg(test)]
#[path = "../../tests/unit/raster/solid.rs"]
mod tests;
