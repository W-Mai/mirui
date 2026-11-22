//! Front-to-back column rendering for height fields and procedural surfaces.

use super::{Projection, SceneCanvas};
use crate::HeightField;
use mirui::types::{Fixed, Fixed64};

/// Samples in a row share their depth, world-space pixel width and material preparation.
#[derive(Clone, Copy)]
pub struct TerrainRow {
    pub depth: Fixed,
    pub pixel_width: Fixed,
    /// Inclusive bounds in the surface's sampling coordinates.
    pub bounds: [Fixed; 4],
}

/// Supplies heights and materials for column rendering. A sample can retain a plane address so the renderer only reads its material after visibility is known.
pub trait TerrainSurface {
    type Sample;
    fn height_at(&self, position: [Fixed; 2]) -> Option<(Fixed, Self::Sample)>;
    /// Returns packed RGBA top and wall colors.
    fn colors(&self, position: [Fixed; 2], height: Fixed, sample: Self::Sample) -> [u32; 2];
    /// Applies row lighting or fog to a ground face that will be drawn.
    fn shade(&self, color: u32) -> u32 {
        color
    }
    fn begin_row(&mut self, _row: TerrainRow) {}
    fn overlay(&self, _position: [Fixed; 2], _column: &mut TerrainColumn<'_, '_, '_>) {}
}

impl TerrainSurface for &HeightField {
    type Sample = usize;
    fn height_at(&self, [x, z]: [Fixed; 2]) -> Option<(Fixed, usize)> {
        let address = self.address_at(x, z)?;
        Some((self.grid().elevation(self.levels()[address]), address))
    }
    fn colors(&self, _: [Fixed; 2], _: Fixed, address: usize) -> [u32; 2] {
        let color = self.color(self.materials()[address]);
        [u32::from_le_bytes([color.r, color.g, color.b, color.a]); 2]
    }
}

#[derive(Clone, Copy)]
struct HeightProjection {
    origin: Fixed,
    scale: Fixed,
}

impl HeightProjection {
    fn new(center: Fixed, scale: Fixed, slope: Fixed, depth: Fixed, altitude: Fixed) -> Self {
        Self {
            origin: center - scale * (slope * depth - altitude),
            scale,
        }
    }
    fn at(self, height: Fixed) -> i32 {
        (self.origin - self.scale * height).to_int()
    }
    fn minimum_y(self, [low, high]: [Fixed; 2]) -> i32 {
        self.at(if self.scale >= Fixed::ZERO { high } else { low })
    }
}

fn ray_step(scale: Fixed, rows: Fixed, denominator: Fixed) -> Fixed {
    let width_step = Fixed::from_ratio(25, 10) * denominator / scale;
    let step = if rows.is_zero() {
        width_step
    } else {
        (denominator * denominator / rows).min(width_step)
    };
    step.max(Fixed::HALF)
}

/// One sampled column of a suspended surface. Its layers share the ground depth buffer.
pub struct TerrainColumn<'a, 'b, 'c> {
    canvas: &'a mut SceneCanvas<'b, 'c>,
    x: usize,
    depth: Fixed,
    current: HeightProjection,
    previous: HeightProjection,
}

impl TerrainColumn<'_, '_, '_> {
    /// Draws the top, underside and side wall colors of a slab. Wall colors are ordered from bottom to top. A slab with no walls has a dithered, single-slice surface.
    pub fn slab(
        &mut self,
        base: Fixed,
        step: Fixed,
        top: u32,
        underside: u32,
        walls: impl DoubleEndedIterator<Item = u32> + ExactSizeIterator,
    ) {
        let count = walls.len();
        let Ok(count) = i32::try_from(count) else {
            return;
        };
        let top_height = base + step * count;
        self.canvas.write_span::<true>(
            self.x,
            self.current.at(top_height),
            self.previous.at(top_height),
            top,
            self.depth,
            count == 0,
        );
        self.canvas.write_span::<true>(
            self.x,
            self.previous.at(base),
            self.current.at(base),
            underside,
            self.depth,
            count == 0,
        );
        for (i, color) in walls.enumerate().rev() {
            let height = base + step * i as i32;
            self.canvas.write_span::<true>(
                self.x,
                self.previous.at(height + step),
                self.previous.at(height),
                color,
                self.depth,
                false,
            );
        }
    }
}

/// A camera pass over a terrain plane. The unit `yaw` pair is (sin, cos) in the sampling plane. At yaw zero, positive screen X increases the first coordinate and increasing depth decreases the second.
#[derive(Clone, Copy)]
pub struct TerrainView {
    pub projection: Projection,
    pub origin: [Fixed; 2],
    pub altitude: Fixed,
    pub yaw: (Fixed, Fixed),
    /// Inclusive minimum and maximum elevations of the ground and any suspended layers.
    pub heights: [Fixed; 2],
    /// Camera-space near and far depth limits.
    pub depth: [Fixed; 2],
}

