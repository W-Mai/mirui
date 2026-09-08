use super::{CmapEntry, CmapIndex, GlyphId};
use crate::PayloadLimits;

const SFNT_HEADER_LEN: usize = 12;
const SFNT_TABLE_RECORD_LEN: usize = 16;

const TRUETYPE: [u8; 4] = [0x00, 0x01, 0x00, 0x00];
const OPENTYPE_CFF: [u8; 4] = *b"OTTO";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShapingData<'a> {
    bytes: &'a [u8],
    table_count: u16,
}

impl<'a> ShapingData<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, ShapingDataError> {
        if bytes.len() < SFNT_HEADER_LEN {
            return Err(ShapingDataError::TruncatedHeader {
                available: bytes.len(),
            });
        }
        let signature: [u8; 4] = bytes[..4].try_into().expect("complete SFNT header");
        if signature != TRUETYPE && signature != OPENTYPE_CFF {
            return Err(ShapingDataError::UnsupportedSignature(signature));
        }
        let table_count = u16::from_be_bytes(bytes[4..6].try_into().unwrap());
        if table_count == 0 {
            return Err(ShapingDataError::EmptyDirectory);
        }
        let directory_len = usize::from(table_count)
            .checked_mul(SFNT_TABLE_RECORD_LEN)
            .and_then(|value| value.checked_add(SFNT_HEADER_LEN))
            .ok_or(ShapingDataError::SizeOverflow)?;
        if bytes.len() < directory_len {
            return Err(ShapingDataError::TruncatedDirectory {
                needed: directory_len,
                available: bytes.len(),
            });
        }
        let mut previous = None;
        for index in 0..usize::from(table_count) {
            let record = Self::record_at(bytes, index);
            let tag: [u8; 4] = record[..4].try_into().unwrap();
            if let Some(previous) = previous
                && tag <= previous
            {
                return Err(ShapingDataError::UnsortedTable {
                    index,
                    previous,
                    current: tag,
                });
            }
            previous = Some(tag);

            let offset = u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize;
            let length = u32::from_be_bytes(record[12..16].try_into().unwrap()) as usize;
            if offset % 4 != 0 {
                return Err(ShapingDataError::MisalignedTable { index, offset });
            }
            if length == 0 {
                return Err(ShapingDataError::EmptyTable { index, tag });
            }
            let end = offset
                .checked_add(length)
                .ok_or(ShapingDataError::SizeOverflow)?;
            if offset < directory_len || end > bytes.len() {
                return Err(ShapingDataError::TableOutOfBounds {
                    index,
                    offset,
                    length,
                });
            }
        }

        let shaping = Self { bytes, table_count };
        for required in [*b"OS/2", *b"cmap", *b"head", *b"hhea", *b"hmtx", *b"maxp"] {
            if shaping.table(required).is_none() {
                return Err(ShapingDataError::MissingTable(required));
            }
        }
        let has_truetype_outlines =
            shaping.table(*b"glyf").is_some() && shaping.table(*b"loca").is_some();
        let has_cff_outlines =
            shaping.table(*b"CFF ").is_some() || shaping.table(*b"CFF2").is_some();
        if !has_truetype_outlines && !has_cff_outlines {
            return Err(ShapingDataError::MissingOutlines);
        }
        let cmap = ttf_parser::cmap::Table::parse(shaping.table(*b"cmap").unwrap())
            .ok_or(ShapingDataError::MalformedCmap)?;
        if !cmap
            .subtables
            .into_iter()
            .any(|subtable| subtable.is_unicode())
        {
            return Err(ShapingDataError::MalformedCmap);
        }
        Ok(shaping)
    }

    pub(crate) fn validate_cmap(self, index: CmapIndex<'_>) -> Result<(), ShapingDataError> {
        self.validate_cmap_by(index.iter(), |scalar| index.lookup(scalar))
    }

    pub(in crate::font) fn validate_cmap_entries(
        self,
        entries: &[CmapEntry],
    ) -> Result<(), ShapingDataError> {
        self.validate_cmap_by(entries.iter().copied(), |scalar| {
            entries
                .binary_search_by_key(&scalar, |entry| entry.scalar())
                .ok()
                .map(|index| entries[index].glyph_id())
        })
    }

    fn validate_cmap_by(
        self,
        entries: impl Iterator<Item = CmapEntry>,
        lookup: impl Fn(char) -> Option<GlyphId>,
    ) -> Result<(), ShapingDataError> {
        let table = ttf_parser::cmap::Table::parse(self.table(*b"cmap").unwrap())
            .ok_or(ShapingDataError::MalformedCmap)?;
        for entry in entries {
            let shaping = effective_glyph(table.subtables, entry.scalar() as u32);
            if shaping != Some(entry.glyph_id()) {
                return Err(ShapingDataError::CmapMismatch {
                    scalar: entry.scalar() as u32,
                    shaping,
                    index: Some(entry.glyph_id()),
                });
            }
        }

        let mut mismatch = None;
        for (subtable_index, subtable) in table.subtables.into_iter().enumerate() {
            if !subtable.is_unicode() {
                continue;
            }
            subtable.codepoints(|scalar| {
                if mismatch.is_some() || subtable.glyph_index(scalar).is_none() {
                    return;
                }
                if table
                    .subtables
                    .into_iter()
                    .take(subtable_index)
                    .any(|earlier| earlier.is_unicode() && earlier.glyph_index(scalar).is_some())
                {
                    return;
                }
                let Some(character) = char::from_u32(scalar) else {
                    mismatch = Some(ShapingDataError::CmapMismatch {
                        scalar,
                        shaping: effective_glyph(table.subtables, scalar),
                        index: None,
                    });
                    return;
                };
                let shaping = effective_glyph(table.subtables, scalar);
                let indexed = lookup(character);
                if shaping != indexed {
                    mismatch = Some(ShapingDataError::CmapMismatch {
                        scalar,
                        shaping,
                        index: indexed,
                    });
                }
            });
            if let Some(error) = mismatch {
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn preflight(self, limits: &PayloadLimits) -> Result<(), ShapingDataError> {
        if self.bytes.len() > limits.max_font_shaping_bytes() {
            return Err(ShapingDataError::LimitExceeded {
                limit: limits.max_font_shaping_bytes(),
                actual: self.bytes.len(),
            });
        }
        Ok(())
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn table_count(self) -> u16 {
        self.table_count
    }

    pub fn table(self, tag: [u8; 4]) -> Option<&'a [u8]> {
        let mut start = 0;
        let mut end = usize::from(self.table_count);
        while start < end {
            let middle = start + (end - start) / 2;
            let record = Self::record_at(self.bytes, middle);
            match record[..4].cmp(&tag) {
                core::cmp::Ordering::Less => start = middle + 1,
                core::cmp::Ordering::Greater => end = middle,
                core::cmp::Ordering::Equal => {
                    let offset = u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize;
                    let length = u32::from_be_bytes(record[12..16].try_into().unwrap()) as usize;
                    return self.bytes.get(offset..offset + length);
                }
            }
        }
        None
    }

    fn record_at(bytes: &[u8], index: usize) -> &[u8] {
        let offset = SFNT_HEADER_LEN + index * SFNT_TABLE_RECORD_LEN;
        &bytes[offset..offset + SFNT_TABLE_RECORD_LEN]
    }
}

#[cfg(test)]
pub(crate) fn test_sfnt() -> alloc::vec::Vec<u8> {
    let mut cmap = alloc::vec![0; 40];
    cmap[2..4].copy_from_slice(&1_u16.to_be_bytes());
    cmap[4..6].copy_from_slice(&3_u16.to_be_bytes());
    cmap[6..8].copy_from_slice(&10_u16.to_be_bytes());
    cmap[8..12].copy_from_slice(&12_u32.to_be_bytes());
    cmap[12..14].copy_from_slice(&13_u16.to_be_bytes());
    cmap[16..20].copy_from_slice(&28_u32.to_be_bytes());
    cmap[24..28].copy_from_slice(&1_u32.to_be_bytes());
    cmap[28..32].copy_from_slice(&('A' as u32).to_be_bytes());
    cmap[32..36].copy_from_slice(&('B' as u32).to_be_bytes());
    cmap[36..40].copy_from_slice(&1_u32.to_be_bytes());

    let directory_len = SFNT_HEADER_LEN + TEST_TAGS.len() * SFNT_TABLE_RECORD_LEN;
    let mut bytes = alloc::vec![0; directory_len + (TEST_TAGS.len() - 1) * 4 + cmap.len()];
    bytes[..4].copy_from_slice(&TRUETYPE);
    bytes[4..6].copy_from_slice(&(TEST_TAGS.len() as u16).to_be_bytes());
    let mut offset = directory_len;
    for (index, tag) in TEST_TAGS.into_iter().enumerate() {
        let record = SFNT_HEADER_LEN + index * SFNT_TABLE_RECORD_LEN;
        let body = if tag == *b"cmap" { &cmap[..] } else { &[0; 4] };
        bytes[record..record + 4].copy_from_slice(&tag);
        bytes[record + 8..record + 12].copy_from_slice(&(offset as u32).to_be_bytes());
        bytes[record + 12..record + 16].copy_from_slice(&(body.len() as u32).to_be_bytes());
        bytes[offset..offset + body.len()].copy_from_slice(body);
        offset += body.len();
    }
    bytes
}

#[cfg(test)]
const TEST_TAGS: [[u8; 4]; 8] = [
    *b"OS/2", *b"cmap", *b"glyf", *b"head", *b"hhea", *b"hmtx", *b"loca", *b"maxp",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ShapingDataError {
    TruncatedHeader {
        available: usize,
    },
    UnsupportedSignature([u8; 4]),
    EmptyDirectory,
    SizeOverflow,
    TruncatedDirectory {
        needed: usize,
        available: usize,
    },
    UnsortedTable {
        index: usize,
        previous: [u8; 4],
        current: [u8; 4],
    },
    MisalignedTable {
        index: usize,
        offset: usize,
    },
    EmptyTable {
        index: usize,
        tag: [u8; 4],
    },
    TableOutOfBounds {
        index: usize,
        offset: usize,
        length: usize,
    },
    MissingTable([u8; 4]),
    MissingOutlines,
    MalformedCmap,
    CmapMismatch {
        scalar: u32,
        shaping: Option<GlyphId>,
        index: Option<GlyphId>,
    },
    LimitExceeded {
        limit: usize,
        actual: usize,
    },
}

fn effective_glyph(subtables: ttf_parser::cmap::Subtables<'_>, scalar: u32) -> Option<GlyphId> {
    subtables
        .into_iter()
        .filter(|subtable| subtable.is_unicode())
        .find_map(|subtable| subtable.glyph_index(scalar))
        .map(|glyph| GlyphId::new(glyph.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sfnt() -> alloc::vec::Vec<u8> {
        test_sfnt()
    }

    #[test]
    fn shaping_data_borrows_sorted_sfnt_tables() {
        let bytes = sfnt();
        let shaping = ShapingData::open(&bytes).unwrap();
        assert_eq!(shaping.table_count(), 8);
        assert_eq!(shaping.table(*b"head"), Some(&[0; 4][..]));
        assert_eq!(shaping.table(*b"GSUB"), None);
        assert_eq!(shaping.as_bytes().as_ptr(), bytes.as_ptr());
    }

    #[test]
    fn shaping_data_enforces_its_independent_byte_limit() {
        let bytes = sfnt();
        let shaping = ShapingData::open(&bytes).unwrap();
        assert_eq!(
            shaping.preflight(&PayloadLimits::HOST.with_max_font_shaping_bytes(bytes.len() - 1)),
            Err(ShapingDataError::LimitExceeded {
                limit: bytes.len() - 1,
                actual: bytes.len()
            })
        );
        assert_eq!(
            shaping.preflight(&PayloadLimits::HOST.with_max_font_shaping_bytes(bytes.len())),
            Ok(())
        );
    }

    #[test]
    fn shaping_data_rejects_directory_and_table_bounds() {
        assert!(matches!(
            ShapingData::open(&[0; 11]),
            Err(ShapingDataError::TruncatedHeader { .. })
        ));
        let mut bytes = sfnt();
        bytes[SFNT_HEADER_LEN..SFNT_HEADER_LEN + 4].copy_from_slice(b"zzzz");
        assert!(matches!(
            ShapingData::open(&bytes),
            Err(ShapingDataError::UnsortedTable { .. })
        ));
        let mut bytes = sfnt();
        bytes[SFNT_HEADER_LEN + 8..SFNT_HEADER_LEN + 12].copy_from_slice(&1_u32.to_be_bytes());
        assert_eq!(
            ShapingData::open(&bytes),
            Err(ShapingDataError::MisalignedTable {
                index: 0,
                offset: 1
            })
        );
    }

    #[test]
    fn shaping_data_requires_metrics_cmap_and_outlines() {
        for missing in [*b"OS/2", *b"cmap", *b"head", *b"hhea", *b"hmtx", *b"maxp"] {
            let mut bytes = sfnt();
            let index = TEST_TAGS.iter().position(|tag| *tag == missing).unwrap();
            let record = SFNT_HEADER_LEN + index * SFNT_TABLE_RECORD_LEN;
            bytes[record..record + 4].copy_from_slice(b"zzzz");
            assert!(ShapingData::open(&bytes).is_err());
        }
        let mut bytes = sfnt();
        for tag in [*b"glyf", *b"loca"] {
            let index = TEST_TAGS
                .iter()
                .position(|candidate| *candidate == tag)
                .unwrap();
            let record = SFNT_HEADER_LEN + index * SFNT_TABLE_RECORD_LEN;
            bytes[record..record + 4].copy_from_slice(if tag == *b"glyf" {
                b"glya"
            } else {
                b"locb"
            });
        }
        assert_eq!(
            ShapingData::open(&bytes),
            Err(ShapingDataError::MissingOutlines)
        );
    }

    #[test]
    fn shaping_data_requires_a_parseable_unicode_cmap() {
        let mut bytes = sfnt();
        let record = SFNT_HEADER_LEN + SFNT_TABLE_RECORD_LEN;
        let offset =
            u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize;
        bytes[offset + 4..offset + 6].copy_from_slice(&1_u16.to_be_bytes());
        assert_eq!(
            ShapingData::open(&bytes),
            Err(ShapingDataError::MalformedCmap)
        );
    }
}
