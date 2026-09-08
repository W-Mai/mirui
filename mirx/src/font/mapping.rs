#![doc = include_str!("../../docs/glyph-maps.md")]

use core::iter::FusedIterator;

use crate::image::{AtlasMap, Region, TileGrid, TileGridError};

/// Logical raster organization, independent of coding and physical alignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GlyphPacking {
    /// Fixed cells in glyph order, with no per-glyph map bytes.
    GlyphMajor,
    /// Explicit glyph regions in a shared two-dimensional surface.
    Atlas2D,
}

#[derive(Clone, Copy, Debug)]
enum MapSource<'a> {
    Grid(TileGrid),
    Atlas(AtlasMap<'a>),
}

/// Allocation-free raster-ordinal binding for derived cells or a shared atlas.
#[derive(Clone, Copy, Debug)]
pub struct GlyphMap<'a> {
    source: MapSource<'a>,
}

impl<'a> GlyphMap<'a> {
    /// Derives fixed-cell regions in a vertical logical surface.
    ///
    /// Cell dimensions must be nonzero. Physical row and cell padding are not
    /// described by this map; their addresses require the storage layout.
    pub fn cells(width: u32, height: u32, count: usize) -> Result<Self, GlyphMapError> {
        let count = u32::try_from(count).map_err(|_| GlyphMapError::SizeOverflow)?;
        let total_height = height
            .checked_mul(count)
            .ok_or(GlyphMapError::SizeOverflow)?;
        let grid =
            TileGrid::new(width, total_height, width, height).map_err(GlyphMapError::Grid)?;
        Ok(Self {
            source: MapSource::Grid(grid),
        })
    }

    pub const fn atlas(map: AtlasMap<'a>) -> Self {
        Self {
            source: MapSource::Atlas(map),
        }
    }

    pub const fn packing(self) -> GlyphPacking {
        match self.source {
            MapSource::Grid(_) => GlyphPacking::GlyphMajor,
            MapSource::Atlas(_) => GlyphPacking::Atlas2D,
        }
    }

    pub const fn width(self) -> u32 {
        match self.source {
            MapSource::Grid(grid) => grid.width(),
            MapSource::Atlas(map) => map.width(),
        }
    }

    pub const fn height(self) -> u32 {
        match self.source {
            MapSource::Grid(grid) => grid.height(),
            MapSource::Atlas(map) => map.height(),
        }
    }

    pub const fn cell_extent(self) -> Option<(u32, u32)> {
        match self.source {
            MapSource::Grid(grid) => Some((grid.tile_width(), grid.tile_height())),
            MapSource::Atlas(_) => None,
        }
    }

    pub const fn atlas_map(self) -> Option<AtlasMap<'a>> {
        match self.source {
            MapSource::Grid(_) => None,
            MapSource::Atlas(map) => Some(map),
        }
    }

    pub fn len(self) -> usize {
        match self.source {
            MapSource::Grid(grid) => grid.len(),
            MapSource::Atlas(map) => map.len(),
        }
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn get(self, index: usize) -> Option<Region> {
        match self.source {
            MapSource::Grid(grid) => grid.get(index),
            MapSource::Atlas(map) => map.get(index),
        }
    }

    pub fn iter(self) -> GlyphRegions<'a> {
        GlyphRegions {
            map: self,
            front: 0,
            back: self.len(),
        }
    }
}

impl<'a> IntoIterator for GlyphMap<'a> {
    type Item = Region;
    type IntoIter = GlyphRegions<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Exact-size, double-ended region iteration with constant-time skips.
#[derive(Clone, Debug)]
pub struct GlyphRegions<'a> {
    map: GlyphMap<'a>,
    front: usize,
    back: usize,
}

impl Iterator for GlyphRegions<'_> {
    type Item = Region;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.map.get(index)
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

impl DoubleEndedIterator for GlyphRegions<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.map.get(self.back)
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

impl ExactSizeIterator for GlyphRegions<'_> {
    fn len(&self) -> usize {
        self.back - self.front
    }
}

impl FusedIterator for GlyphRegions<'_> {}

/// Invalid implicit-cell geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GlyphMapError {
    SizeOverflow,
    Grid(TileGridError),
}

#[cfg(test)]
mod tests;
