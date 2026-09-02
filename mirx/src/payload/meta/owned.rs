use alloc::{string::String, vec::Vec};
use core::mem::{self, size_of};

use super::{
    HEADER_LEN, MetaDecodeError, MetaEntryRef, MetaValueRef, MetaView, validate_payload_layout,
};
use crate::payload::envelope::{CRC_TRAILER_LEN, VERSION, checked_payload_len, write_crc_trailer};
use crate::reader::PayloadLimits;

/// One editable ordered META value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetaValue {
    Text(String),
    Bytes(Vec<u8>),
    Extension { kind: u8, flags: u8, bytes: Vec<u8> },
}

/// One editable META entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetaEntry {
    pub key: String,
    pub value: MetaValue,
}

impl MetaEntry {
    pub fn new(key: impl Into<String>, value: MetaValue) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }

    pub fn text(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: MetaValue::Text(value.into()),
        }
    }

    pub fn bytes(key: impl Into<String>, value: impl Into<Vec<u8>>) -> Self {
        Self {
            key: key.into(),
            value: MetaValue::Bytes(value.into()),
        }
    }

    pub fn extension(
        key: impl Into<String>,
        kind: u8,
        flags: u8,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            key: key.into(),
            value: MetaValue::Extension {
                kind,
                flags,
                bytes: bytes.into(),
            },
        }
    }
}

/// Editable ordered META multimap.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Meta {
    pub entries: Vec<MetaEntry>,
}

/// Failure from a positional in-memory META mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MetaMutationError {
    IndexOutOfBounds { index: usize, len: usize },
    AllocationFailed,
}

/// Failure while validating or encoding an editable META value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MetaEncodeError {
    InvalidPayload(MetaDecodeError),
    EntryCountOverflow { actual: usize },
    KeyLengthOverflow { index: usize, actual: usize },
    ValueLengthOverflow { index: usize, actual: usize },
    KnownValueKindAsExtension { index: usize, kind: u8 },
    BufferTooSmall { needed: usize, available: usize },
    AllocationFailed,
}

impl From<MetaDecodeError> for MetaEncodeError {
    fn from(value: MetaDecodeError) -> Self {
        Self::InvalidPayload(value)
    }
}

impl Meta {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn from_entries(entries: Vec<MetaEntry>) -> Self {
        Self { entries }
    }

    /// Decodes one complete META payload with bounded, fallible allocation.
    ///
    /// Structural, resource, exact-end, and CRC validation complete before
    /// owned entry, key, or value storage is reserved.
    pub fn decode_with_limits(
        payload: &[u8],
        limits: &PayloadLimits,
    ) -> Result<Self, MetaDecodeError> {
        decode_payload_with_allocator(payload, limits, &mut CheckedDecodeAllocator)
    }

    pub(crate) fn decode_view_with_limits(
        view: MetaView<'_>,
        limits: &PayloadLimits,
    ) -> Result<Self, MetaDecodeError> {
        validate_decoded_budget(view.len(), view.meta_bytes, limits)?;
        decode_view_with_allocator(view, &mut CheckedDecodeAllocator)
    }

    /// Appends an entry after reserving its slot fallibly.
    pub fn push(&mut self, entry: MetaEntry) -> Result<(), MetaMutationError> {
        self.insert(self.entries.len(), entry)
    }

    /// Inserts an entry at an exact position, including `len()` for append.
    pub fn insert(&mut self, index: usize, entry: MetaEntry) -> Result<(), MetaMutationError> {
        self.insert_with(index, entry, |entries| {
            entries
                .try_reserve(1)
                .map_err(|_| MetaMutationError::AllocationFailed)
        })
    }

    /// Replaces one entry and returns the previous value.
    pub fn replace(
        &mut self,
        index: usize,
        entry: MetaEntry,
    ) -> Result<MetaEntry, MetaMutationError> {
        let len = self.entries.len();
        let current = self
            .entries
            .get_mut(index)
            .ok_or(MetaMutationError::IndexOutOfBounds { index, len })?;
        Ok(mem::replace(current, entry))
    }

