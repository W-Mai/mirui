use core::{iter::FusedIterator, slice::ChunksExact};

use crate::wire::{read_u16_le, read_u32_le};

pub const CMAP_INDEX_RECORD_LEN: usize = 6;
pub const GLYPH_ID_RECORD_LEN: usize = 2;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GlyphId(u16);

impl GlyphId {
    pub const NOTDEF: Self = Self(0);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CmapEntry {
    scalar: char,
    glyph_id: GlyphId,
}

impl CmapEntry {
    pub const fn new(scalar: char, glyph_id: GlyphId) -> Self {
        Self { scalar, glyph_id }
    }

    pub const fn scalar(self) -> char {
        self.scalar
    }

    pub const fn glyph_id(self) -> GlyphId {
        self.glyph_id
    }

    pub fn encode_record_into(self, output: &mut [u8]) -> Result<usize, CmapIndexError> {
        if output.len() < CMAP_INDEX_RECORD_LEN {
            return Err(CmapIndexError::BufferTooSmall {
                needed: CMAP_INDEX_RECORD_LEN,
                available: output.len(),
            });
        }
        output[..4].copy_from_slice(&(self.scalar as u32).to_le_bytes());
        output[4..CMAP_INDEX_RECORD_LEN].copy_from_slice(&self.glyph_id.get().to_le_bytes());
        Ok(CMAP_INDEX_RECORD_LEN)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CmapIndex<'a> {
    bytes: &'a [u8],
}

impl<'a> CmapIndex<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, CmapIndexError> {
        if bytes.len() % CMAP_INDEX_RECORD_LEN != 0 {
            return Err(CmapIndexError::PartialRecord {
                byte_len: bytes.len(),
            });
        }
        let mut previous = None;
        for (index, record) in bytes.chunks_exact(CMAP_INDEX_RECORD_LEN).enumerate() {
            let value = read_u32_le(record, 0).expect("complete cmap record");
            let scalar =
                char::from_u32(value).ok_or(CmapIndexError::InvalidScalar { index, value })?;
            if let Some(previous) = previous
                && scalar <= previous
            {
                return Err(CmapIndexError::NotSorted {
                    index,
                    previous,
                    current: scalar,
                });
            }
            previous = Some(scalar);
        }
        Ok(Self { bytes })
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / CMAP_INDEX_RECORD_LEN
    }

    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(self, index: usize) -> Option<CmapEntry> {
        let offset = index.checked_mul(CMAP_INDEX_RECORD_LEN)?;
        let record = self.bytes.get(offset..offset + CMAP_INDEX_RECORD_LEN)?;
        Some(CmapEntry {
            scalar: char::from_u32(read_u32_le(record, 0)?).expect("validated cmap scalar"),
            glyph_id: GlyphId::new(read_u16_le(record, 4)?),
        })
    }

    pub fn lookup(self, scalar: char) -> Option<GlyphId> {
        let mut start = 0;
        let mut end = self.len();
        while start < end {
            let middle = start + (end - start) / 2;
            let entry = self.get(middle).expect("validated cmap index");
            match entry.scalar.cmp(&scalar) {
                core::cmp::Ordering::Less => start = middle + 1,
                core::cmp::Ordering::Greater => end = middle,
                core::cmp::Ordering::Equal => return Some(entry.glyph_id),
            }
        }
        None
    }

    pub fn iter(self) -> CmapIter<'a> {
        CmapIter {
            records: self.bytes.chunks_exact(CMAP_INDEX_RECORD_LEN),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CmapIndexError {
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    PartialRecord {
        byte_len: usize,
    },
    InvalidScalar {
        index: usize,
        value: u32,
    },
    NotSorted {
        index: usize,
        previous: char,
        current: char,
    },
}

#[derive(Clone, Debug)]
pub struct CmapIter<'a> {
    records: ChunksExact<'a, u8>,
}

impl Iterator for CmapIter<'_> {
    type Item = CmapEntry;

    fn next(&mut self) -> Option<Self::Item> {
        self.records.next().map(cmap_entry)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.records.nth(n).map(cmap_entry)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.records.size_hint()
    }

    fn count(self) -> usize {
        self.records.len()
    }
}

impl DoubleEndedIterator for CmapIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.records.next_back().map(cmap_entry)
    }
}

impl ExactSizeIterator for CmapIter<'_> {
    fn len(&self) -> usize {
        self.records.len()
    }
}

impl FusedIterator for CmapIter<'_> {}

