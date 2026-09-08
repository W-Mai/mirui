#![doc = include_str!("../../docs/glyph-maps.md")]

use core::iter::FusedIterator;

use crate::image::{Region, RegionError, TileGrid, TileGridError};
use crate::wire::{read_u32_le, write_u32_le};

pub const GLYPH_REGION_LEN: usize = 16;

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
    Atlas {
        width: u32,
        height: u32,
        records: AtlasRecords<'a>,
    },
}

#[derive(Clone, Copy, Debug)]
enum AtlasRecords<'a> {
    Native(&'a [Region]),
    Wire(&'a [u8]),
}

impl AtlasRecords<'_> {
    fn len(self) -> usize {
        match self {
            Self::Native(regions) => regions.len(),
            Self::Wire(bytes) => bytes.len() / GLYPH_REGION_LEN,
        }
    }

    fn get(self, index: usize) -> Option<Result<Region, RegionError>> {
        match self {
            Self::Native(regions) => regions.get(index).copied().map(Ok),
            Self::Wire(bytes) => {
                if index >= self.len() {
                    return None;
                }
                let offset = index * GLYPH_REGION_LEN;
                Some(Region::new(
                    read_u32_le(bytes, offset).unwrap(),
                    read_u32_le(bytes, offset + 4).unwrap(),
                    read_u32_le(bytes, offset + 8).unwrap(),
                    read_u32_le(bytes, offset + 12).unwrap(),
                ))
            }
        }
    }

    fn fields(self, index: usize) -> Option<(u32, u32, u32, u32)> {
        match self {
            Self::Native(regions) => regions
                .get(index)
                .map(|region| (region.x(), region.y(), region.width(), region.height())),
            Self::Wire(bytes) => {
                let offset = index.checked_mul(GLYPH_REGION_LEN)?;
                Some((
                    read_u32_le(bytes, offset)?,
                    read_u32_le(bytes, offset + 4)?,
                    read_u32_le(bytes, offset + 8)?,
                    read_u32_le(bytes, offset + 12)?,
                ))
            }
        }
    }

    fn validate(self, width: u32, height: u32) -> Result<(), GlyphMapError> {
        let size = self
            .len()
            .checked_mul(GLYPH_REGION_LEN)
            .ok_or(GlyphMapError::SizeOverflow)?;
        u32::try_from(size).map_err(|_| GlyphMapError::SizeOverflow)?;
        for index in 0..self.len() {
            let (x, y, region_width, region_height) =
                self.fields(index).expect("valid record index");
            if (region_width == 0 || region_height == 0)
                && (x != 0 || y != 0 || region_width != 0 || region_height != 0)
            {
                return Err(GlyphMapError::NonCanonicalEmpty { index });
            }
            let region = self
                .get(index)
                .expect("valid record index")
                .map_err(|error| GlyphMapError::InvalidRegion { index, error })?;
            if region.right() > width || region.bottom() > height {
                return Err(GlyphMapError::InvalidRegion {
                    index,
                    error: RegionError::OutOfBounds,
                });
            }
        }
        Ok(())
    }
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
        Self::from_atlas(width, height, AtlasRecords::Native(regions))
    }

    /// Borrows an exact atlas-map body of little-endian 16-byte records.
    ///
    /// Count derives from byte length. Empty maps remain Atlas2D, not implicit
    /// GlyphMajor. No record alignment or decoded-array allocation is required.
    pub fn from_records(width: u32, height: u32, bytes: &'a [u8]) -> Result<Self, GlyphMapError> {
        u32::try_from(bytes.len()).map_err(|_| GlyphMapError::SizeOverflow)?;
        if bytes.len() % GLYPH_REGION_LEN != 0 {
            return Err(GlyphMapError::PartialRecord {
                byte_len: bytes.len(),
            });
        }
        Self::from_atlas(width, height, AtlasRecords::Wire(bytes))
    }

    fn from_atlas(
        width: u32,
        height: u32,
        records: AtlasRecords<'a>,
    ) -> Result<Self, GlyphMapError> {
        records.validate(width, height)?;
        Ok(Self {
            source: MapSource::Atlas {
                width,
                height,
                records,
            },
        })
    }

    /// The caller has already checked exact record boundaries and atlas bounds.
    pub(super) fn from_validated_records(width: u32, height: u32, bytes: &'a [u8]) -> Self {
        Self {
            source: MapSource::Atlas {
                width,
                height,
                records: AtlasRecords::Wire(bytes),
            },
        }
    }

    pub const fn packing(self) -> GlyphPacking {
        match self.source {
            MapSource::Grid(_) => GlyphPacking::GlyphMajor,
            MapSource::Atlas { .. } => GlyphPacking::Atlas2D,
        }
    }

    pub const fn width(self) -> u32 {
        match self.source {
            MapSource::Grid(grid) => grid.width(),
            MapSource::Atlas { width, .. } => width,
        }
    }

    pub const fn height(self) -> u32 {
        match self.source {
            MapSource::Grid(grid) => grid.height(),
            MapSource::Atlas { height, .. } => height,
        }
    }

    pub const fn cell_extent(self) -> Option<(u32, u32)> {
        match self.source {
            MapSource::Grid(grid) => Some((grid.tile_width(), grid.tile_height())),
            MapSource::Atlas { .. } => None,
        }
    }

    pub fn len(self) -> usize {
        match self.source {
            MapSource::Grid(grid) => grid.len(),
            MapSource::Atlas { records, .. } => records.len(),
        }
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn get(self, index: usize) -> Option<Region> {
        match self.source {
            MapSource::Grid(grid) => grid.get(index),
            MapSource::Atlas { records, .. } => records
                .get(index)
                .map(|r| r.expect("validated glyph region")),
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
            MapSource::Atlas { records, .. } => records.len() * GLYPH_REGION_LEN,
        }
    }

    /// Emits canonical map bytes without allocation or partially written errors.
    /// The output suffix is preserved, including all output for implicit maps.
    pub fn encode_into(self, out: &mut [u8]) -> Result<usize, GlyphMapError> {
        let needed = self.encoded_len();
        if out.len() < needed {
            return Err(GlyphMapError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        match self.source {
            MapSource::Grid(_) => {}
            MapSource::Atlas {
                records: AtlasRecords::Wire(bytes),
                ..
            } => out[..needed].copy_from_slice(bytes),
            MapSource::Atlas { .. } => {
                for (record, region) in out[..needed].chunks_exact_mut(GLYPH_REGION_LEN).zip(self) {
                    write_u32_le(record, 0, region.x());
                    write_u32_le(record, 4, region.y());
                    write_u32_le(record, 8, region.width());
                    write_u32_le(record, 12, region.height());
                }
            }
        }
        Ok(needed)
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