    /// Removes and returns one entry at an exact position.
    pub fn remove(&mut self, index: usize) -> Result<MetaEntry, MetaMutationError> {
        let len = self.entries.len();
        if index >= len {
            return Err(MetaMutationError::IndexOutOfBounds { index, len });
        }
        Ok(self.entries.remove(index))
    }

    /// Removes every exact key match while retaining all other wire order.
    pub fn remove_all(&mut self, key: &str) -> usize {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.key != key);
        before - self.entries.len()
    }

    /// Returns the exact size of this value's canonical META payload.
    pub fn encoded_payload_len(&self) -> Result<usize, MetaEncodeError> {
        Ok(self.payload_plan()?.encoded_len())
    }

    /// Encodes a canonical META payload into the start of `out`.
    ///
    /// Complete validation precedes the capacity check. Errors leave all of
    /// `out` unchanged, and success preserves its unused suffix.
    pub fn encode_payload_into(&self, out: &mut [u8]) -> Result<usize, MetaEncodeError> {
        self.payload_plan()?.copy_payload_into(out)
    }

    /// Allocates and encodes one exact-length canonical META payload.
    pub fn encode_payload(&self) -> Result<Vec<u8>, MetaEncodeError> {
        self.payload_plan()?.payload_to_vec()
    }

    pub(crate) fn payload_plan(&self) -> Result<MetaPayloadPlan<'_>, MetaEncodeError> {
        MetaPayloadPlan::new(self)
    }

    fn insert_with(
        &mut self,
        index: usize,
        entry: MetaEntry,
        reserve: impl FnOnce(&mut Vec<MetaEntry>) -> Result<(), MetaMutationError>,
    ) -> Result<(), MetaMutationError> {
        let len = self.entries.len();
        if index > len {
            return Err(MetaMutationError::IndexOutOfBounds { index, len });
        }
        reserve(&mut self.entries)?;
        self.entries.insert(index, entry);
        Ok(())
    }

    #[cfg(test)]
    fn encode_payload_with(
        &self,
        reserve: impl FnOnce(&mut Vec<u8>, usize) -> Result<(), MetaEncodeError>,
    ) -> Result<Vec<u8>, MetaEncodeError> {
        self.payload_plan()?.payload_to_vec_with(reserve)
    }
}

trait DecodeAllocator {
    fn reserve_entries(
        &mut self,
        entries: &mut Vec<MetaEntry>,
        count: usize,
    ) -> Result<(), MetaDecodeError>;

    fn copy_string(&mut self, value: &str) -> Result<String, MetaDecodeError>;

    fn copy_bytes(&mut self, value: &[u8]) -> Result<Vec<u8>, MetaDecodeError>;
}

struct CheckedDecodeAllocator;

impl DecodeAllocator for CheckedDecodeAllocator {
    fn reserve_entries(
        &mut self,
        entries: &mut Vec<MetaEntry>,
        count: usize,
    ) -> Result<(), MetaDecodeError> {
        entries
            .try_reserve_exact(count)
            .map_err(|_| MetaDecodeError::AllocationFailed)
    }

    fn copy_string(&mut self, value: &str) -> Result<String, MetaDecodeError> {
        let mut owned = String::new();
        owned
            .try_reserve_exact(value.len())
            .map_err(|_| MetaDecodeError::AllocationFailed)?;
        owned.push_str(value);
        Ok(owned)
    }

    fn copy_bytes(&mut self, value: &[u8]) -> Result<Vec<u8>, MetaDecodeError> {
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(value.len())
            .map_err(|_| MetaDecodeError::AllocationFailed)?;
        owned.extend_from_slice(value);
        Ok(owned)
    }
}

fn decode_payload_with_allocator<A: DecodeAllocator>(
    payload: &[u8],
    limits: &PayloadLimits,
    allocator: &mut A,
) -> Result<Meta, MetaDecodeError> {
    let layout = validate_payload_layout(payload, limits)?;
    validate_decoded_budget(
        usize::from(layout.entry_count()),
        layout.meta_bytes(),
        limits,
    )?;
    let view = layout.validate_crc()?;
    decode_view_with_allocator(view, allocator)
}

fn decode_view_with_allocator<A: DecodeAllocator>(
    view: MetaView<'_>,
    allocator: &mut A,
) -> Result<Meta, MetaDecodeError> {
    let mut entries = Vec::new();
    allocator.reserve_entries(&mut entries, view.len())?;
    for entry in view.entries() {
        entries.push(copy_entry(entry, allocator)?);
    }
    Ok(Meta { entries })
}

