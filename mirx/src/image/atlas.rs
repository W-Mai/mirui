#![doc = include_str!("../../docs/atlas-map.md")]

use core::iter::FusedIterator;

use super::{Region, RegionError};
use crate::wire::{read_u32_le, write_u32_le};

pub const ATLAS_REGION_LEN: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Records<'a> {
    Native(&'a [Region]),
    Wire(&'a [u8]),
}

impl Records<'_> {
    fn len(self) -> usize {
        match self {
            Self::Native(regions) => regions.len(),
            Self::Wire(bytes) => bytes.len() / ATLAS_REGION_LEN,
        }
    }

    fn fields(self, index: usize) -> Option<(u32, u32, u32, u32)> {
        match self {
            Self::Native(regions) => regions
                .get(index)
                .map(|region| (region.x(), region.y(), region.width(), region.height())),
            Self::Wire(bytes) => {
                let offset = index.checked_mul(ATLAS_REGION_LEN)?;
                Some((
                    read_u32_le(bytes, offset)?,
                    read_u32_le(bytes, offset + 4)?,
                    read_u32_le(bytes, offset + 8)?,
                    read_u32_le(bytes, offset + 12)?,
                ))
            }
        }
    }

    fn region(self, index: usize) -> Option<Result<Region, RegionError>> {
        let (x, y, width, height) = self.fields(index)?;
        Some(Region::new(x, y, width, height))
    }
}

/// Ordered rectangles within one logical atlas extent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtlasMap<'a> {
    width: u32,
    height: u32,
    records: Records<'a>,
}

impl<'a> AtlasMap<'a> {
    /// Borrows native rectangles and validates the complete map.
    pub fn new(width: u32, height: u32, regions: &'a [Region]) -> Result<Self, AtlasMapError> {
        Self::from_source(width, height, Records::Native(regions))
    }

    /// Borrows canonical little-endian 16-byte rectangle records.
    pub fn open(width: u32, height: u32, bytes: &'a [u8]) -> Result<Self, AtlasMapError> {
        u32::try_from(bytes.len()).map_err(|_| AtlasMapError::SizeOverflow)?;
        if bytes.len() % ATLAS_REGION_LEN != 0 {
            return Err(AtlasMapError::PartialRecord {
                byte_len: bytes.len(),
            });
        }
        Self::from_source(width, height, Records::Wire(bytes))
    }

    fn from_source(width: u32, height: u32, records: Records<'a>) -> Result<Self, AtlasMapError> {
        let encoded_len = records
            .len()
            .checked_mul(ATLAS_REGION_LEN)
            .ok_or(AtlasMapError::SizeOverflow)?;
        u32::try_from(encoded_len).map_err(|_| AtlasMapError::SizeOverflow)?;
        for index in 0..records.len() {
            let (x, y, region_width, region_height) =
                records.fields(index).expect("valid atlas record index");
            if (region_width == 0 || region_height == 0)
                && (x != 0 || y != 0 || region_width != 0 || region_height != 0)
            {
                return Err(AtlasMapError::NonCanonicalEmpty { index });
            }
            let region = records
                .region(index)
                .expect("valid atlas record index")
                .map_err(|error| AtlasMapError::InvalidRegion { index, error })?;
            if region.right() > width || region.bottom() > height {
                return Err(AtlasMapError::InvalidRegion {
                    index,
                    error: RegionError::OutOfBounds,
                });
            }
        }
        Ok(Self {
            width,
            height,
            records,
        })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub fn len(self) -> usize {
        self.records.len()
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn get(self, index: usize) -> Option<Region> {
        self.records
            .region(index)
            .map(|region| region.expect("validated atlas region"))
    }

    pub fn iter(self) -> AtlasRegions<'a> {
        AtlasRegions {
            map: self,
            front: 0,
            back: self.len(),
        }
    }

    pub fn encoded_len(self) -> usize {
        self.records.len() * ATLAS_REGION_LEN
    }

    /// Writes canonical records while preserving bytes beyond the returned length.
    pub fn encode_into(self, out: &mut [u8]) -> Result<usize, AtlasMapError> {
        let needed = self.encoded_len();
        if out.len() < needed {
            return Err(AtlasMapError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        match self.records {
            Records::Wire(bytes) => out[..needed].copy_from_slice(bytes),
            Records::Native(_) => {
                for (record, region) in out[..needed].chunks_exact_mut(ATLAS_REGION_LEN).zip(self) {
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

impl<'a> IntoIterator for AtlasMap<'a> {
    type Item = Region;
    type IntoIter = AtlasRegions<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone, Debug)]
pub struct AtlasRegions<'a> {
    map: AtlasMap<'a>,
    front: usize,
    back: usize,
}

impl Iterator for AtlasRegions<'_> {
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

impl DoubleEndedIterator for AtlasRegions<'_> {
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

impl ExactSizeIterator for AtlasRegions<'_> {
    fn len(&self) -> usize {
        self.back - self.front
    }
}

impl FusedIterator for AtlasRegions<'_> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AtlasMapError {
    SizeOverflow,
    PartialRecord { byte_len: usize },
    InvalidRegion { index: usize, error: RegionError },
    NonCanonicalEmpty { index: usize },
    BufferTooSmall { needed: usize, available: usize },
}

#[cfg(test)]
mod tests;
