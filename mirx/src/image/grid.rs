use core::iter::FusedIterator;

use super::SurfaceDescriptor;

/// Unsigned sample coordinates with checked, exclusive right and bottom edges.
///
/// Coordinates are surface pixels until projected with `for_plane`, then plane
/// elements. Indexed and alpha sub-byte layouts remain sample coordinates;
/// this type does not round them to byte addresses or storage strides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Region {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Region {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self, RegionError> {
        x.checked_add(width)
            .ok_or(RegionError::CoordinateOverflow)?;
        y.checked_add(height)
            .ok_or(RegionError::CoordinateOverflow)?;
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub const fn x(self) -> u32 {
        self.x
    }
    pub const fn y(self) -> u32 {
        self.y
    }
    pub const fn width(self) -> u32 {
        self.width
    }
    pub const fn height(self) -> u32 {
        self.height
    }
    pub const fn right(self) -> u32 {
        self.x + self.width
    }
    pub const fn bottom(self) -> u32 {
        self.y + self.height
    }
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Tests positive-area intersection; touching edges and empty regions do not overlap.
    pub const fn intersects(self, other: Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.x() < other.right()
            && other.x() < self.right()
            && self.y() < other.bottom()
            && other.y() < self.bottom()
    }

    /// Returns the positive-area intersection in the same coordinate space.
    pub fn intersection(self, other: Self) -> Option<Self> {
        if !self.intersects(other) {
            return None;
        }
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Some(Self {
            x,
            y,
            width: self.right().min(other.right()) - x,
            height: self.bottom().min(other.bottom()) - y,
        })
    }

    /// Maps an exact surface region into one plane's element coordinates.
    ///
    /// Interior boundaries must align to subsampling. Only the outer surface
    /// edge may round upward for odd chroma dimensions; arbitrary regions are
    /// never silently expanded into neighboring samples.
    pub fn for_plane(self, surface: SurfaceDescriptor, index: u8) -> Result<Self, RegionError> {
        self.check_bounds(surface.width(), surface.height())?;
        let plane = surface
            .plane(index)
            .ok_or(RegionError::InvalidPlane(index))?;
        let x_multiple = 1u32 << plane.subsample_x_log2();
        let y_multiple = 1u32 << plane.subsample_y_log2();
        let horizontal = Self::project_axis(self.x, self.width, surface.width(), x_multiple);
        let vertical = Self::project_axis(self.y, self.height, surface.height(), y_multiple);
        let (Some((x, width)), Some((y, height))) = (horizontal, vertical) else {
            return Err(RegionError::UnalignedPlaneRegion {
                index,
                x_multiple,
                y_multiple,
            });
        };
        Self::new(x, y, width, height)
    }

    fn project_axis(start: u32, length: u32, extent: u32, multiple: u32) -> Option<(u32, u32)> {
        if length == 0 {
            return Some((start / multiple, 0));
        }
        let end = start + length;
        if start % multiple != 0 || (end != extent && end % multiple != 0) {
            return None;
        }
        let projected = start / multiple;
        Some((projected, end.div_ceil(multiple) - projected))
    }

    fn check_bounds(self, width: u32, height: u32) -> Result<(), RegionError> {
        if self.right() > width || self.bottom() > height {
            return Err(RegionError::OutOfBounds);
        }
        Ok(())
    }
}

impl SurfaceDescriptor {
    /// Checks exact logical surface bounds without clipping or rounding.
    pub fn region(self, x: u32, y: u32, width: u32, height: u32) -> Result<Region, RegionError> {
        let region = Region::new(x, y, width, height)?;
        region.check_bounds(self.width(), self.height())?;
        Ok(region)
    }

    /// Derives one shared grid whose tiles partition every sample plane.
    /// Interior tile edges cannot split a chroma element.
    pub fn tile_grid(self, tile_width: u32, tile_height: u32) -> Result<TileGrid, TileGridError> {
        let grid = TileGrid::new(self.width(), self.height(), tile_width, tile_height)?;
        if !grid.is_empty() {
            for (index, plane) in self.planes().enumerate() {
                let x_multiple = 1u32 << plane.subsample_x_log2();
                let y_multiple = 1u32 << plane.subsample_y_log2();
                if (grid.columns > 1 && tile_width % x_multiple != 0)
                    || (grid.rows > 1 && tile_height % y_multiple != 0)
                {
                    return Err(TileGridError::UnalignedPlaneBoundary {
                        index: index as u8,
                        x_multiple,
                        y_multiple,
                    });
                }
            }
        }
        Ok(grid)
    }
}

