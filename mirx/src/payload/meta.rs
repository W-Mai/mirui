use core::{iter::FusedIterator, str};

use crate::payload::envelope::{Envelope, EnvelopeError, ExactEnvelope, checked_payload_len};
use crate::reader::PayloadLimits;
use crate::wire::{read_u16_le, read_u32_le};

#[path = "meta/owned.rs"]
mod owned;

pub use owned::{Meta, MetaEncodeError, MetaEntry, MetaMutationError, MetaValue};

const HEADER_LEN: usize = 4;
const ENTRY_HEADER_LEN: usize = 8;

/// Failure while validating or opening a META payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MetaDecodeError {
    Truncated {
        needed: usize,
        available: usize,
    },
    UnsupportedVersion(u8),
    UnknownFlags(u8),
    TooManyEntries {
        count: u32,
        limit: u32,
    },
    /// An entry declares bytes beyond the CRC-covered payload prefix.
    /// `available` excludes the four-byte CRC trailer.
    EntryOutOfBounds {
        index: u16,
        declared_end: u64,
        available: usize,
    },
    EmptyKey {
        index: u16,
    },
    MetaBytesLimitExceeded {
        needed: usize,
        limit: usize,
    },
    InvalidKeyUtf8 {
        index: u16,
    },
    KeyContainsNul {
        index: u16,
    },
    InvalidTextUtf8 {
        index: u16,
    },
    PayloadLengthMismatch {
        expected: usize,
        actual: usize,
    },
    CrcMismatch {
        expected: u32,
        actual: u32,
    },
    DecodedBytesLimitExceeded {
        needed: usize,
        limit: usize,
    },
    AllocationFailed,
    SizeOverflow,
}

/// One borrowed META value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetaValueRef<'a> {
    Text(&'a str),
    Bytes(&'a [u8]),
    Extension {
        kind: u8,
        flags: u8,
        bytes: &'a [u8],
    },
}

/// One borrowed META entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetaEntryRef<'a> {
    pub key: &'a str,
    pub value: MetaValueRef<'a>,
}

/// Zero-allocation view over one validated ordered META payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetaView<'a> {
    entries: &'a [u8],
    entry_count: u16,
    meta_bytes: usize,
}

impl<'a> MetaView<'a> {
    /// Opens and validates one complete META payload without allocating.
    pub fn open_payload(
        payload: &'a [u8],
        limits: &PayloadLimits,
    ) -> Result<Self, MetaDecodeError> {
        validate_payload_layout(payload, limits)?.validate_crc()
    }

    /// Returns the number of entries in wire order.
    pub const fn len(&self) -> usize {
        self.entry_count as usize
    }

    /// Returns whether this META payload contains no entries.
    pub const fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// Iterates over all entries in wire order without allocating.
    pub const fn entries(&self) -> MetaEntryIter<'a> {
        MetaEntryIter {
            remaining: self.entries,
            remaining_entries: self.entry_count,
        }
    }

    /// Returns the first entry whose key is an exact, case-sensitive match.
    pub fn get_first(&self, key: &str) -> Option<MetaEntryRef<'a>> {
        self.entries().find(|entry| entry.key == key)
    }

    /// Returns the last entry whose key is an exact, case-sensitive match.
    pub fn get_last(&self, key: &str) -> Option<MetaEntryRef<'a>> {
        self.entries().filter(|entry| entry.key == key).last()
    }

    /// Iterates over every exact key match in wire order without allocating.
    pub fn get_all<'b>(
        &'b self,
        key: &'b str,
    ) -> impl Clone + FusedIterator<Item = MetaEntryRef<'a>> + 'b
    where
        'a: 'b,
    {
        self.entries().filter(move |entry| entry.key == key)
    }
}

pub(crate) struct ValidatedMetaLayout<'a> {
    exact: ExactEnvelope<'a>,
    entries: &'a [u8],
    entry_count: u16,
    meta_bytes: usize,
}

impl<'a> ValidatedMetaLayout<'a> {
    pub(crate) const fn entry_count(&self) -> u16 {
        self.entry_count
    }