fn copy_entry<A: DecodeAllocator>(
    entry: MetaEntryRef<'_>,
    allocator: &mut A,
) -> Result<MetaEntry, MetaDecodeError> {
    let key = allocator.copy_string(entry.key)?;
    let value = match entry.value {
        MetaValueRef::Text(value) => MetaValue::Text(allocator.copy_string(value)?),
        MetaValueRef::Bytes(value) => MetaValue::Bytes(allocator.copy_bytes(value)?),
        MetaValueRef::Extension { kind, flags, bytes } => MetaValue::Extension {
            kind,
            flags,
            bytes: allocator.copy_bytes(bytes)?,
        },
    };
    Ok(MetaEntry { key, value })
}

fn validate_decoded_budget(
    entry_count: usize,
    meta_bytes: usize,
    limits: &PayloadLimits,
) -> Result<(), MetaDecodeError> {
    let needed = checked_decoded_bytes(entry_count, meta_bytes)?;
    let limit = limits.max_decoded_bytes();
    if needed > limit {
        return Err(MetaDecodeError::DecodedBytesLimitExceeded { needed, limit });
    }
    Ok(())
}

fn checked_decoded_bytes(entry_count: usize, meta_bytes: usize) -> Result<usize, MetaDecodeError> {
    entry_count
        .checked_mul(size_of::<MetaEntry>())
        .and_then(|entry_bytes| entry_bytes.checked_add(meta_bytes))
        .ok_or(MetaDecodeError::SizeOverflow)
}

#[derive(Clone, Copy)]
pub(crate) struct MetaPayloadPlan<'a> {
    meta: &'a Meta,
    entry_count: u16,
    // Retained for bounded typed writes before the document layer consumes it.
    #[allow(dead_code)]
    meta_bytes: usize,
    covered_len: usize,
    payload_len: usize,
}

impl<'a> MetaPayloadPlan<'a> {
    fn new(meta: &'a Meta) -> Result<Self, MetaEncodeError> {
        let entry_count = checked_entry_count(meta.entries.len())?;
        let mut entries_len = 0usize;
        let mut meta_bytes = 0usize;

        for (index, entry) in meta.entries.iter().enumerate() {
            let (_, _, value) = entry.value.wire_parts();
            let entry_len = checked_entry_wire_len(index, entry.key.len(), value.len())?;
            if entry.key.is_empty() {
                return Err(invalid_payload(MetaDecodeError::EmptyKey {
                    index: u16::try_from(index).expect("validated META entry count"),
                }));
            }
            if entry.key.as_bytes().contains(&0) {
                return Err(invalid_payload(MetaDecodeError::KeyContainsNul {
                    index: u16::try_from(index).expect("validated META entry count"),
                }));
            }
            if let Some(kind) = entry.value.known_kind_as_extension() {
                return Err(MetaEncodeError::KnownValueKindAsExtension { index, kind });
            }
            entries_len = entries_len
                .checked_add(entry_len)
                .ok_or_else(size_overflow)?;
            meta_bytes = meta_bytes
                .checked_add(entry.key.len())
                .and_then(|bytes| bytes.checked_add(value.len()))
                .ok_or_else(size_overflow)?;
        }

        let covered_len = HEADER_LEN
            .checked_add(entries_len)
            .ok_or_else(size_overflow)?;
        let payload_len = checked_payload_len(covered_len).map_err(|_| size_overflow())?;
        Ok(Self {
            meta,
            entry_count,
            meta_bytes,
            covered_len,
            payload_len,
        })
    }

    pub(crate) const fn encoded_len(self) -> usize {
        self.payload_len
    }

    #[allow(dead_code)]
    pub(crate) fn validate_limits(self, limits: &PayloadLimits) -> Result<(), MetaDecodeError> {
        let count = u32::from(self.entry_count);
        if count > limits.max_meta_entries() {
            return Err(MetaDecodeError::TooManyEntries {
                count,
                limit: limits.max_meta_entries(),
            });
        }
        if self.meta_bytes > limits.max_meta_bytes() {
            return Err(MetaDecodeError::MetaBytesLimitExceeded {
                needed: self.meta_bytes,
                limit: limits.max_meta_bytes(),
            });
        }
        validate_decoded_budget(usize::from(self.entry_count), self.meta_bytes, limits)
    }