/// A row-major regular grid; rectangles are derived instead of stored per tile.
///
/// `new` operates in the supplied coordinate space. Use a surface's `tile_grid`
/// method for joint-plane grids with checked chroma boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileGrid {
    width: u32,
    height: u32,
    tile_width: u32,
    tile_height: u32,
    columns: u32,
    rows: u32,
    count: usize,
}

impl TileGrid {
    pub fn new(
        width: u32,
        height: u32,
        tile_width: u32,
        tile_height: u32,
    ) -> Result<Self, TileGridError> {
        if tile_width == 0 || tile_height == 0 {
            return Err(TileGridError::EmptyTile);
        }
        let columns = width.div_ceil(tile_width);
        let rows = height.div_ceil(tile_height);
        let count = columns
            .checked_mul(rows)
            .ok_or(TileGridError::TooManyTiles)?;
        let count = usize::try_from(count).map_err(|_| TileGridError::TooManyTiles)?;
        Ok(Self {
            width,
            height,
            tile_width,
            tile_height,
            columns,
            rows,
            count,
        })
    }

    pub const fn width(self) -> u32 {
        self.width
    }
    pub const fn height(self) -> u32 {
        self.height
    }
    pub const fn tile_width(self) -> u32 {
        self.tile_width
    }
    pub const fn tile_height(self) -> u32 {
        self.tile_height
    }
    pub const fn columns(self) -> u32 {
        self.columns
    }
    pub const fn rows(self) -> u32 {
        self.rows
    }
    pub const fn len(self) -> usize {
        self.count
    }
    pub const fn is_empty(self) -> bool {
        self.count == 0
    }

    /// Resolves an ordinal in constant time and clips only the outer edge tile.
    pub fn get(self, index: usize) -> Option<Region> {
        if index >= self.count {
            return None;
        }
        let index = index as u32;
        let x = index % self.columns * self.tile_width;
        let y = index / self.columns * self.tile_height;
        Some(Region {
            x,
            y,
            width: self.tile_width.min(self.width - x),
            height: self.tile_height.min(self.height - y),
        })
    }

    pub fn iter(self) -> Tiles {
        Tiles {
            grid: self,
            front: 0,
            back: self.count,
        }
    }
}

/// Exact-size tile iteration with constant-time forward and backward skips.
#[derive(Clone, Debug)]
pub struct Tiles {
    grid: TileGrid,
    front: usize,
    back: usize,
}

impl Iterator for Tiles {
    type Item = Region;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.grid.get(index)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.front += n;
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len(), Some(self.len()))
    }
    fn count(self) -> usize {
        self.len()
    }
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for Tiles {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.grid.get(self.back)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.back -= n;
        self.next_back()
    }
}