impl TerrainView {
    /// Composites another terrain over existing depths. Columns restart at the bottom of the tile. Equal depths retain the first surface.
    pub fn draw_layer(&self, canvas: &mut SceneCanvas<'_, '_>, surface: &mut impl TerrainSurface) {
        canvas.columns.fill(canvas.color.clip[3]);
        self.draw(canvas, surface);
    }
    /// Draws the terrain into a tile initialized with `SceneCanvas::clear_depth`. Draw depth-tested objects afterwards. The ray sequence is anchored to the full view so subdividing a frame preserves its samples.
    pub fn draw(&self, canvas: &mut SceneCanvas<'_, '_>, surface: &mut impl TerrainSurface) {
        let [left, clip_top, right, bottom] = canvas.color.clip;
        let p = self.projection;
        let [low, high] = self.heights;
        if left >= right || clip_top >= bottom || p.scale <= Fixed::ZERO || high < low {
            return;
        }
        let [near, far] = self.depth;
        let [cx, cy] = p.center;
        let below = Fixed::from_int(canvas.color.bounds[3]) - cy;
        let perspective = p.perspective > Fixed64::ZERO;
        let near = if perspective {
            near.max(-p.divide_perspective(Fixed::from_ratio(85, 100)))
        } else {
            near
        };
        let altitude = self.altitude;
        let above_ground =
            !perspective || altitude + p.divide_perspective(p.slope) > high + Fixed::ONE;
        let dn = p.scale * p.slope + p.apply_perspective(below);
        let mut depth = if above_ground && dn > Fixed::ZERO {
            near.max((p.scale * (altitude - high) - below) / dn - Fixed::from_int(2))
        } else {
            near
        };
        let above = cy - Fixed::from_int(canvas.color.bounds[1]) + Fixed::from_int(2);
        let slope = p.scale * p.slope;
        let end = if slope > p.apply_perspective(above) {
            far.min((p.scale * (altitude - low) + above) / (slope - p.apply_perspective(above)))
        } else {
            far
        };
        if depth >= end {
            return;
        }
        let mut denominator = Fixed::ONE + p.apply_perspective(depth);
        let rows = p.scale * (p.slope + p.apply_perspective(altitude));
        if denominator <= Fixed::ZERO {
            return;
        }
        let mut row = HeightProjection::new(cy, p.scale / denominator, p.slope, depth, altitude);
        // An orthographic ray has the same denominator and spacing at every depth.
        let orthographic = p
            .perspective
            .is_zero()
            .then(|| (ray_step(p.scale, rows, denominator), denominator / p.scale));
        let mut minimum_y = row.minimum_y(self.heights);
        let mut open_columns = right - left;
        let (sin, cos) = self.yaw;
        while depth < end && open_columns > 0 {
            let step =
                orthographic.map_or_else(|| ray_step(p.scale, rows, denominator), |(step, _)| step);
            if depth > Fixed::MAX - step {
                break;
            }
            depth += step;
            let previous = row;
            let scale = if orthographic.is_some() {
                previous.scale
            } else {
                denominator = Fixed::ONE + p.apply_perspective(depth);
                if denominator <= Fixed::ZERO {
                    break;
                }
                p.scale / denominator
            };
            row = HeightProjection::new(cy, scale, p.slope, depth, altitude);
            let previous_minimum = minimum_y;
            minimum_y = row.minimum_y(self.heights);
            if minimum_y >= bottom && previous_minimum >= bottom {
                continue;
            }
            let width = orthographic.map_or_else(|| denominator / p.scale, |(_, width)| width);
            let dx = cos * width;
            let dz = -sin * width;
            let mut x = self.origin[0] - cx * dx - depth * sin + dx * left;
            let mut z = self.origin[1] - cx * dz - depth * cos + dz * left;
            let last = right - left - 1;
            let x1 = x + dx * last;
            let z1 = z + dz * last;
            surface.begin_row(TerrainRow {
                depth,
                pixel_width: width,
                bounds: [x.min(x1), z.min(z1), x.max(x1), z.max(z1)],
            });
            let mut last_height = low;
            let mut ground_rows = [row.at(low), previous.at(low)];
            for px in left..right {
                let column_index = (px - canvas.color.origin[0]) as usize;
                let mut edge = canvas.columns[column_index];
                let position = [x, z];
                x += dx;
                z += dz;
                if edge <= clip_top {
                    continue;
                }
                let x = px as usize;
                surface.overlay(
                    position,
                    &mut TerrainColumn {
                        canvas,
                        x,
                        depth,
                        current: row,
                        previous,
                    },
                );
                let Some((height, sample)) = surface.height_at(position) else {
                    continue;
                };
                if last_height != height {
                    ground_rows = [row.at(height), previous.at(height)];
                    last_height = height;
                }
                let [top_y, wall_y] = ground_rows;
                if top_y >= edge && wall_y >= edge {
                    continue;
                }
                let [top, wall] = surface.colors(position, height, sample);
                if wall_y < edge {
                    canvas.write_span::<true>(x, wall_y, edge, surface.shade(wall), depth, false);
                    edge = wall_y;
                }
                if top_y < edge {
                    canvas.write_span::<true>(x, top_y, edge, surface.shade(top), depth, false);
                    edge = top_y;
                }
                canvas.columns[column_index] = edge;
                if edge <= clip_top {
                    open_columns -= 1;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/raster/terrain.rs"]
mod tests;