fn cmap_entry(record: &[u8]) -> CmapEntry {
    CmapEntry {
        scalar: char::from_u32(read_u32_le(record, 0).expect("complete cmap record"))
            .expect("validated cmap scalar"),
        glyph_id: GlyphId::new(read_u16_le(record, 4).expect("complete cmap record")),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GlyphIds<'a> {
    bytes: &'a [u8],
}

impl<'a> GlyphIds<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, GlyphIdsError> {
        if bytes.len() % GLYPH_ID_RECORD_LEN != 0 {
            return Err(GlyphIdsError::PartialRecord {
                byte_len: bytes.len(),
            });
        }
        let mut previous = None;
        for (index, record) in bytes.chunks_exact(GLYPH_ID_RECORD_LEN).enumerate() {
            let current = read_u16_le(record, 0).expect("complete glyph ID record");
            if let Some(previous) = previous
                && current <= previous
            {
                return Err(GlyphIdsError::NotSorted {
                    index,
                    previous: GlyphId::new(previous),
                    current: GlyphId::new(current),
                });
            }
            previous = Some(current);
        }
        Ok(Self { bytes })
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / GLYPH_ID_RECORD_LEN
    }

    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(self, ordinal: usize) -> Option<GlyphId> {
        let offset = ordinal.checked_mul(GLYPH_ID_RECORD_LEN)?;
        read_u16_le(self.bytes, offset).map(GlyphId::new)
    }

    pub fn ordinal(self, glyph_id: GlyphId) -> Result<usize, usize> {
        let mut start = 0;
        let mut end = self.len();
        while start < end {
            let middle = start + (end - start) / 2;
            match self
                .get(middle)
                .expect("validated glyph ID index")
                .cmp(&glyph_id)
            {
                core::cmp::Ordering::Less => start = middle + 1,
                core::cmp::Ordering::Greater => end = middle,
                core::cmp::Ordering::Equal => return Ok(middle),
            }
        }
        Err(start)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GlyphIdsError {
    PartialRecord {
        byte_len: usize,
    },
    NotSorted {
        index: usize,
        previous: GlyphId,
        current: GlyphId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmap_maps_sorted_scalars_to_repeating_glyphs() {
        let mut bytes = [0; 3 * CMAP_INDEX_RECORD_LEN];
        for (record, (scalar, glyph)) in bytes.chunks_exact_mut(CMAP_INDEX_RECORD_LEN).zip([
            ('A', 7_u16),
            ('B', 7),
            ('\u{10ffff}', 23),
        ]) {
            record[..4].copy_from_slice(&(scalar as u32).to_le_bytes());
            record[4..].copy_from_slice(&glyph.to_le_bytes());
        }
        let cmap = CmapIndex::open(&bytes).unwrap();
        assert_eq!(cmap.lookup('A'), Some(GlyphId::new(7)));
        assert_eq!(cmap.lookup('C'), None);
        assert_eq!(cmap.get(2).unwrap().scalar(), '\u{10ffff}');
        assert_eq!(cmap.iter().count(), 3);

        let mut output = [0x5a; CMAP_INDEX_RECORD_LEN + 1];
        assert_eq!(
            CmapEntry::new('中', GlyphId::new(42)).encode_record_into(&mut output),
            Ok(CMAP_INDEX_RECORD_LEN)
        );
        assert_eq!(&output[..4], &('中' as u32).to_le_bytes());
        assert_eq!(&output[4..6], &42_u16.to_le_bytes());
        assert_eq!(output[6], 0x5a);
    }

    #[test]
    fn cmap_rejects_partial_invalid_and_unsorted_records() {
        assert_eq!(
            CmapIndex::open(&[0; 5]),
            Err(CmapIndexError::PartialRecord { byte_len: 5 })
        );
        let mut invalid = [0; CMAP_INDEX_RECORD_LEN];
        invalid[..4].copy_from_slice(&0xd800_u32.to_le_bytes());
        assert_eq!(
            CmapIndex::open(&invalid),
            Err(CmapIndexError::InvalidScalar {
                index: 0,
                value: 0xd800
            })
        );
        let mut duplicate = [0; 2 * CMAP_INDEX_RECORD_LEN];
        for record in duplicate.chunks_exact_mut(CMAP_INDEX_RECORD_LEN) {
            record[..4].copy_from_slice(&('A' as u32).to_le_bytes());
        }
        assert_eq!(
            CmapIndex::open(&duplicate),
            Err(CmapIndexError::NotSorted {
                index: 1,
                previous: 'A',
                current: 'A'
            })
        );
    }

    #[test]
    fn sparse_glyph_ids_map_between_ordinals_and_shaping_ids() {
        let mut bytes = [0; 6];
        for (record, glyph) in bytes.chunks_exact_mut(2).zip([0_u16, 9, u16::MAX]) {
            record.copy_from_slice(&glyph.to_le_bytes());
        }
        let ids = GlyphIds::open(&bytes).unwrap();
        assert_eq!(ids.get(1), Some(GlyphId::new(9)));
        assert_eq!(ids.ordinal(GlyphId::new(9)), Ok(1));
        assert_eq!(ids.ordinal(GlyphId::new(8)), Err(1));
        assert_eq!(ids.get(usize::MAX), None);
    }

    #[test]
    fn sparse_glyph_ids_reject_partial_duplicate_and_descending_records() {
        assert_eq!(
            GlyphIds::open(&[0]),
            Err(GlyphIdsError::PartialRecord { byte_len: 1 })
        );
        for values in [[2_u16, 2_u16], [2, 1]] {
            let mut bytes = [0; 4];
            bytes[..2].copy_from_slice(&values[0].to_le_bytes());
            bytes[2..].copy_from_slice(&values[1].to_le_bytes());
            assert!(matches!(
                GlyphIds::open(&bytes),
                Err(GlyphIdsError::NotSorted { index: 1, .. })
            ));
        }
    }
}
