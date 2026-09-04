use core::{iter::FusedIterator, slice::ChunksExact};

use crate::wire::read_u32_le;

const CODEPOINT_LEN: usize = 4;

/// Failure while validating a shared FONT Unicode scalar table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontCodepointError {
    PartialRecord {
        byte_len: usize,
    },
    InvalidScalar {
        index: usize,
        value: u32,
    },
    NotSorted {
        index: usize,
        previous: u32,
        current: u32,
    },
}

/// Borrowed, strictly sorted Unicode scalars indexed by shared glyph ordinal.
///
/// Records are little-endian u32 values, not UTF-8 text or aligned Rust chars.
/// Validation scans the table once; lookup and iteration do not allocate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FontCodepoints<'a> {
    bytes: &'a [u8],
}

impl<'a> FontCodepoints<'a> {
    /// Validates a complete CODEPOINTS section body, including an empty table.
    pub fn open(bytes: &'a [u8]) -> Result<Self, FontCodepointError> {
        if bytes.len() % CODEPOINT_LEN != 0 {
            return Err(FontCodepointError::PartialRecord {
                byte_len: bytes.len(),
            });
        }
        let mut previous = None;
        for (index, record) in bytes.chunks_exact(CODEPOINT_LEN).enumerate() {
            let value = read_u32_le(record, 0).expect("complete codepoint record");
            if char::from_u32(value).is_none() {
                return Err(FontCodepointError::InvalidScalar { index, value });
            }
            if let Some(previous) = previous {
                if value <= previous {
                    return Err(FontCodepointError::NotSorted {
                        index,
                        previous,
                        current: value,
                    });
                }
            }
            previous = Some(value);
        }
        Ok(Self { bytes })
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / CODEPOINT_LEN
    }

    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(self, index: usize) -> Option<char> {
        let offset = index.checked_mul(CODEPOINT_LEN)?;
        char::from_u32(read_u32_le(self.bytes, offset)?)
    }

    /// Returns the glyph ordinal, or the insertion ordinal when absent.
    pub fn binary_search(self, codepoint: char) -> Result<usize, usize> {
        let mut start = 0;
        let mut end = self.len();
        while start < end {
            let middle = start + (end - start) / 2;
            match self
                .get(middle)
                .expect("validated glyph ordinal")
                .cmp(&codepoint)
            {
                core::cmp::Ordering::Less => start = middle + 1,
                core::cmp::Ordering::Greater => end = middle,
                core::cmp::Ordering::Equal => return Ok(middle),
            }
        }
        Err(start)
    }

    pub fn iter(self) -> FontCodepointIter<'a> {
        FontCodepointIter {
            records: self.bytes.chunks_exact(CODEPOINT_LEN),
        }
    }
}

impl<'a> IntoIterator for FontCodepoints<'a> {
    type Item = char;
    type IntoIter = FontCodepointIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Exact-size, double-ended iteration over shared FONT glyph ordinals.
#[derive(Clone, Debug)]
pub struct FontCodepointIter<'a> {
    records: ChunksExact<'a, u8>,
}

impl FontCodepointIter<'_> {
    fn scalar(record: &[u8]) -> char {
        char::from_u32(read_u32_le(record, 0).expect("complete codepoint record"))
            .expect("validated Unicode scalar")
    }
}

impl Iterator for FontCodepointIter<'_> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        self.records.next().map(Self::scalar)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.records.nth(n).map(Self::scalar)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.records.size_hint()
    }

    fn count(self) -> usize {
        self.records.len()
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for FontCodepointIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.records.next_back().map(Self::scalar)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.records.nth_back(n).map(Self::scalar)
    }
}

impl ExactSizeIterator for FontCodepointIter<'_> {
    fn len(&self) -> usize {
        self.records.len()
    }
}

impl FusedIterator for FontCodepointIter<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unaligned_wire_values_cover_unicode_and_share_ordinals() {
        let scalars = ['\0', 'A', '中', '\u{d7ff}', '\u{e000}', '😀', '\u{10ffff}'];
        #[repr(align(4))]
        struct Bytes([u8; 1 + 7 * CODEPOINT_LEN]);
        let mut storage = Bytes([0; 1 + 7 * CODEPOINT_LEN]);
        for (record, scalar) in storage.0[1..].chunks_exact_mut(CODEPOINT_LEN).zip(scalars) {
            record.copy_from_slice(&(scalar as u32).to_le_bytes());
        }
        let table = FontCodepoints::open(&storage.0[1..]).unwrap();
        assert_eq!(table.as_bytes().as_ptr(), storage.0[1..].as_ptr());
        assert_eq!(table.as_bytes().as_ptr() as usize % CODEPOINT_LEN, 1);
        assert_eq!(table.len(), scalars.len());
        for (index, scalar) in scalars.into_iter().enumerate() {
            assert_eq!(table.get(index), Some(scalar));
            assert_eq!(table.binary_search(scalar), Ok(index));
        }
        assert_eq!(table.get(usize::MAX), None);
        assert_eq!(table.get(scalars.len()), None);
        assert_eq!(table.binary_search('B'), Err(2));
        assert_eq!(table.iter().count(), scalars.len());
        assert_eq!(table.iter().last(), Some('\u{10ffff}'));
        let mut iter = table.iter();
        assert_eq!(iter.next(), Some('\0'));
        assert_eq!(iter.next_back(), Some('\u{10ffff}'));
        assert_eq!(iter.nth(1), Some('中'));
        assert_eq!(iter.nth_back(1), Some('\u{e000}'));
        assert_eq!(iter.len(), 1);
        assert_eq!(iter.next(), Some('\u{d7ff}'));
        assert_eq!(iter.next_back(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn invalid_scalars_duplicates_and_unsorted_records_are_rejected() {
        for value in [0xd800_u32, 0xdfff, 0x110000, u32::MAX] {
            assert_eq!(
                FontCodepoints::open(&value.to_le_bytes()),
                Err(FontCodepointError::InvalidScalar { index: 0, value })
            );
        }
        for second in [64_u32, 65] {
            let mut bytes = [0; 8];
            bytes[..4].copy_from_slice(&65_u32.to_le_bytes());
            bytes[4..].copy_from_slice(&second.to_le_bytes());
            assert_eq!(
                FontCodepoints::open(&bytes),
                Err(FontCodepointError::NotSorted {
                    index: 1,
                    previous: 65,
                    current: second
                })
            );
        }
        for byte_len in [1, 2, 3, 5, 6, 7] {
            assert_eq!(
                FontCodepoints::open(&[0; 8][..byte_len]),
                Err(FontCodepointError::PartialRecord { byte_len })
            );
        }
        let empty = FontCodepoints::open(&[]).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.binary_search('A'), Err(0));
        assert_eq!(empty.iter().next(), None);
    }
}
