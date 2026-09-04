use super::CodingId;
use crate::wire::{read_u16_le, read_u32_le, write_u16_le, write_u32_le};

/// Byte width of a coding table's record count.
pub const CODING_TABLE_HEADER_LEN: usize = 4;
/// Byte width of one coding identifier, revision, and parameter end.
pub const CODING_RECORD_LEN: usize = 8;

/// One coding profile and its borrowed, profile-specific parameters.
///
/// Empty parameters select the defaults of this exact identifier and revision.
/// Representability does not imply that a decoder supports the profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodingRecord<'a> {
    id: CodingId,
    revision: u16,
    params: &'a [u8],
}

impl<'a> CodingRecord<'a> {
    /// Tight logical samples in selected-plane order, without codec parameters.
    pub const RAW: Self = Self::new(CodingId::RAW, 1, &[]);

    pub const fn new(id: CodingId, revision: u16, params: &'a [u8]) -> Self {
        Self {
            id,
            revision,
            params,
        }
    }

    pub const fn id(self) -> CodingId {
        self.id
    }

    pub const fn revision(self) -> u16 {
        self.revision
    }

    pub const fn params(self) -> &'a [u8] {
        self.params
    }

    pub(crate) fn encode_entry(self, params_end: u32) -> [u8; CODING_RECORD_LEN] {
        let mut bytes = [0; CODING_RECORD_LEN];
        write_u16_le(&mut bytes, 0, self.id.raw());
        write_u16_le(&mut bytes, 2, self.revision);
        write_u32_le(&mut bytes, 4, params_end);
        bytes
    }
}

/// Validated, allocation-free ordinal access to a CODINGS section.
///
/// The section stores a little-endian count, fixed-size records, then parameter
/// bytes. Each record stores a cumulative parameter end; adjacent ends delimit
/// the borrowed parameters without duplicated offsets and sizes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodingTable<'a> {
    records: &'a [u8],
    params: &'a [u8],
}

