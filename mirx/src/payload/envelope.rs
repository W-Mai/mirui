use crate::crc32;

pub(crate) const VERSION: u8 = 1;
pub(crate) const CRC_TRAILER_LEN: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EnvelopeError {
    Truncated { needed: usize, available: usize },
    UnsupportedVersion(u8),
    PayloadLengthMismatch { expected: usize, actual: usize },
    CrcMismatch { expected: u32, actual: u32 },
    SizeOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Envelope<'a> {
    covered: &'a [u8],
    stored_crc: u32,
    payload_len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExactEnvelope<'a>(Envelope<'a>);

impl<'a> Envelope<'a> {
    pub(crate) fn open_v1(
        payload: &'a [u8],
        minimum_covered_len: usize,
    ) -> Result<Self, EnvelopeError> {
        let Some(&version) = payload.first() else {
            return Err(EnvelopeError::Truncated {
                needed: 1,
                available: 0,
            });
        };
        if version != VERSION {
            return Err(EnvelopeError::UnsupportedVersion(version));
        }

        let minimum_payload_len = checked_payload_len(minimum_covered_len.max(1))?;
        if payload.len() < minimum_payload_len {
            return Err(EnvelopeError::Truncated {
                needed: minimum_payload_len,
                available: payload.len(),
            });
        }

        let crc_offset = payload.len() - CRC_TRAILER_LEN;
        let _ = checked_payload_len(crc_offset)?;
        let (covered, trailer) = payload.split_at(crc_offset);
        let stored_crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
        Ok(Self {
            covered,
            stored_crc,
            payload_len: payload.len(),
        })
    }

    pub(crate) const fn covered(self) -> &'a [u8] {
        self.covered
    }

    pub(crate) fn body(self) -> &'a [u8] {
        &self.covered[1..]
    }

    pub(crate) fn validate_exact_end(
        self,
        expected_covered_len: usize,
    ) -> Result<ExactEnvelope<'a>, EnvelopeError> {
        let expected = checked_payload_len(expected_covered_len)?;
        if expected != self.payload_len {
            return Err(EnvelopeError::PayloadLengthMismatch {
                expected,
                actual: self.payload_len,
            });
        }
        Ok(ExactEnvelope(self))
    }
}

impl<'a> ExactEnvelope<'a> {
    pub(crate) const fn covered(self) -> &'a [u8] {
        self.0.covered
    }

    pub(crate) fn body(self) -> &'a [u8] {
        &self.0.covered[1..]
    }

    pub(crate) fn validate_crc(self) -> Result<(), EnvelopeError> {
        let actual = crc32::compute(self.0.covered);
        if self.0.stored_crc != actual {
            return Err(EnvelopeError::CrcMismatch {
                expected: self.0.stored_crc,
                actual,
            });
        }
        Ok(())
    }
}

pub(crate) fn checked_payload_len(covered_len: usize) -> Result<usize, EnvelopeError> {
    let payload_len = covered_len
        .checked_add(CRC_TRAILER_LEN)
        .ok_or(EnvelopeError::SizeOverflow)?;
    let _ = u32::try_from(payload_len).map_err(|_| EnvelopeError::SizeOverflow)?;
    Ok(payload_len)
}