    pub(crate) const fn meta_bytes(&self) -> usize {
        self.meta_bytes
    }

    pub(crate) fn validate_crc(self) -> Result<MetaView<'a>, MetaDecodeError> {
        self.exact.validate_crc().map_err(map_envelope_error)?;
        Ok(MetaView {
            entries: self.entries,
            entry_count: self.entry_count,
            meta_bytes: self.meta_bytes,
        })
    }
}

pub(crate) fn validate_payload_layout<'a>(
    payload: &'a [u8],
    limits: &PayloadLimits,
) -> Result<ValidatedMetaLayout<'a>, MetaDecodeError> {
    let envelope = Envelope::open_v1(payload, HEADER_LEN).map_err(map_envelope_error)?;
    let covered = envelope.covered();
    let flags = covered[1];
    if flags != 0 {
        return Err(MetaDecodeError::UnknownFlags(flags));
    }

    let entry_count = read_u16_le(covered, 2).ok_or(MetaDecodeError::SizeOverflow)?;
    let count = u32::from(entry_count);
    let minimum_entries_len = usize::from(entry_count)
        .checked_mul(ENTRY_HEADER_LEN)
        .ok_or(MetaDecodeError::SizeOverflow)?;
    let minimum_covered_len = HEADER_LEN
        .checked_add(minimum_entries_len)
        .ok_or(MetaDecodeError::SizeOverflow)?;
    let minimum_payload_len =
        checked_payload_len(minimum_covered_len).map_err(map_envelope_error)?;
    if payload.len() < minimum_payload_len {
        return Err(MetaDecodeError::Truncated {
            needed: minimum_payload_len,
            available: payload.len(),
        });
    }

    let limit = limits.max_meta_entries();
    if count > limit {
        return Err(MetaDecodeError::TooManyEntries { count, limit });
    }

    let mut offset = HEADER_LEN;
    let mut meta_bytes = 0usize;
    for index in 0..entry_count {
        let scanned = scan_entry(covered, offset, index, meta_bytes, limits)?;
        offset = scanned.end;
        meta_bytes = scanned.meta_bytes;
    }

    let exact = envelope
        .validate_exact_end(offset)
        .map_err(map_envelope_error)?;

    let entries = covered
        .get(HEADER_LEN..offset)
        .ok_or(MetaDecodeError::SizeOverflow)?;
    Ok(ValidatedMetaLayout {
        exact,
        entries,
        entry_count,
        meta_bytes,
    })
}

impl<'a> IntoIterator for MetaView<'a> {
    type Item = MetaEntryRef<'a>;
    type IntoIter = MetaEntryIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries()
    }
}

/// Iterator over validated borrowed META entries.
#[derive(Clone, Debug)]
pub struct MetaEntryIter<'a> {
    remaining: &'a [u8],
    remaining_entries: u16,
}