    #[allow(dead_code)]
    pub(crate) fn equals_payload(self, candidate: &[u8]) -> bool {
        if candidate.len() != self.payload_len {
            return false;
        }
        let expected_header = [
            VERSION,
            0,
            self.entry_count.to_le_bytes()[0],
            self.entry_count.to_le_bytes()[1],
        ];
        if candidate[..HEADER_LEN] != expected_header {
            return false;
        }

        let mut offset = HEADER_LEN;
        for entry in &self.meta.entries {
            let (kind, flags, value) = entry.value.wire_parts();
            let key_len = u16::try_from(entry.key.len()).expect("validated META key length");
            let value_len = u32::try_from(value.len()).expect("validated META value length");
            let mut header = [0; 8];
            header[..2].copy_from_slice(&key_len.to_le_bytes());
            header[2] = kind;
            header[3] = flags;
            header[4..].copy_from_slice(&value_len.to_le_bytes());
            if candidate[offset..offset + header.len()] != header {
                return false;
            }
            offset += header.len();
            if candidate[offset..offset + entry.key.len()] != *entry.key.as_bytes() {
                return false;
            }
            offset += entry.key.len();
            if candidate[offset..offset + value.len()] != *value {
                return false;
            }
            offset += value.len();
        }
        debug_assert_eq!(offset, self.covered_len);

        let (covered, trailer) = candidate.split_at(self.covered_len);
        let mut expected_trailer = [0; CRC_TRAILER_LEN];
        write_crc_trailer(covered, &mut expected_trailer);
        trailer == expected_trailer
    }