pub(crate) fn write_crc_trailer(covered: &[u8], trailer: &mut [u8; CRC_TRAILER_LEN]) {
    *trailer = crc32::compute(covered).to_le_bytes();
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    const MINIMUM_GOLDEN: [u8; 5] = [0x01, 0x1b, 0xdf, 0x05, 0xa5];

    fn payload_from_covered(covered: &[u8]) -> Vec<u8> {
        let mut payload = covered.to_vec();
        payload.extend_from_slice(&crc32::compute(covered).to_le_bytes());
        payload
    }

    #[test]
    fn independent_minimum_golden_opens_and_seals() {
        let envelope = Envelope::open_v1(&MINIMUM_GOLDEN, 1).unwrap();
        assert_eq!(envelope.covered(), &[VERSION]);
        assert!(envelope.body().is_empty());
        assert_eq!(envelope.covered().as_ptr(), MINIMUM_GOLDEN.as_ptr());
        let exact = envelope.validate_exact_end(1).unwrap();
        assert_eq!(exact.covered(), &[VERSION]);
        assert!(exact.body().is_empty());
        assert_eq!(exact.validate_crc(), Ok(()));

        let mut trailer = [0u8; CRC_TRAILER_LEN];
        write_crc_trailer(&[VERSION], &mut trailer);
        assert_eq!(trailer, MINIMUM_GOLDEN[1..]);
    }

    #[test]
    fn type_minimum_is_checked_before_a_safe_split() {
        const MINIMUM_COVERED_LEN: usize = 8;
        let needed = MINIMUM_COVERED_LEN + CRC_TRAILER_LEN;
        for available in 1..needed {
            let payload = vec![VERSION; available];
            assert_eq!(
                Envelope::open_v1(&payload, MINIMUM_COVERED_LEN),
                Err(EnvelopeError::Truncated { needed, available })
            );
        }

        let exact = vec![VERSION; needed];
        assert_eq!(
            Envelope::open_v1(&exact, MINIMUM_COVERED_LEN)
                .unwrap()
                .covered()
                .len(),
            MINIMUM_COVERED_LEN
        );
        assert!(Envelope::open_v1(&MINIMUM_GOLDEN, 0).is_ok());
    }

    #[test]
    fn version_error_priority_is_independent_of_type_minimum() {
        assert_eq!(
            Envelope::open_v1(&[], usize::MAX),
            Err(EnvelopeError::Truncated {
                needed: 1,
                available: 0,
            })
        );
        assert_eq!(
            Envelope::open_v1(&[VERSION + 1], usize::MAX),
            Err(EnvelopeError::UnsupportedVersion(VERSION + 1))
        );
        assert_eq!(
            Envelope::open_v1(&[VERSION], usize::MAX),
            Err(EnvelopeError::SizeOverflow)
        );
    }

    #[test]
    fn crc_validation_is_explicit_and_covers_the_complete_prefix() {
        let payload = payload_from_covered(&[VERSION, 0x10, 0x20, 0x30]);
        let envelope = Envelope::open_v1(&payload, 1).unwrap();
        let covered_len = payload.len() - CRC_TRAILER_LEN;
        assert_eq!(
            envelope
                .validate_exact_end(covered_len)
                .unwrap()
                .validate_crc(),
            Ok(())
        );

        let mut corrupted_version = payload.clone();
        corrupted_version[0] ^= 0x80;
        assert_eq!(
            Envelope::open_v1(&corrupted_version, 1),
            Err(EnvelopeError::UnsupportedVersion(VERSION ^ 0x80))
        );

        for index in 1..payload.len() - CRC_TRAILER_LEN {
            let mut corrupted = payload.clone();
            corrupted[index] ^= 0x80;
            let envelope = Envelope::open_v1(&corrupted, 1).unwrap();
            assert!(matches!(
                envelope
                    .validate_exact_end(covered_len)
                    .unwrap()
                    .validate_crc(),
                Err(EnvelopeError::CrcMismatch { .. })
            ));
        }

        let mut corrupted_trailer = payload.clone();
        let last = corrupted_trailer.len() - 1;
        corrupted_trailer[last] ^= 0x80;
        let envelope = Envelope::open_v1(&corrupted_trailer, 1).unwrap();
        assert!(matches!(
            envelope
                .validate_exact_end(covered_len)
                .unwrap()
                .validate_crc(),
            Err(EnvelopeError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn crc_mismatch_reports_stored_and_computed_values() {
        let payload = payload_from_covered(&[VERSION, 0x10, 0x20]);
        let mut corrupted = payload.clone();
        corrupted[1] ^= 0x01;
        let crc_offset = payload.len() - CRC_TRAILER_LEN;
        let expected = u32::from_le_bytes([
            payload[crc_offset],
            payload[crc_offset + 1],
            payload[crc_offset + 2],
            payload[crc_offset + 3],
        ]);
        let actual = crc32::compute(&corrupted[..crc_offset]);

        assert_eq!(
            Envelope::open_v1(&corrupted, 1)
                .unwrap()
                .validate_exact_end(crc_offset)
                .unwrap()
                .validate_crc(),
            Err(EnvelopeError::CrcMismatch { expected, actual })
        );
    }

    #[test]
    fn exact_end_is_independent_from_crc_validation() {
        let payload = payload_from_covered(&[VERSION, 0x10, 0x20]);
        let envelope = Envelope::open_v1(&payload, 1).unwrap();

        assert_eq!(
            envelope.validate_exact_end(2),
            Err(EnvelopeError::PayloadLengthMismatch {
                expected: 2 + CRC_TRAILER_LEN,
                actual: payload.len(),
            })
        );
        assert_eq!(
            envelope.validate_exact_end(4),
            Err(EnvelopeError::PayloadLengthMismatch {
                expected: 4 + CRC_TRAILER_LEN,
                actual: payload.len(),
            })
        );
        let exact = envelope.validate_exact_end(3).unwrap();
        assert_eq!(exact.covered(), &[VERSION, 0x10, 0x20]);
        assert_eq!(exact.body(), &[0x10, 0x20]);
        assert_eq!(exact.validate_crc(), Ok(()));
    }

    #[test]
    fn recomputed_physical_crc_does_not_hide_semantic_trailing_bytes() {
        let mut covered = vec![VERSION, 0xaa];
        covered.extend_from_slice(&MINIMUM_GOLDEN[1..]);
        let payload = payload_from_covered(&covered);
        let envelope = Envelope::open_v1(&payload, 1).unwrap();

        let crc_offset = payload.len() - CRC_TRAILER_LEN;
        let stored_crc = u32::from_le_bytes([
            payload[crc_offset],
            payload[crc_offset + 1],
            payload[crc_offset + 2],
            payload[crc_offset + 3],
        ]);
        assert_eq!(stored_crc, crc32::compute(&payload[..crc_offset]));
        assert_eq!(
            envelope.validate_exact_end(1),
            Err(EnvelopeError::PayloadLengthMismatch {
                expected: MINIMUM_GOLDEN.len(),
                actual: payload.len(),
            })
        );
    }

    #[test]
    fn trailer_write_is_little_endian_and_preserves_caller_suffix() {
        let covered = [
            VERSION, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9',
        ];
        let payload_len = checked_payload_len(covered.len()).unwrap();
        let mut output = vec![0xcc; payload_len + 3];
        output[..covered.len()].copy_from_slice(&covered);

        let (payload, suffix) = output.split_at_mut(payload_len);
        let (covered_output, trailer) = payload.split_at_mut(covered.len());
        let trailer: &mut [u8; CRC_TRAILER_LEN] = trailer.try_into().unwrap();
        write_crc_trailer(covered_output, trailer);

        assert_eq!(covered_output, &covered);
        assert_eq!(trailer.as_slice(), crc32::compute(&covered).to_le_bytes());
        assert_eq!(suffix, &[0xcc; 3]);
        assert_eq!(
            Envelope::open_v1(payload, covered.len())
                .unwrap()
                .validate_exact_end(covered.len())
                .unwrap()
                .validate_crc(),
            Ok(())
        );
    }

    #[test]
    fn payload_length_checks_wire_and_usize_boundaries() {
        let largest_covered = u32::MAX as usize - CRC_TRAILER_LEN;
        assert_eq!(checked_payload_len(largest_covered), Ok(u32::MAX as usize));
        assert_eq!(
            checked_payload_len(largest_covered + 1),
            Err(EnvelopeError::SizeOverflow)
        );
        assert_eq!(
            checked_payload_len(usize::MAX),
            Err(EnvelopeError::SizeOverflow)
        );

        let envelope = Envelope::open_v1(&MINIMUM_GOLDEN, 1).unwrap();
        assert_eq!(
            envelope.validate_exact_end(usize::MAX),
            Err(EnvelopeError::SizeOverflow)
        );
    }
}
