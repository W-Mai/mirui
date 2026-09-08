#![doc = include_str!("../../docs/glyph-maps.md")]

use core::iter::FusedIterator;

use crate::image::{
    ATLAS_REGION_LEN, AtlasMap, AtlasMapError, Region, RegionError, TileGrid, TileGridError,
};

pub const GLYPH_REGION_LEN: usize = ATLAS_REGION_LEN;

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

/// Validated glyph-ordinal mapping with allocation-free native and wire access.
///
/// Coordinates address logical samples, not byte offsets. Fixed-cell maps
/// omit records; atlas maps store x/y/width/height without repeating codepoints,
/// metrics, coding or alignment. Shared and overlapping atlas regions are
/// permitted: they reference existing samples rather than write to them.
#[derive(Clone, Copy, Debug)]
pub struct GlyphMap<'a> {
    source: MapSource<'a>,
}

impl<'a> GlyphMap<'a> {
    /// Derives fixed-cell regions in a vertical logical surface.
    ///
    /// Cell dimensions must be nonzero. Physical row and cell padding are not
    /// described by this map; their addresses require the storage layout.
    pub fn glyph_major(width: u32, height: u32, count: usize) -> Result<Self, GlyphMapError> {
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

    /// Borrows native rectangles and validates every region against the atlas.
    pub fn atlas(width: u32, height: u32, regions: &'a [Region]) -> Result<Self, GlyphMapError> {
        AtlasMap::new(width, height, regions)
            .map(|map| Self {
                source: MapSource::Atlas(map),
            })
            .map_err(map_atlas_error)
    }

    /// Borrows an exact atlas-map body of little-endian 16-byte records.
    ///
    /// Count derives from byte length. Empty maps remain Atlas2D, not implicit
    /// GlyphMajor. No record alignment or decoded-array allocation is required.
    pub fn from_records(width: u32, height: u32, bytes: &'a [u8]) -> Result<Self, GlyphMapError> {
        AtlasMap::open(width, height, bytes)
            .map(|map| Self {
                source: MapSource::Atlas(map),
            })
            .map_err(map_atlas_error)
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

    /// Exact map byte count; implicit fixed cells require zero bytes.
    pub fn encoded_len(self) -> usize {
        match self.source {
            MapSource::Grid(_) => 0,
            MapSource::Atlas(map) => map.encoded_len(),
        }
    }

    /// Emits canonical map bytes without allocation or partially written errors.
    /// The output suffix is preserved, including all output for implicit maps.
    pub fn encode_into(self, out: &mut [u8]) -> Result<usize, GlyphMapError> {
        match self.source {
            MapSource::Grid(_) => Ok(0),
            MapSource::Atlas(map) => map.encode_into(out).map_err(map_atlas_error),
        }
    }
}

fn map_atlas_error(error: AtlasMapError) -> GlyphMapError {
    match error {
        AtlasMapError::SizeOverflow => GlyphMapError::SizeOverflow,
        AtlasMapError::PartialRecord { byte_len } => GlyphMapError::PartialRecord { byte_len },
        AtlasMapError::InvalidRegion { index, error } => {
            GlyphMapError::InvalidRegion { index, error }
        }
        AtlasMapError::NonCanonicalEmpty { index } => GlyphMapError::NonCanonicalEmpty { index },
        AtlasMapError::BufferTooSmall { needed, available } => {
            GlyphMapError::BufferTooSmall { needed, available }
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

/// Invalid map geometry, record bounds, or output capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GlyphMapError {
    SizeOverflow,
    Grid(TileGridError),
    PartialRecord { byte_len: usize },
    InvalidRegion { index: usize, error: RegionError },
    NonCanonicalEmpty { index: usize },
    BufferTooSmall { needed: usize, available: usize },
}

#[cfg(test)]
mod tests;