impl<'a> Iterator for MetaEntryIter<'a> {
    type Item = MetaEntryRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_entries == 0 {
            return None;
        }
        let (entry, remaining) = decode_validated_entry(self.remaining)
            .expect("MetaView::open_payload validated every META entry");
        self.remaining = remaining;
        self.remaining_entries -= 1;
        Some(entry)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::from(self.remaining_entries);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for MetaEntryIter<'_> {}
impl FusedIterator for MetaEntryIter<'_> {}

struct ScannedEntry {
    end: usize,
    meta_bytes: usize,
}

fn scan_entry(
    covered: &[u8],
    offset: usize,
    index: u16,
    current_meta_bytes: usize,
    limits: &PayloadLimits,
) -> Result<ScannedEntry, MetaDecodeError> {
    let fixed_end = offset
        .checked_add(ENTRY_HEADER_LEN)
        .ok_or(MetaDecodeError::SizeOverflow)?;
    checked_payload_len(fixed_end).map_err(map_envelope_error)?;
    if fixed_end > covered.len() {
        return Err(MetaDecodeError::EntryOutOfBounds {
            index,
            declared_end: fixed_end as u64,
            available: covered.len(),
        });
    }

    let key_len = usize::from(read_u16_le(covered, offset).ok_or(MetaDecodeError::SizeOverflow)?);
    if key_len == 0 {
        return Err(MetaDecodeError::EmptyKey { index });
    }
    let kind = covered[offset + 2];
    let flags = covered[offset + 3];
    let value_len =
        u64::from(read_u32_le(covered, offset + 4).ok_or(MetaDecodeError::SizeOverflow)?);

    let declared_end = (fixed_end as u64)
        .checked_add(key_len as u64)
        .and_then(|end| end.checked_add(value_len))
        .ok_or(MetaDecodeError::SizeOverflow)?;
    let end = usize::try_from(declared_end).map_err(|_| MetaDecodeError::SizeOverflow)?;
    checked_payload_len(end).map_err(map_envelope_error)?;
    if end > covered.len() {
        return Err(MetaDecodeError::EntryOutOfBounds {
            index,
            declared_end,
            available: covered.len(),
        });
    }
    let value_len = usize::try_from(value_len).map_err(|_| MetaDecodeError::SizeOverflow)?;

    let entry_meta_bytes = key_len
        .checked_add(value_len)
        .ok_or(MetaDecodeError::SizeOverflow)?;
    let meta_bytes = current_meta_bytes
        .checked_add(entry_meta_bytes)
        .ok_or(MetaDecodeError::SizeOverflow)?;
    let limit = limits.max_meta_bytes();
    if meta_bytes > limit {
        return Err(MetaDecodeError::MetaBytesLimitExceeded {
            needed: meta_bytes,
            limit,
        });
    }

    let key_end = fixed_end
        .checked_add(key_len)
        .ok_or(MetaDecodeError::SizeOverflow)?;
    let key_bytes = &covered[fixed_end..key_end];
    str::from_utf8(key_bytes).map_err(|_| MetaDecodeError::InvalidKeyUtf8 { index })?;
    if key_bytes.contains(&0) {
        return Err(MetaDecodeError::KeyContainsNul { index });
    }

    if kind == 0 && flags == 0 {
        str::from_utf8(&covered[key_end..end])
            .map_err(|_| MetaDecodeError::InvalidTextUtf8 { index })?;
    }

    Ok(ScannedEntry { end, meta_bytes })
}

fn decode_validated_entry(remaining: &[u8]) -> Option<(MetaEntryRef<'_>, &[u8])> {
    let key_len = usize::from(read_u16_le(remaining, 0)?);
    let kind = *remaining.get(2)?;
    let flags = *remaining.get(3)?;
    let value_len = usize::try_from(read_u32_le(remaining, 4)?).ok()?;
    let key_end = ENTRY_HEADER_LEN.checked_add(key_len)?;
    let entry_end = key_end.checked_add(value_len)?;
    let key = str::from_utf8(remaining.get(ENTRY_HEADER_LEN..key_end)?).ok()?;
    let bytes = remaining.get(key_end..entry_end)?;
    let value = match (kind, flags) {
        (0, 0) => MetaValueRef::Text(str::from_utf8(bytes).ok()?),
        (1, 0) => MetaValueRef::Bytes(bytes),
        _ => MetaValueRef::Extension { kind, flags, bytes },
    };
    Some((MetaEntryRef { key, value }, remaining.get(entry_end..)?))
}

fn map_envelope_error(error: EnvelopeError) -> MetaDecodeError {
    match error {
        EnvelopeError::Truncated { needed, available } => {
            MetaDecodeError::Truncated { needed, available }
        }
        EnvelopeError::UnsupportedVersion(version) => MetaDecodeError::UnsupportedVersion(version),
        EnvelopeError::PayloadLengthMismatch { expected, actual } => {
            MetaDecodeError::PayloadLengthMismatch { expected, actual }
        }
        EnvelopeError::CrcMismatch { expected, actual } => {
            MetaDecodeError::CrcMismatch { expected, actual }
        }
        EnvelopeError::SizeOverflow => MetaDecodeError::SizeOverflow,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use core::mem::{needs_drop, size_of};

    use super::*;
    use crate::{ChunkType, Reader, crc32, encode_chunks};

    const EMPTY: [u8; 8] = [0x01, 0x00, 0x00, 0x00, 0x79, 0xb8, 0xf8, 0x99];
    const TEXT: [u8; 18] = [
        0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x6b, 0x76, 0x20,
        0x0a, 0xe0, 0xcc,
    ];
    const BYTES: [u8; 19] = [
        0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x62, 0x00, 0xff,
        0xe3, 0x2b, 0x85, 0x94,
    ];
    const EXTENSION: [u8; 21] = [
        0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x80, 0xa5, 0x04, 0x00, 0x00, 0x00, 0x78, 0xde, 0xad,
        0xbe, 0xef, 0xdd, 0x4a, 0x22, 0xdb,
    ];

    #[derive(Clone, Copy)]
    struct RawEntry<'a> {
        key: &'a [u8],
        kind: u8,
        flags: u8,
        value: &'a [u8],
    }

    fn payload(entries: &[RawEntry<'_>]) -> Vec<u8> {
        let count = u16::try_from(entries.len()).unwrap();
        let mut covered = Vec::new();
        covered.extend_from_slice(&[1, 0]);
        covered.extend_from_slice(&count.to_le_bytes());
        for entry in entries {
            covered.extend_from_slice(&u16::try_from(entry.key.len()).unwrap().to_le_bytes());
            covered.extend_from_slice(&[entry.kind, entry.flags]);
            covered.extend_from_slice(&u32::try_from(entry.value.len()).unwrap().to_le_bytes());
            covered.extend_from_slice(entry.key);
            covered.extend_from_slice(entry.value);
        }
        seal(covered)
    }

    fn seal(mut covered: Vec<u8>) -> Vec<u8> {
        covered.extend_from_slice(&crc32(&covered).to_le_bytes());
        covered
    }

    fn text<'a>(key: &'a [u8], value: &'a [u8]) -> RawEntry<'a> {
        RawEntry {
            key,
            kind: 0,
            flags: 0,
            value,
        }
    }

    fn bytes<'a>(key: &'a [u8], value: &'a [u8]) -> RawEntry<'a> {
        RawEntry {
            key,
            kind: 1,
            flags: 0,
            value,
        }
    }

    fn assert_borrowed_from(payload: &[u8], bytes: &[u8]) {
        let payload_start = payload.as_ptr() as usize;
        let payload_end = payload_start + payload.len();
        let borrowed_start = bytes.as_ptr() as usize;
        let borrowed_end = borrowed_start + bytes.len();
        assert!(payload_start <= borrowed_start);
        assert!(borrowed_end <= payload_end);
    }

    #[test]
    fn independent_literals_cover_empty_text_bytes_and_extensions() {
        let zero = PayloadLimits::EMBEDDED
            .with_max_meta_entries(0)
            .with_max_meta_bytes(0)
            .with_max_decoded_bytes(0);
        let empty = MetaView::open_payload(&EMPTY, &zero).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.meta_bytes, 0);

        let text = MetaView::open_payload(&TEXT, &PayloadLimits::HOST).unwrap();
        assert_eq!(
            text.entries().next(),
            Some(MetaEntryRef {
                key: "k",
                value: MetaValueRef::Text("v"),
            })
        );
        assert_eq!(text.meta_bytes, 2);

        let bytes = MetaView::open_payload(&BYTES, &PayloadLimits::HOST).unwrap();
        assert_eq!(
            bytes.entries().next(),
            Some(MetaEntryRef {
                key: "b",
                value: MetaValueRef::Bytes(&[0x00, 0xff]),
            })
        );

        let extension = MetaView::open_payload(&EXTENSION, &PayloadLimits::HOST).unwrap();
        assert_eq!(
            extension.entries().next(),
            Some(MetaEntryRef {
                key: "x",
                value: MetaValueRef::Extension {
                    kind: 0x80,
                    flags: 0xa5,
                    bytes: &[0xde, 0xad, 0xbe, 0xef],
                },
            })
        );
    }

    #[test]
    fn values_borrow_the_payload_and_all_noncanonical_pairs_are_extensions() {
        let payload = payload(&[
            text(b"name", b"dashboard"),
            bytes(b"blob", &[0x00, 0xff]),
            RawEntry {
                key: b"reserved",
                kind: 0x02,
                flags: 0,
                value: &[0xff],
            },
            RawEntry {
                key: b"flagged-text",
                kind: 0,
                flags: 1,
                value: &[0xff],
            },
            RawEntry {
                key: b"flagged-bytes",
                kind: 1,
                flags: 0x80,
                value: &[0xfe],
            },
        ]);
        let view = MetaView::open_payload(&payload, &PayloadLimits::HOST).unwrap();
        let entries: Vec<_> = view.entries().collect();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].value, MetaValueRef::Text("dashboard"));
        assert_eq!(entries[1].value, MetaValueRef::Bytes(&[0x00, 0xff]));
        assert_eq!(
            entries[2].value,
            MetaValueRef::Extension {
                kind: 0x02,
                flags: 0,
                bytes: &[0xff],
            }
        );
        assert_eq!(
            entries[3].value,
            MetaValueRef::Extension {
                kind: 0,
                flags: 1,
                bytes: &[0xff],
            }
        );
        assert_eq!(
            entries[4].value,
            MetaValueRef::Extension {
                kind: 1,
                flags: 0x80,
                bytes: &[0xfe],
            }
        );