    fn copy_payload_into(self, out: &mut [u8]) -> Result<usize, MetaEncodeError> {
        let needed = self.encoded_len();
        if out.len() < needed {
            return Err(MetaEncodeError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        self.emit_payload(&mut out[..needed]);
        Ok(needed)
    }

    pub(crate) fn payload_to_vec(self) -> Result<Vec<u8>, MetaEncodeError> {
        self.payload_to_vec_with(|out, needed| {
            out.try_reserve_exact(needed)
                .map_err(|_| MetaEncodeError::AllocationFailed)
        })
    }

    fn payload_to_vec_with(
        self,
        reserve: impl FnOnce(&mut Vec<u8>, usize) -> Result<(), MetaEncodeError>,
    ) -> Result<Vec<u8>, MetaEncodeError> {
        let needed = self.encoded_len();
        let mut out = Vec::new();
        reserve(&mut out, needed)?;
        out.resize(needed, 0);
        self.emit_payload(&mut out);
        Ok(out)
    }

    fn emit_payload(self, out: &mut [u8]) {
        debug_assert_eq!(out.len(), self.payload_len);
        let (covered, trailer) = out.split_at_mut(self.covered_len);
        covered[0] = VERSION;
        covered[1] = 0;
        covered[2..4].copy_from_slice(&self.entry_count.to_le_bytes());

        let mut offset = HEADER_LEN;
        for entry in &self.meta.entries {
            let (kind, flags, value) = entry.value.wire_parts();
            let key_len = u16::try_from(entry.key.len()).expect("validated META key length");
            let value_len = u32::try_from(value.len()).expect("validated META value length");
            covered[offset..offset + 2].copy_from_slice(&key_len.to_le_bytes());
            covered[offset + 2] = kind;
            covered[offset + 3] = flags;
            covered[offset + 4..offset + 8].copy_from_slice(&value_len.to_le_bytes());
            offset += 8;
            covered[offset..offset + entry.key.len()].copy_from_slice(entry.key.as_bytes());
            offset += entry.key.len();
            covered[offset..offset + value.len()].copy_from_slice(value);
            offset += value.len();
        }
        debug_assert_eq!(offset, covered.len());
        let trailer: &mut [u8; CRC_TRAILER_LEN] =
            trailer.try_into().expect("validated META trailer length");
        write_crc_trailer(covered, trailer);
    }
}

impl MetaValue {
    fn known_kind_as_extension(&self) -> Option<u8> {
        match self {
            Self::Extension { kind, flags, .. } if *flags == 0 && (*kind == 0 || *kind == 1) => {
                Some(*kind)
            }
            _ => None,
        }
    }

    fn wire_parts(&self) -> (u8, u8, &[u8]) {
        match self {
            Self::Text(value) => (0, 0, value.as_bytes()),
            Self::Bytes(value) => (1, 0, value),
            Self::Extension { kind, flags, bytes } => (*kind, *flags, bytes),
        }
    }
}

fn checked_entry_count(actual: usize) -> Result<u16, MetaEncodeError> {
    u16::try_from(actual).map_err(|_| MetaEncodeError::EntryCountOverflow { actual })
}

fn checked_entry_wire_len(
    index: usize,
    key_len: usize,
    value_len: usize,
) -> Result<usize, MetaEncodeError> {
    u16::try_from(key_len).map_err(|_| MetaEncodeError::KeyLengthOverflow {
        index,
        actual: key_len,
    })?;
    u32::try_from(value_len).map_err(|_| MetaEncodeError::ValueLengthOverflow {
        index,
        actual: value_len,
    })?;
    8usize
        .checked_add(key_len)
        .and_then(|len| len.checked_add(value_len))
        .ok_or_else(size_overflow)
}

fn invalid_payload(error: MetaDecodeError) -> MetaEncodeError {
    MetaEncodeError::InvalidPayload(error)
}

fn size_overflow() -> MetaEncodeError {
    invalid_payload(MetaDecodeError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::payload::envelope::EnvelopeError;

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

    fn sample_meta() -> Meta {
        Meta::from_entries(vec![
            MetaEntry::text("a", "12"),
            MetaEntry::bytes("bc", vec![3]),
        ])
    }

    fn decoded_bytes(meta: &Meta) -> usize {
        meta.entries.len() * size_of::<MetaEntry>()
            + meta
                .entries
                .iter()
                .map(|entry| entry.key.len() + entry.value.wire_parts().2.len())
                .sum::<usize>()
    }

    fn assert_invalid(meta: &Meta, expected: MetaEncodeError) {
        let mut out = [0xa5; 1];
        assert_eq!(meta.encoded_payload_len(), Err(expected));
        assert_eq!(meta.encode_payload_into(&mut out), Err(expected));
        assert_eq!(meta.encode_payload(), Err(expected));
        assert_eq!(out, [0xa5]);
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ReserveRequest {
        Entries(usize),
        String(usize),
        Bytes(usize),
    }

    #[derive(Default)]
    struct TrackingAllocator {
        fail_at: Option<usize>,
        requests: Vec<ReserveRequest>,
    }

    impl TrackingAllocator {
        fn failing(fail_at: usize) -> Self {
            Self {
                fail_at: Some(fail_at),
                requests: Vec::new(),
            }
        }

        fn record(&mut self, request: ReserveRequest) -> Result<(), MetaDecodeError> {
            let call = self.requests.len();
            self.requests.push(request);
            if self.fail_at == Some(call) {
                return Err(MetaDecodeError::AllocationFailed);
            }
            Ok(())
        }
    }

    impl DecodeAllocator for TrackingAllocator {
        fn reserve_entries(
            &mut self,
            entries: &mut Vec<MetaEntry>,
            count: usize,
        ) -> Result<(), MetaDecodeError> {
            self.record(ReserveRequest::Entries(count))?;
            CheckedDecodeAllocator.reserve_entries(entries, count)
        }

        fn copy_string(&mut self, value: &str) -> Result<String, MetaDecodeError> {
            self.record(ReserveRequest::String(value.len()))?;
            CheckedDecodeAllocator.copy_string(value)
        }

        fn copy_bytes(&mut self, value: &[u8]) -> Result<Vec<u8>, MetaDecodeError> {
            self.record(ReserveRequest::Bytes(value.len()))?;
            CheckedDecodeAllocator.copy_bytes(value)
        }
    }

    #[test]
    fn owned_values_decode_and_encode_the_independent_literals() {
        let cases = [
            (Meta::new(), EMPTY.as_slice()),
            (
                Meta::from_entries(vec![MetaEntry::text("k", "v")]),
                TEXT.as_slice(),
            ),
            (
                Meta::from_entries(vec![MetaEntry::bytes("b", vec![0x00, 0xff])]),
                BYTES.as_slice(),
            ),
            (
                Meta::from_entries(vec![MetaEntry::extension(
                    "x",
                    0x80,
                    0xa5,
                    vec![0xde, 0xad, 0xbe, 0xef],
                )]),
                EXTENSION.as_slice(),
            ),
        ];

        for (expected, literal) in cases {
            let decoded = Meta::decode_with_limits(literal, &PayloadLimits::HOST).unwrap();
            assert_eq!(decoded, expected);
            assert_eq!(expected.encoded_payload_len(), Ok(literal.len()));
            assert_eq!(expected.encode_payload().unwrap(), literal);
        }
    }

    #[test]
    fn every_noncanonical_kind_flags_pair_round_trips_as_an_extension() {
        let meta = Meta::from_entries(vec![
            MetaEntry::extension("text-flags", 0, 1, vec![0xff]),
            MetaEntry::extension("bytes-flags", 1, 0x80, vec![0xfe]),
            MetaEntry::extension("reserved", 2, 0, vec![0xfd]),
            MetaEntry::extension("application", 0xff, 0xa5, vec![0xfc]),
        ]);
        let payload = meta.encode_payload().unwrap();
        let decoded = Meta::decode_with_limits(&payload, &PayloadLimits::HOST).unwrap();
        assert_eq!(decoded, meta);
        assert_eq!(decoded.encode_payload().unwrap(), payload);
    }

    #[test]
    fn decoded_budget_precedes_crc_and_every_owned_reserve() {
        let meta = sample_meta();
        let payload = meta.encode_payload().unwrap();
        let needed = decoded_bytes(&meta);
        let exact = PayloadLimits::HOST
            .with_max_meta_entries(2)
            .with_max_meta_bytes(6)
            .with_max_decoded_bytes(needed);

        let mut corrupt = payload.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0x80;

        let mut limited_allocator = TrackingAllocator::default();
        assert_eq!(
            decode_payload_with_allocator(
                &corrupt,
                &exact.with_max_decoded_bytes(needed - 1),
                &mut limited_allocator,
            ),
            Err(MetaDecodeError::DecodedBytesLimitExceeded {
                needed,
                limit: needed - 1,
            })
        );
        assert!(limited_allocator.requests.is_empty());

        let mut crc_allocator = TrackingAllocator::default();
        assert!(matches!(
            decode_payload_with_allocator(&corrupt, &exact, &mut crc_allocator),
            Err(MetaDecodeError::CrcMismatch { .. })
        ));
        assert!(crc_allocator.requests.is_empty());

        let mut allocator = TrackingAllocator::default();
        assert_eq!(
            decode_payload_with_allocator(&payload, &exact, &mut allocator),
            Ok(meta)
        );
        assert_eq!(
            allocator.requests,
            [
                ReserveRequest::Entries(2),
                ReserveRequest::String(1),
                ReserveRequest::String(2),
                ReserveRequest::String(2),
                ReserveRequest::Bytes(1),
            ]
        );
    }

    #[test]
    fn owned_decode_reports_every_component_reserve_failure() {
        let meta = sample_meta();
        let payload = meta.encode_payload().unwrap();
        let limits = PayloadLimits::HOST.with_max_decoded_bytes(decoded_bytes(&meta));
        let request_count = 5;

        for fail_at in 0..request_count {
            let mut allocator = TrackingAllocator::failing(fail_at);
            assert_eq!(
                decode_payload_with_allocator(&payload, &limits, &mut allocator),
                Err(MetaDecodeError::AllocationFailed)
            );
            assert_eq!(allocator.requests.len(), fail_at + 1);
        }
    }

    #[test]
    fn positional_crud_preserves_order_and_is_failure_atomic() {
        let mut meta =
            Meta::from_entries(vec![MetaEntry::text("a", "0"), MetaEntry::text("a", "2")]);
        meta.insert(1, MetaEntry::text("a", "1")).unwrap();
        meta.push(MetaEntry::new("A", MetaValue::Text(String::from("3"))))
            .unwrap();
        assert_eq!(
            meta.entries
                .iter()
                .map(|entry| entry.value.wire_parts().2)
                .collect::<Vec<_>>(),
            [
                b"0".as_slice(),
                b"1".as_slice(),
                b"2".as_slice(),
                b"3".as_slice()
            ]
        );

        let snapshot = meta.clone();
        let pointer = meta.entries.as_ptr();
        assert_eq!(
            meta.insert_with(1, MetaEntry::text("x", "x"), |_| {
                Err(MetaMutationError::AllocationFailed)
            }),
            Err(MetaMutationError::AllocationFailed)
        );
        assert_eq!(meta, snapshot);
        assert_eq!(meta.entries.as_ptr(), pointer);
        assert_eq!(
            meta.insert_with(99, MetaEntry::text("x", "x"), |_| unreachable!()),
            Err(MetaMutationError::IndexOutOfBounds { index: 99, len: 4 })
        );

        let replaced = meta.replace(1, MetaEntry::bytes("b", vec![9])).unwrap();
        assert_eq!(replaced, MetaEntry::text("a", "1"));
        assert_eq!(meta.remove(1).unwrap(), MetaEntry::bytes("b", vec![9]));
        assert_eq!(meta.remove_all("a"), 2);
        assert_eq!(meta.remove_all("a"), 0);
        assert_eq!(meta.entries, [MetaEntry::text("A", "3")]);
        assert_eq!(
            meta.replace(1, MetaEntry::text("x", "x")),
            Err(MetaMutationError::IndexOutOfBounds { index: 1, len: 1 })
        );
        assert_eq!(
            meta.remove(1),
            Err(MetaMutationError::IndexOutOfBounds { index: 1, len: 1 })
        );
    }

    #[test]
    fn checked_encoder_rejects_invalid_keys_and_ambiguous_extensions_first() {
        let empty = Meta::from_entries(vec![MetaEntry::text("", "value")]);
        assert_invalid(
            &empty,
            invalid_payload(MetaDecodeError::EmptyKey { index: 0 }),
        );

        let nul = Meta::from_entries(vec![MetaEntry::text("bad\0key", "value")]);
        assert_invalid(
            &nul,
            invalid_payload(MetaDecodeError::KeyContainsNul { index: 0 }),
        );

        for (kind, value) in [(0, vec![0xff]), (1, vec![0xfe])] {
            let ambiguous = Meta::from_entries(vec![MetaEntry::extension("key", kind, 0, value)]);
            assert_invalid(
                &ambiguous,
                MetaEncodeError::KnownValueKindAsExtension { index: 0, kind },
            );
        }

        let mut out = [0xa5; 1];
        assert_eq!(
            empty.encode_payload_into(&mut out),
            Err(invalid_payload(MetaDecodeError::EmptyKey { index: 0 }))
        );
        assert_eq!(out, [0xa5]);
        assert_eq!(
            empty.encode_payload_with(|_, _| unreachable!()),
            Err(invalid_payload(MetaDecodeError::EmptyKey { index: 0 }))
        );
    }

    #[test]
    fn encode_into_is_failure_atomic_suffix_safe_and_reserve_fallible() {
        let meta = sample_meta();
        let expected = meta.encode_payload().unwrap();
        let needed = expected.len();

        let mut short = vec![0xa5; needed - 1];
        let before = short.clone();
        assert_eq!(
            meta.encode_payload_into(&mut short),
            Err(MetaEncodeError::BufferTooSmall {
                needed,
                available: needed - 1,
            })
        );
        assert_eq!(short, before);

        let mut out = vec![0xa5; needed + 3];
        assert_eq!(meta.encode_payload_into(&mut out), Ok(needed));
        assert_eq!(&out[..needed], expected);
        assert_eq!(&out[needed..], &[0xa5; 3]);

        assert_eq!(
            meta.encode_payload_with(|_, requested| {
                assert_eq!(requested, needed);
                Err(MetaEncodeError::AllocationFailed)
            }),
            Err(MetaEncodeError::AllocationFailed)
        );
        let mut requested = 0;
        let encoded = meta
            .encode_payload_with(|out, needed| {
                requested = needed;
                out.try_reserve_exact(needed)
                    .map_err(|_| MetaEncodeError::AllocationFailed)
            })
            .unwrap();
        assert_eq!(requested, needed);
        assert_eq!(encoded, expected);
    }

    #[test]
    fn payload_plan_matches_only_the_complete_canonical_payload() {
        let meta = sample_meta();
        let plan = meta.payload_plan().unwrap();
        let mut payload = meta.encode_payload().unwrap();
        assert!(plan.equals_payload(&payload));

        for index in 0..payload.len() {
            payload[index] ^= 0x80;
            assert!(!plan.equals_payload(&payload), "changed byte {index}");
            payload[index] ^= 0x80;
        }

        payload[12] ^= 1;
        let covered_len = payload.len() - CRC_TRAILER_LEN;
        let (covered, trailer) = payload.split_at_mut(covered_len);
        let trailer: &mut [u8; CRC_TRAILER_LEN] = trailer.try_into().unwrap();
        write_crc_trailer(covered, trailer);
        assert!(!plan.equals_payload(&payload));

        payload.push(0);
        assert!(!plan.equals_payload(&payload));
        payload.truncate(plan.encoded_len() - 1);
        assert!(!plan.equals_payload(&payload));
    }

    #[test]
    fn plan_limits_use_exact_entry_meta_and_decoded_budgets() {
        let meta = sample_meta();
        let plan = meta.payload_plan().unwrap();
        let decoded = decoded_bytes(&meta);
        let exact = PayloadLimits::HOST
            .with_max_meta_entries(2)
            .with_max_meta_bytes(6)
            .with_max_decoded_bytes(decoded);
        assert_eq!(plan.validate_limits(&exact), Ok(()));
        assert_eq!(
            plan.validate_limits(&exact.with_max_meta_entries(1)),
            Err(MetaDecodeError::TooManyEntries { count: 2, limit: 1 })
        );
        assert_eq!(
            plan.validate_limits(&exact.with_max_meta_bytes(5)),
            Err(MetaDecodeError::MetaBytesLimitExceeded {
                needed: 6,
                limit: 5,
            })
        );
        assert_eq!(
            plan.validate_limits(&exact.with_max_decoded_bytes(decoded - 1)),
            Err(MetaDecodeError::DecodedBytesLimitExceeded {
                needed: decoded,
                limit: decoded - 1,
            })
        );

        let empty = Meta::new();
        assert_eq!(
            empty.payload_plan().unwrap().validate_limits(
                &PayloadLimits::HOST
                    .with_max_meta_entries(0)
                    .with_max_meta_bytes(0)
                    .with_max_decoded_bytes(0),
            ),
            Ok(())
        );
    }

    #[test]
    fn sizing_helpers_cover_every_wire_representation_boundary() {
        assert_eq!(checked_entry_count(u16::MAX as usize), Ok(u16::MAX));
        assert_eq!(
            checked_entry_count(u16::MAX as usize + 1),
            Err(MetaEncodeError::EntryCountOverflow {
                actual: u16::MAX as usize + 1,
            })
        );
        assert!(checked_entry_wire_len(3, u16::MAX as usize, 0).is_ok());
        assert_eq!(
            checked_entry_wire_len(3, u16::MAX as usize + 1, 0),
            Err(MetaEncodeError::KeyLengthOverflow {
                index: 3,
                actual: u16::MAX as usize + 1,
            })
        );

        #[cfg(target_pointer_width = "64")]
        {
            assert!(checked_entry_wire_len(4, 1, u32::MAX as usize).is_ok());
            assert_eq!(
                checked_entry_wire_len(4, 1, u32::MAX as usize + 1),
                Err(MetaEncodeError::ValueLengthOverflow {
                    index: 4,
                    actual: u32::MAX as usize + 1,
                })
            );

            let largest_value = u32::MAX as usize - 17;
            let largest_entry = checked_entry_wire_len(0, 1, largest_value).unwrap();
            let largest_covered = HEADER_LEN + largest_entry;
            assert_eq!(checked_payload_len(largest_covered), Ok(u32::MAX as usize));
            assert_eq!(
                checked_payload_len(largest_covered + 1),
                Err(EnvelopeError::SizeOverflow)
            );
        }
    }
}
