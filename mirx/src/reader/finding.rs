use core::iter::FusedIterator;

use super::{ChunkRef, ContainerHeader, EntryIter, Reader};
use crate::{ChunkType, PrimaryHints};

/// A non-fatal MIRX container compliance issue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ComplianceFinding {
    /// Primary hints remain set even though no primary type is selected.
    StalePrimaryHints { hints: PrimaryHints },
    /// The selected primary type has no matching table entry.
    StalePrimaryType {
        chunk_type: ChunkType,
        hints: PrimaryHints,
    },
    /// A matching primary uses the legacy all-zero hint representation.
    LegacyZeroPrimaryHints { index: u16, chunk_type: ChunkType },
    /// Two non-empty payload ranges overlap in the source file.
    OverlappingPayloads {
        first_index: u16,
        second_index: u16,
        overlap_offset: u32,
        overlap_size: u32,
    },
}

#[derive(Clone, Copy, Debug)]
enum PrimaryCheck {
    Skip,
    Inspect {
        chunk_type: Option<ChunkType>,
        hints: PrimaryHints,
    },
}

/// Lazy, zero-allocation iterator over non-fatal container findings.
#[derive(Clone, Debug)]
pub struct FindingIter<'a> {
    primary_pending: bool,
    primary_check: PrimaryCheck,
    primary_entries: EntryIter<'a>,
    overlap_outer: EntryIter<'a>,
    overlap_inner: EntryIter<'a>,
    overlap_first: Option<ChunkRef<'a>>,
}

impl<'a> FindingIter<'a> {
    fn new(entries: EntryIter<'a>, primary_check: PrimaryCheck) -> Self {
        let primary_entries = entries.clone();
        let mut overlap_outer = entries;
        let overlap_first = overlap_outer.next();
        let overlap_inner = overlap_outer.clone();
        Self {
            primary_pending: true,
            primary_check,
            primary_entries,
            overlap_outer,
            overlap_inner,
            overlap_first,
        }
    }

    fn next_primary(&mut self) -> Option<ComplianceFinding> {
        if !self.primary_pending {
            return None;
        }
        self.primary_pending = false;

        let PrimaryCheck::Inspect { chunk_type, hints } = self.primary_check else {
            return None;
        };
        let Some(chunk_type) = chunk_type else {
            return (!hints.is_zero()).then_some(ComplianceFinding::StalePrimaryHints { hints });
        };

        match self
            .primary_entries
            .find(|entry| entry.chunk_type() == chunk_type)
        {
            None => Some(ComplianceFinding::StalePrimaryType { chunk_type, hints }),
            Some(entry) if hints.is_zero() => Some(ComplianceFinding::LegacyZeroPrimaryHints {
                index: u16::try_from(entry.index()).expect("validated MIRX chunk count"),
                chunk_type,
            }),
            Some(_) => None,
        }
    }

    fn next_overlap(&mut self) -> Option<ComplianceFinding> {
        loop {
            let first = self.overlap_first?;
            for second in self.overlap_inner.by_ref() {
                if let Some((overlap_offset, overlap_size)) = payload_overlap(first, second) {
                    return Some(ComplianceFinding::OverlappingPayloads {
                        first_index: u16::try_from(first.index())
                            .expect("validated MIRX chunk count"),
                        second_index: u16::try_from(second.index())
                            .expect("validated MIRX chunk count"),
                        overlap_offset,
                        overlap_size,
                    });
                }
            }

            self.overlap_first = self.overlap_outer.next();
            self.overlap_inner = self.overlap_outer.clone();
        }
    }
}

impl<'a> Iterator for FindingIter<'a> {
    type Item = ComplianceFinding;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_primary().or_else(|| self.next_overlap())
    }
}

impl FusedIterator for FindingIter<'_> {}

impl<'a> Reader<'a> {
    /// Streams non-fatal container compliance issues without allocating.
    pub fn compliance_findings(&self) -> FindingIter<'a> {
        let primary_check = match self.header {
            ContainerHeader::Chunk(header) if !self.has_future_semantics => PrimaryCheck::Inspect {
                chunk_type: ChunkType::new(header.primary_chunk_type),
                hints: self.primary_hints(),
            },
            ContainerHeader::Flat(_) | ContainerHeader::Chunk(_) => PrimaryCheck::Skip,
        };
        FindingIter::new(self.chunks(), primary_check)
    }
}