        for entry in entries {
            assert_borrowed_from(&payload, entry.key.as_bytes());
            let value = match entry.value {
                MetaValueRef::Text(value) => value.as_bytes(),
                MetaValueRef::Bytes(value) | MetaValueRef::Extension { bytes: value, .. } => value,
            };
            assert_borrowed_from(&payload, value);
        }
    }

    #[test]
    fn duplicate_queries_preserve_wire_order_and_match_exactly() {
        let payload = payload(&[
            text(b"tag", b"first"),
            text(b"Tag", b"case"),
            bytes(b"tag", b"second"),
            text(b"tag", b"last"),
            text("é".as_bytes(), b"composed"),
            text("e\u{301}".as_bytes(), b"decomposed"),
        ]);
        let view = MetaView::open_payload(&payload, &PayloadLimits::HOST).unwrap();

        assert_eq!(
            view.get_first("tag").unwrap().value,
            MetaValueRef::Text("first")
        );
        assert_eq!(
            view.get_last("tag").unwrap().value,
            MetaValueRef::Text("last")
        );
        assert_eq!(
            view.get_first("Tag").unwrap().value,
            MetaValueRef::Text("case")
        );
        assert_eq!(view.get_first("TAG"), None);
        assert_eq!(
            view.get_first("é").unwrap().value,
            MetaValueRef::Text("composed")
        );
        assert_eq!(
            view.get_first("e\u{301}").unwrap().value,
            MetaValueRef::Text("decomposed")
        );
        assert_eq!(
            view.get_all("tag")
                .map(|entry| entry.value)
                .collect::<Vec<_>>(),
            [
                MetaValueRef::Text("first"),
                MetaValueRef::Bytes(b"second"),
                MetaValueRef::Text("last"),
            ]
        );
    }

    #[test]
    fn entry_iterator_is_cloneable_exact_sized_fused_and_compact() {
        fn assert_exact<I: ExactSizeIterator>(_iter: &I) {}
        fn assert_fused<I: FusedIterator>(_iter: &I) {}

        let payload = payload(&[text(b"a", b"1"), text(b"b", b"2")]);
        let view = MetaView::open_payload(&payload, &PayloadLimits::HOST).unwrap();
        let mut first = view.entries();
        let mut clone = first.clone();
        assert_exact(&first);
        assert_fused(&first);
        assert_eq!(first.len(), 2);
        assert_eq!(first.size_hint(), (2, Some(2)));
        assert_eq!(first.next().unwrap().key, "a");
        assert_eq!(first.len(), 1);
        assert_eq!(clone.nth(1).unwrap().key, "b");
        assert_eq!(clone.len(), 0);
        assert_eq!(clone.next(), None);
        assert_eq!(clone.next(), None);
        assert_eq!(view.into_iter().count(), 2);

        assert!(!needs_drop::<MetaView<'_>>());
        assert!(size_of::<MetaView<'_>>() <= 32);
        assert!(!needs_drop::<MetaEntryIter<'_>>());
        assert!(size_of::<MetaEntryIter<'_>>() <= 24);
    }

    #[test]
    fn version_flags_exact_end_and_crc_have_stable_priority() {
        assert_eq!(
            MetaView::open_payload(&[], &PayloadLimits::HOST),
            Err(MetaDecodeError::Truncated {
                needed: 1,
                available: 0,
            })
        );
        assert_eq!(
            MetaView::open_payload(&[2], &PayloadLimits::HOST),
            Err(MetaDecodeError::UnsupportedVersion(2))
        );
        assert_eq!(
            MetaView::open_payload(&[1], &PayloadLimits::HOST),
            Err(MetaDecodeError::Truncated {
                needed: 8,
                available: 1,
            })
        );

        let flags = seal(alloc::vec![1, 0xa5, 0xff, 0xff]);
        assert_eq!(
            MetaView::open_payload(&flags, &PayloadLimits::HOST),
            Err(MetaDecodeError::UnknownFlags(0xa5))
        );

        let mut trailing_covered = EMPTY[..HEADER_LEN].to_vec();
        trailing_covered.push(0xaa);
        let trailing = seal(trailing_covered);
        assert_eq!(
            MetaView::open_payload(&trailing, &PayloadLimits::HOST),
            Err(MetaDecodeError::PayloadLengthMismatch {
                expected: EMPTY.len(),
                actual: EMPTY.len() + 1,
            })
        );

        let mut bad_crc = EMPTY;
        bad_crc[7] ^= 0x80;
        assert!(matches!(
            MetaView::open_payload(&bad_crc, &PayloadLimits::HOST),
            Err(MetaDecodeError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn declared_count_lower_bound_precedes_the_entry_limit() {
        let one_without_entry = seal(alloc::vec![1, 0, 1, 0]);
        let zero_limit = PayloadLimits::HOST.with_max_meta_entries(0);
        assert_eq!(
            MetaView::open_payload(&one_without_entry, &zero_limit),
            Err(MetaDecodeError::Truncated {
                needed: 16,
                available: 8,
            })
        );

        let maximum_without_entries = seal(alloc::vec![1, 0, 0xff, 0xff]);
        assert_eq!(
            MetaView::open_payload(&maximum_without_entries, &zero_limit),
            Err(MetaDecodeError::Truncated {
                needed: 524_288,
                available: 8,
            })
        );

        let two = payload(&[text(b"a", b""), text(b"b", b"")]);
        assert_eq!(
            MetaView::open_payload(&two, &PayloadLimits::HOST.with_max_meta_entries(1)),
            Err(MetaDecodeError::TooManyEntries { count: 2, limit: 1 })
        );
    }

    #[test]
    fn entry_ranges_check_the_outer_wire_size_before_physical_bounds() {
        fn declared_value_payload(value_len: u32) -> Vec<u8> {
            let mut covered = alloc::vec![1, 0, 1, 0, 1, 0, 1, 0];
            covered.extend_from_slice(&value_len.to_le_bytes());
            covered.push(b'k');
            seal(covered)
        }

        let missing_value = declared_value_payload(1);
        assert_eq!(
            MetaView::open_payload(&missing_value, &PayloadLimits::HOST),
            Err(MetaDecodeError::EntryOutOfBounds {
                index: 0,
                declared_end: 14,
                available: 13,
            })
        );

        let largest_wire_value = declared_value_payload(0xffff_ffee);
        assert_eq!(
            MetaView::open_payload(&largest_wire_value, &PayloadLimits::HOST),
            Err(MetaDecodeError::EntryOutOfBounds {
                index: 0,
                declared_end: u64::from(u32::MAX) - 4,
                available: 13,
            })
        );
        assert_eq!(
            MetaView::open_payload(&declared_value_payload(0xffff_ffef), &PayloadLimits::HOST),
            Err(MetaDecodeError::SizeOverflow)
        );
    }

    #[test]
    fn keys_and_known_text_enforce_their_string_contracts() {
        assert_eq!(
            MetaView::open_payload(&payload(&[text(b"", b"value")]), &PayloadLimits::HOST,),
            Err(MetaDecodeError::EmptyKey { index: 0 })
        );
        assert_eq!(
            MetaView::open_payload(&payload(&[text(&[0xff], b"value")]), &PayloadLimits::HOST,),
            Err(MetaDecodeError::InvalidKeyUtf8 { index: 0 })
        );
        assert_eq!(
            MetaView::open_payload(
                &payload(&[text(b"bad\0key", b"value")]),
                &PayloadLimits::HOST,
            ),
            Err(MetaDecodeError::KeyContainsNul { index: 0 })
        );
        assert_eq!(
            MetaView::open_payload(&payload(&[text(b"key", &[0xff])]), &PayloadLimits::HOST,),
            Err(MetaDecodeError::InvalidTextUtf8 { index: 0 })
        );

        let text_nul = payload(&[text(b"key", b"a\0b")]);
        assert_eq!(
            MetaView::open_payload(&text_nul, &PayloadLimits::HOST)
                .unwrap()
                .get_first("key")
                .unwrap()
                .value,
            MetaValueRef::Text("a\0b")
        );
    }

    #[test]
    fn aggregate_meta_bytes_include_every_key_and_value_but_not_headers() {
        let aggregate_payload = payload(&[
            text(b"a", b"12"),
            RawEntry {
                key: b"bc",
                kind: 0x80,
                flags: 1,
                value: b"345",
            },
        ]);
        let exact = PayloadLimits::HOST
            .with_max_meta_entries(2)
            .with_max_meta_bytes(8)
            .with_max_decoded_bytes(0);
        let view = MetaView::open_payload(&aggregate_payload, &exact).unwrap();
        assert_eq!(view.meta_bytes, 8);
        assert_eq!(
            MetaView::open_payload(&aggregate_payload, &exact.with_max_meta_bytes(7)),
            Err(MetaDecodeError::MetaBytesLimitExceeded {
                needed: 8,
                limit: 7,
            })
        );

        let invalid_key = payload(&[text(&[0xff], b"v")]);
        assert_eq!(
            MetaView::open_payload(&invalid_key, &PayloadLimits::HOST.with_max_meta_bytes(1),),
            Err(MetaDecodeError::MetaBytesLimitExceeded {
                needed: 2,
                limit: 1,
            })
        );
    }

    #[test]
    fn chunk_accessor_is_type_gated_and_retains_source_pointers() {
        let bytes = encode_chunks(&[
            (crate::header::chunk_type::META, 0, TEXT.as_slice()),
            (crate::header::chunk_type::FONT, 0, b"opaque"),
        ]);
        let reader = Reader::open(&bytes).unwrap();
        let mut chunks = reader.chunks();
        let meta_chunk = chunks.next().unwrap();
        let view = meta_chunk.meta(&PayloadLimits::HOST).unwrap().unwrap();
        let entry = view.entries().next().unwrap();
        assert_eq!(meta_chunk.chunk_type(), ChunkType::META);
        assert_borrowed_from(meta_chunk.payload(), entry.key.as_bytes());
        assert_borrowed_from(
            meta_chunk.payload(),
            match entry.value {
                MetaValueRef::Text(value) => value.as_bytes(),
                _ => unreachable!(),
            },
        );

        let font_chunk = chunks.next().unwrap();
        assert_eq!(font_chunk.meta(&PayloadLimits::HOST), Ok(None));
    }
}