impl<'a> CodingTable<'a> {
    /// Validates the entire table without decoding any profile parameters.
    pub fn open(bytes: &'a [u8]) -> Result<Self, CodingTableError> {
        u32::try_from(bytes.len()).map_err(|_| CodingTableError::SizeOverflow)?;
        let count = read_u32_le(bytes, 0).ok_or(CodingTableError::Truncated)?;
        if count == 0 {
            return Err(CodingTableError::EmptyTable);
        }
        let end = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(CODING_RECORD_LEN))
            .and_then(|size| size.checked_add(CODING_TABLE_HEADER_LEN))
            .ok_or(CodingTableError::SizeOverflow)?;
        let records = bytes
            .get(CODING_TABLE_HEADER_LEN..end)
            .ok_or(CodingTableError::Truncated)?;
        let params = &bytes[end..];
        let mut previous = 0;
        for (index, record) in records.chunks_exact(CODING_RECORD_LEN).enumerate() {
            let end = read_u32_le(record, 4).expect("complete coding record");
            if end < previous || u64::from(end) > params.len() as u64 {
                return Err(CodingTableError::InvalidParameterEnd {
                    index: index as u32,
                    end,
                });
            }
            previous = end;
        }
        if previous as usize != params.len() {
            return Err(CodingTableError::UnreferencedParameters);
        }
        Ok(Self { records, params })
    }

    pub const fn len(self) -> usize {
        self.records.len() / CODING_RECORD_LEN
    }

    pub const fn is_empty(self) -> bool {
        self.records.is_empty()
    }

    /// Resolves a record in constant time, including its borrowed parameters.
    pub fn get(self, index: usize) -> Option<CodingRecord<'a>> {
        if index >= self.len() {
            return None;
        }
        let offset = index * CODING_RECORD_LEN;
        let start = if index == 0 {
            0
        } else {
            read_u32_le(self.records, offset - CODING_RECORD_LEN + 4)? as usize
        };
        let end = read_u32_le(self.records, offset + 4)? as usize;
        Some(CodingRecord::new(
            CodingId::new(read_u16_le(self.records, offset)?),
            read_u16_le(self.records, offset + 2)?,
            &self.params[start..end],
        ))
    }

    pub fn iter(
        self,
    ) -> impl ExactSizeIterator<Item = CodingRecord<'a>>
    + DoubleEndedIterator
    + core::iter::FusedIterator
    + Clone
    + 'a {
        (0..self.len()).map(move |index| self.get(index).expect("validated coding ordinal"))
    }

    /// Computes the exact size, rejecting an empty table and u32 overflow.
    pub fn encoded_len(records: &[CodingRecord<'_>]) -> Result<usize, CodingTableError> {
        if records.is_empty() {
            return Err(CodingTableError::EmptyTable);
        }
        let mut size = records
            .len()
            .checked_mul(CODING_RECORD_LEN)
            .and_then(|size| size.checked_add(CODING_TABLE_HEADER_LEN))
            .ok_or(CodingTableError::SizeOverflow)?;
        for record in records {
            size = size
                .checked_add(record.params.len())
                .ok_or(CodingTableError::SizeOverflow)?;
        }
        u32::try_from(size).map_err(|_| CodingTableError::SizeOverflow)?;
        Ok(size)
    }

    /// Encodes into caller storage; validation errors leave the buffer intact.
    /// Bytes after the returned length are never written.
    pub fn encode_into(
        records: &[CodingRecord<'_>],
        out: &mut [u8],
    ) -> Result<usize, CodingTableError> {
        let needed = Self::encoded_len(records)?;
        if out.len() < needed {
            return Err(CodingTableError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        write_u32_le(out, 0, records.len() as u32);
        let params_start = CODING_TABLE_HEADER_LEN + records.len() * CODING_RECORD_LEN;
        let mut params_end = 0;
        for (index, record) in records.iter().enumerate() {
            let offset = CODING_TABLE_HEADER_LEN + index * CODING_RECORD_LEN;
            let start = params_start + params_end;
            params_end += record.params.len();
            out[offset..offset + CODING_RECORD_LEN]
                .copy_from_slice(&record.encode_entry(params_end as u32));
            out[start..params_start + params_end].copy_from_slice(record.params);
        }
        Ok(needed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CodingTableError {
    Truncated,
    EmptyTable,
    InvalidParameterEnd { index: u32, end: u32 },
    UnreferencedParameters,
    BufferTooSmall { needed: usize, available: usize },
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKET: [u8; 32] = [
        3, 0, 0, 0, 0xef, 0xbe, 2, 0, 2, 0, 0, 0, 1, 0, 9, 0, 2, 0, 0, 0, 0xff, 0xff, 0xff, 0xff,
        4, 0, 0, 0, 10, 11, 12, 13,
    ];

    #[test]
    fn independent_packet_preserves_ordinal_identity_and_parameter_boundaries() {
        let table = CodingTable::open(&PACKET).unwrap();
        assert_eq!(table.len(), 3);
        assert!(!table.is_empty());
        let records = [
            CodingRecord::new(CodingId::new(0xbeef), 2, &[10, 11]),
            CodingRecord::new(CodingId::new(1), 9, &[]),
            CodingRecord::new(CodingId::new(0xffff), 0xffff, &[12, 13]),
        ];
        assert!(table.iter().eq(records));
        assert!(table.iter().rev().eq(records.into_iter().rev()));
        assert_eq!(table.get(usize::MAX), None);
        assert_eq!(table.get(3), None);
        assert_eq!(
            table.get(0).unwrap().params().as_ptr(),
            PACKET[28..].as_ptr()
        );
        let mut out = [0xa5; 35];
        assert_eq!(CodingTable::encode_into(&records, &mut out), Ok(32));
        assert_eq!(&out[..32], PACKET);
        assert_eq!(&out[32..], &[0xa5; 3]);
    }

    #[test]
    fn every_truncated_prefix_and_unreferenced_suffix_is_rejected() {
        for end in 0..PACKET.len() {
            assert!(CodingTable::open(&PACKET[..end]).is_err(), "end {end}");
        }
        let mut trailing = [0; 33];
        trailing[..32].copy_from_slice(&PACKET);
        assert_eq!(
            CodingTable::open(&trailing),
            Err(CodingTableError::UnreferencedParameters)
        );
        assert_eq!(
            CodingTable::open(&[0; 4]),
            Err(CodingTableError::EmptyTable)
        );
        assert!(CodingTable::open(&[255; 4]).is_err());
    }

    #[test]
    fn parameter_ends_must_partition_the_body_monotonically() {
        for end in [1, 5, u32::MAX] {
            let mut bytes = PACKET;
            write_u32_le(&mut bytes, 16, end);
            assert_eq!(
                CodingTable::open(&bytes),
                Err(CodingTableError::InvalidParameterEnd { index: 1, end })
            );
        }
    }

    #[test]
    fn encoding_errors_are_atomic_and_default_parameters_cost_no_body_bytes() {
        let defaults = [CodingRecord::new(CodingId::new(123), 7, &[])];
        assert_eq!(CodingTable::encoded_len(&defaults), Ok(12));
        let mut out = [0xa5; 11];
        assert!(matches!(
            CodingTable::encode_into(&defaults, &mut out),
            Err(CodingTableError::BufferTooSmall { .. })
        ));
        assert_eq!(out, [0xa5; 11]);
        assert_eq!(
            CodingTable::encode_into(&[], &mut out),
            Err(CodingTableError::EmptyTable)
        );
        assert_eq!(out, [0xa5; 11]);
        let mut unaligned = [0; 13];
        CodingTable::encode_into(&defaults, &mut unaligned[1..]).unwrap();
        assert_eq!(
            CodingTable::open(&unaligned[1..]).unwrap().get(0),
            Some(defaults[0])
        );
    }
}