fn payload_overlap(first: ChunkRef<'_>, second: ChunkRef<'_>) -> Option<(u32, u32)> {
    let first_size = u32::try_from(first.payload().len()).expect("validated MIRX payload size");
    let second_size = u32::try_from(second.payload().len()).expect("validated MIRX payload size");
    if first_size == 0 || second_size == 0 {
        return None;
    }

    let first_end = first
        .payload_offset()
        .checked_add(first_size)
        .expect("validated MIRX payload range");
    let second_end = second
        .payload_offset()
        .checked_add(second_size)
        .expect("validated MIRX payload range");
    let overlap_offset = first.payload_offset().max(second.payload_offset());
    let overlap_end = first_end.min(second_end);
    if overlap_offset < overlap_end {
        Some((overlap_offset, overlap_end - overlap_offset))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::header::{CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, VERSION_MINOR, chunk_type};
    use crate::{ColorFormat, Layout, Reader, crc32, encode_chunks};

    fn refresh_crc(bytes: &mut [u8]) {
        let checksum = crc32(&bytes[..40]);
        bytes[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn set_primary(bytes: &mut [u8], chunk_type: u16, hints: PrimaryHints) {
        bytes[20..22].copy_from_slice(&chunk_type.to_le_bytes());
        bytes[22..24].copy_from_slice(&hints.sample_layout().raw().to_le_bytes());
        bytes[24..28].copy_from_slice(&hints.width().to_le_bytes());
        bytes[28..32].copy_from_slice(&hints.height().to_le_bytes());
        bytes[32..36].copy_from_slice(&hints.stride().to_le_bytes());
        refresh_crc(bytes);
    }

    fn set_payload_range(bytes: &mut [u8], index: usize, offset: u32, size: u32) {
        let entry = CHUNK_FILE_HEADER_LEN + index * CHUNK_TABLE_ENTRY_LEN;
        bytes[entry + 4..entry + 8].copy_from_slice(&offset.to_le_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&size.to_le_bytes());
    }

    fn payload_offset(bytes: &[u8], index: usize) -> u32 {
        let entry = CHUNK_FILE_HEADER_LEN + index * CHUNK_TABLE_ENTRY_LEN;
        u32::from_le_bytes(bytes[entry + 4..entry + 8].try_into().unwrap())
    }

    fn flat_a8() -> alloc::vec::Vec<u8> {
        let mut bytes = vec![0; crate::FLAT_HEADER_LEN + 1];
        bytes[..4].copy_from_slice(b"MIRX");
        bytes[4] = crate::VERSION_MAJOR;
        bytes[5] = VERSION_MINOR;
        bytes[6] = Layout::Flat.to_u8();
        bytes[8] = ColorFormat::A8.to_u8();
        bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        let checksum = crc32(&bytes[..24]);
        bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    #[test]
    fn flat_and_clean_no_primary_chunk_have_no_findings() {
        assert_eq!(
            Reader::open(&flat_a8())
                .unwrap()
                .compliance_findings()
                .next(),
            None
        );

        let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"meta")]);
        set_primary(&mut bytes, 0, PrimaryHints::ZERO);
        assert_eq!(
            Reader::open(&bytes).unwrap().compliance_findings().next(),
            None
        );
    }

    #[test]
    fn reports_stale_hints_and_stale_primary_type() {
        let hints = PrimaryHints::new(crate::image::SampleLayout::new(0xa5), 3, 4, 12);
        let mut stale_hints = encode_chunks(&[]);
        set_primary(&mut stale_hints, 0, hints);
        assert_eq!(
            Reader::open(&stale_hints)
                .unwrap()
                .compliance_findings()
                .collect::<alloc::vec::Vec<_>>(),
            vec![ComplianceFinding::StalePrimaryHints { hints }]
        );

        let mut stale_type = encode_chunks(&[(chunk_type::META, 0, b"meta")]);
        set_primary(&mut stale_type, chunk_type::VECTOR, hints);
        assert_eq!(
            Reader::open(&stale_type)
                .unwrap()
                .compliance_findings()
                .collect::<alloc::vec::Vec<_>>(),
            vec![ComplianceFinding::StalePrimaryType {
                chunk_type: ChunkType::VECTOR,
                hints,
            }]
        );
    }

    #[test]
    fn reports_legacy_zero_hints_for_the_first_matching_primary() {
        let mut bytes = encode_chunks(&[
            (chunk_type::META, 0, b"meta"),
            (chunk_type::FONT, 0, b"first"),
            (chunk_type::FONT, 0, b"second"),
        ]);
        set_primary(&mut bytes, chunk_type::FONT, PrimaryHints::ZERO);
        assert_eq!(
            Reader::open(&bytes).unwrap().compliance_findings().next(),
            Some(ComplianceFinding::LegacyZeroPrimaryHints {
                index: 1,
                chunk_type: ChunkType::FONT,
            })
        );

        set_primary(
            &mut bytes,
            chunk_type::FONT,
            PrimaryHints::new(crate::image::SampleLayout::NONE, 0, 0, 0),
        );
        assert_eq!(
            Reader::open(&bytes).unwrap().compliance_findings().next(),
            None
        );
    }

    #[test]
    fn reports_primary_then_each_non_empty_overlap_once_in_table_order() {
        let mut bytes = encode_chunks(&[
            (chunk_type::META, 0, b"abcdefgh"),
            (chunk_type::FONT, 0, b"ijklmnop"),
            (chunk_type::VECTOR, 0, b"qrstuvwx"),
        ]);
        let start = payload_offset(&bytes, 0);
        set_payload_range(&mut bytes, 1, start + 2, 5);
        set_payload_range(&mut bytes, 2, start + 4, 4);

        assert_eq!(
            Reader::open(&bytes)
                .unwrap()
                .compliance_findings()
                .collect::<alloc::vec::Vec<_>>(),
            vec![
                ComplianceFinding::LegacyZeroPrimaryHints {
                    index: 0,
                    chunk_type: ChunkType::META,
                },
                ComplianceFinding::OverlappingPayloads {
                    first_index: 0,
                    second_index: 1,
                    overlap_offset: start + 2,
                    overlap_size: 5,
                },
                ComplianceFinding::OverlappingPayloads {
                    first_index: 0,
                    second_index: 2,
                    overlap_offset: start + 4,
                    overlap_size: 4,
                },
                ComplianceFinding::OverlappingPayloads {
                    first_index: 1,
                    second_index: 2,
                    overlap_offset: start + 4,
                    overlap_size: 3,
                },
            ]
        );
    }

    #[test]
    fn ignores_empty_and_adjacent_payload_ranges() {
        let mut bytes = encode_chunks(&[
            (chunk_type::META, 0, b"abcd"),
            (chunk_type::FONT, 0, b"efgh"),
            (chunk_type::VECTOR, 0, b""),
        ]);
        set_primary(&mut bytes, 0, PrimaryHints::ZERO);
        let start = payload_offset(&bytes, 0);
        set_payload_range(&mut bytes, 1, start + 4, 4);
        set_payload_range(&mut bytes, 2, start + 1, 0);
        assert_eq!(
            Reader::open(&bytes).unwrap().compliance_findings().next(),
            None
        );
    }

    #[test]
    fn future_headers_suppress_primary_findings_but_keep_objective_overlaps() {
        for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
            let mut bytes = encode_chunks(&[
                (chunk_type::META, 0, b"abcdef"),
                (chunk_type::FONT, 0, b"uvwxyz"),
            ]);
            bytes[5] = minor;
            bytes[7] = flags;
            set_primary(
                &mut bytes,
                chunk_type::VECTOR,
                PrimaryHints::new(crate::image::SampleLayout::new(0xa5), 3, 4, 12),
            );
            let start = payload_offset(&bytes, 0);
            set_payload_range(&mut bytes, 1, start + 2, 3);

            assert_eq!(
                Reader::open(&bytes)
                    .unwrap()
                    .compliance_findings()
                    .collect::<alloc::vec::Vec<_>>(),
                vec![ComplianceFinding::OverlappingPayloads {
                    first_index: 0,
                    second_index: 1,
                    overlap_offset: start + 2,
                    overlap_size: 3,
                }]
            );
        }
    }

    #[test]
    fn iterator_clone_and_fused_state_are_independent() {
        let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"abcd")]);
        set_primary(&mut bytes, chunk_type::META, PrimaryHints::ZERO);
        let reader = Reader::open(&bytes).unwrap();
        let mut findings = reader.compliance_findings();
        let mut clone = findings.clone();
        assert_eq!(findings.next(), clone.next());
        assert_eq!(findings.next(), None);
        assert_eq!(findings.next(), None);
        assert_eq!(clone.next(), None);
    }
}