impl ExactSizeIterator for Tiles {
    fn len(&self) -> usize {
        self.back - self.front
    }
}
impl FusedIterator for Tiles {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RegionError {
    CoordinateOverflow,
    OutOfBounds,
    InvalidPlane(u8),
    UnalignedPlaneRegion {
        index: u8,
        x_multiple: u32,
        y_multiple: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TileGridError {
    EmptyTile,
    TooManyTiles,
    UnalignedPlaneBoundary {
        index: u8,
        x_multiple: u32,
        y_multiple: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{ColorDescription, SampleLayout};
    use alloc::vec;

    #[test]
    fn joint_yuv_tiles_partition_each_plane_exactly_once() {
        for layout in [
            SampleLayout::I420,
            SampleLayout::YV12,
            SampleLayout::NV12,
            SampleLayout::NV21,
            SampleLayout::P010,
            SampleLayout::P016,
        ] {
            for width in 0..10 {
                for height in 0..10 {
                    let surface = SurfaceDescriptor::new(
                        width,
                        height,
                        layout,
                        ColorDescription::BT709_YUV_LIMITED,
                    )
                    .unwrap();
                    let grid = surface.tile_grid(4, 2).unwrap();
                    for plane_index in 0..surface.plane_count() {
                        let plane = surface.plane(plane_index).unwrap();
                        let mut visits = vec![0; (plane.width() * plane.height()) as usize];
                        for region in grid.iter() {
                            let projected = region.for_plane(surface, plane_index).unwrap();
                            assert!(projected.right() <= plane.width());
                            assert!(projected.bottom() <= plane.height());
                            for y in projected.y()..projected.bottom() {
                                for x in projected.x()..projected.right() {
                                    visits[(y * plane.width() + x) as usize] += 1;
                                }
                            }
                        }
                        assert!(visits.iter().all(|&count| count == 1));
                    }
                }
            }
        }
    }

    #[test]
    fn odd_outer_edges_are_allowed_but_interior_chroma_splits_are_not() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        for size in [(1, 2), (3, 2), (2, 1)] {
            assert!(matches!(
                surface.tile_grid(size.0, size.1),
                Err(TileGridError::UnalignedPlaneBoundary { .. })
            ));
        }
        let grid = surface.tile_grid(4, 2).unwrap();
        assert_eq!(grid.len(), 4);
        let edge = grid.get(3).unwrap();
        assert_eq!(edge, Region::new(4, 2, 1, 1).unwrap());
        assert_eq!(edge.for_plane(surface, 1), Region::new(2, 1, 1, 1));
        assert_eq!(surface.tile_grid(5, 3).unwrap().len(), 1);
        assert_eq!(
            surface.tile_grid(99, 99).unwrap().get(0),
            Some(surface.region(0, 0, 5, 3).unwrap())
        );
        assert!(
            surface
                .region(1, 0, 2, 2)
                .unwrap()
                .for_plane(surface, 1)
                .is_err()
        );
        assert!(
            surface
                .region(0, 0, 3, 2)
                .unwrap()
                .for_plane(surface, 1)
                .is_err()
        );
        assert_eq!(
            edge.for_plane(surface, 2),
            Err(RegionError::InvalidPlane(2))
        );
        assert_eq!(surface.region(4, 2, 2, 1), Err(RegionError::OutOfBounds));
    }

    #[test]
    fn sub_byte_regions_remain_exact_sample_coordinates() {
        for layout in [
            SampleLayout::I1,
            SampleLayout::I2,
            SampleLayout::I4,
            SampleLayout::A1,
            SampleLayout::A2,
            SampleLayout::A4,
        ] {
            let color = if layout.is_alpha() {
                ColorDescription::NONE
            } else {
                ColorDescription::SRGB
            };
            let surface = SurfaceDescriptor::new(9, 3, layout, color).unwrap();
            let region = surface.region(1, 1, 3, 1).unwrap();
            assert_eq!(region.for_plane(surface, 0), Ok(region));
            assert_eq!(surface.tile_grid(3, 1).unwrap().len(), 9);
        }
    }

    #[test]
    fn coordinate_extremes_and_empty_grids_do_not_wrap_or_iterate() {
        assert_eq!(
            Region::new(u32::MAX, 0, 1, 1),
            Err(RegionError::CoordinateOverflow)
        );
        assert_eq!(
            Region::new(0, u32::MAX, 1, 1),
            Err(RegionError::CoordinateOverflow)
        );
        assert_eq!(TileGrid::new(1, 1, 0, 1), Err(TileGridError::EmptyTile));
        assert_eq!(
            TileGrid::new(u32::MAX, 2, 1, 1),
            Err(TileGridError::TooManyTiles)
        );
        let empty = TileGrid::new(0, u32::MAX, 1, 1).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.get(0), None);
        assert_eq!(empty.iter().next(), None);
        let huge = TileGrid::new(u32::MAX, 1, 1, 1).unwrap();
        assert_eq!(huge.iter().count(), u32::MAX as usize);
        assert_eq!(huge.iter().last(), huge.get(huge.len() - 1));
        assert_eq!(
            huge.iter().nth(u32::MAX as usize - 1),
            Region::new(u32::MAX - 1, 0, 1, 1).ok()
        );
        assert_eq!(huge.iter().next_back(), huge.get(huge.len() - 1));
        assert_eq!(huge.iter().nth_back(u32::MAX as usize - 1), huge.get(0));
        let mut exhausted = huge.iter();
        assert_eq!(exhausted.nth(usize::MAX), None);
        assert_eq!(exhausted.next_back(), None);
        assert_eq!(exhausted.len(), 0);
        assert_eq!(huge.get(usize::MAX), None);
        let full = TileGrid::new(u32::MAX, u32::MAX, u32::MAX, u32::MAX).unwrap();
        assert_eq!(full.len(), 1);
        assert_eq!(full.get(0).unwrap().right(), u32::MAX);
    }
}
