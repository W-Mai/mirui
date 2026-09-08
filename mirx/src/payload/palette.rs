#[path = "color_table.rs"]
mod color_table;
#[path = "palette/owned.rs"]
mod owned;

pub use color_table::{ColorTableIter, ColorTableView};
pub use owned::{Palette, PaletteEncodeError, PaletteMutationError};

use crate::payload::envelope::{Envelope, EnvelopeError, ExactEnvelope};
use crate::{ColorFormat, reader::PayloadLimits, wire::read_u32_le};

const HEADER_LEN: usize = 8;
const COLOR_LEN: usize = 4;

/// Failure while validating or opening a PALETTE payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PaletteDecodeError {
    Truncated { needed: usize, available: usize },
    UnsupportedVersion(u8),
    UnsupportedColorFormat(u8),
    UnknownFlags(u16),
    TooManyColors { count: u32, limit: u32 },
    PayloadLengthMismatch { expected: usize, actual: usize },
    CrcMismatch { expected: u32, actual: u32 },
    DecodedBytesLimitExceeded { needed: usize, limit: usize },
    AllocationFailed,
    SizeOverflow,
}

/// Zero-allocation view over one validated ordered PALETTE payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaletteView<'a> {
    colors: ColorTableView<'a>,
}

impl<'a> PaletteView<'a> {
    /// Opens and validates one complete PALETTE payload without allocating.
    pub fn open_payload(
        payload: &'a [u8],
        limits: &PayloadLimits,
    ) -> Result<Self, PaletteDecodeError> {
        validate_payload_layout(payload, limits)?.validate_crc()
    }

    /// Returns the number of colors in wire order.
    pub const fn len(&self) -> usize {
        self.colors.len()
    }

    /// Returns whether this PALETTE contains no colors.
    pub const fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// Returns the borrowed ordered straight-alpha RGBA color table.
    pub const fn colors(&self) -> ColorTableView<'a> {
        self.colors
    }
}

pub(crate) struct ValidatedPaletteLayout<'a> {
    exact: ExactEnvelope<'a>,
    colors: ColorTableView<'a>,
}

impl<'a> ValidatedPaletteLayout<'a> {
    pub(crate) const fn color_count(&self) -> usize {
        self.colors.len()
    }

    pub(crate) fn validate_crc(self) -> Result<PaletteView<'a>, PaletteDecodeError> {
        self.exact.validate_crc().map_err(map_envelope_error)?;
        Ok(PaletteView {
            colors: self.colors,
        })
    }
}

pub(crate) fn validate_payload_layout<'a>(
    payload: &'a [u8],
    limits: &PayloadLimits,
) -> Result<ValidatedPaletteLayout<'a>, PaletteDecodeError> {
    let envelope = Envelope::open_v1(payload, HEADER_LEN).map_err(map_envelope_error)?;
    let covered = envelope.covered();

    let color_format = covered[1];
    if color_format != ColorFormat::RGBA8888.to_u8() {
        return Err(PaletteDecodeError::UnsupportedColorFormat(color_format));
    }

    let flags = u16::from_le_bytes([covered[2], covered[3]]);
    if flags != 0 {
        return Err(PaletteDecodeError::UnknownFlags(flags));
    }

    let color_count = read_u32_le(covered, 4).ok_or(PaletteDecodeError::SizeOverflow)?;
    let limit = limits.max_palette_colors();
    if color_count > limit {
        return Err(PaletteDecodeError::TooManyColors {
            count: color_count,
            limit,
        });
    }

    let colors_len = usize::try_from(color_count)
        .ok()
        .and_then(|count| count.checked_mul(COLOR_LEN))
        .ok_or(PaletteDecodeError::SizeOverflow)?;
    let covered_len = HEADER_LEN
        .checked_add(colors_len)
        .ok_or(PaletteDecodeError::SizeOverflow)?;
    let exact = envelope
        .validate_exact_end(covered_len)
        .map_err(map_envelope_error)?;
    let colors = exact
        .covered()
        .get(HEADER_LEN..covered_len)
        .and_then(ColorTableView::from_rgba_bytes)
        .ok_or(PaletteDecodeError::SizeOverflow)?;

    Ok(ValidatedPaletteLayout { exact, colors })
}

fn map_envelope_error(error: EnvelopeError) -> PaletteDecodeError {
    match error {
        EnvelopeError::Truncated { needed, available } => {
            PaletteDecodeError::Truncated { needed, available }
        }
        EnvelopeError::UnsupportedVersion(version) => {
            PaletteDecodeError::UnsupportedVersion(version)
        }
        EnvelopeError::PayloadLengthMismatch { expected, actual } => {
            PaletteDecodeError::PayloadLengthMismatch { expected, actual }
        }
        EnvelopeError::CrcMismatch { expected, actual } => {
            PaletteDecodeError::CrcMismatch { expected, actual }
        }
        EnvelopeError::SizeOverflow => PaletteDecodeError::SizeOverflow,
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};
    use core::mem::{needs_drop, size_of};

    use super::*;
    use crate::header::chunk_type;
    use crate::{ChunkType, Reader, crc32, encode_chunks};

    fn palette_payload(colors: &[u8]) -> Vec<u8> {
        assert!(colors.chunks_exact(COLOR_LEN).remainder().is_empty());
        let mut covered = vec![0; HEADER_LEN];
        covered[0] = 1;
        covered[1] = ColorFormat::RGBA8888.to_u8();
        covered[4..8].copy_from_slice(
            &u32::try_from(colors.len() / COLOR_LEN)
                .unwrap()
                .to_le_bytes(),
        );
        covered.extend_from_slice(colors);
        let checksum = crc32(&covered);
        covered.extend_from_slice(&checksum.to_le_bytes());
        covered
    }

    fn refresh_crc(payload: &mut [u8]) {
        let crc_offset = payload.len() - 4;
        let checksum = crc32(&payload[..crc_offset]);
        payload[crc_offset..].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn opens_empty_and_ordered_duplicate_straight_rgba_tables() {
        let empty_payload = palette_payload(&[]);
        let empty = PaletteView::open_payload(&empty_payload, &PayloadLimits::EMBEDDED).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert!(empty.colors().as_bytes().is_empty());

        let rgba = [
            0x10, 0x20, 0x30, 0x40, 0xaa, 0xbb, 0xcc, 0xdd, 0x10, 0x20, 0x30, 0x40,
        ];
        let payload = palette_payload(&rgba);
        let palette = PaletteView::open_payload(&payload, &PayloadLimits::EMBEDDED).unwrap();

        assert_eq!(palette.len(), 3);
        assert_eq!(palette.colors().as_bytes(), rgba);
        assert_eq!(
            palette.colors().as_bytes().as_ptr(),
            payload[HEADER_LEN..].as_ptr()
        );
        assert_eq!(
            palette.colors().iter().collect::<Vec<_>>(),
            vec![
                crate::Color::rgba(0x10, 0x20, 0x30, 0x40),
                crate::Color::rgba(0xaa, 0xbb, 0xcc, 0xdd),
                crate::Color::rgba(0x10, 0x20, 0x30, 0x40),
            ]
        );
        assert!(!needs_drop::<PaletteView<'_>>());
        assert!(size_of::<PaletteView<'_>>() <= 16);
    }

    #[test]
    fn enforces_color_budget_without_charging_owned_decode_bytes() {
        let payload = palette_payload(&[0; 12]);
        let limits = PayloadLimits::EMBEDDED
            .with_max_palette_colors(3)
            .with_max_decoded_bytes(0);
        assert!(PaletteView::open_payload(&payload, &limits).is_ok());
        assert_eq!(
            PaletteView::open_payload(&payload, &limits.with_max_palette_colors(2)),
            Err(PaletteDecodeError::TooManyColors { count: 3, limit: 2 })
        );
    }

    #[test]
    fn validates_header_and_count_before_crc() {
        let complete = palette_payload(&[]);
        for available in 0..complete.len() {
            let expected_needed = if available == 0 { 1 } else { complete.len() };
            assert_eq!(
                PaletteView::open_payload(&complete[..available], &PayloadLimits::HOST),
                Err(PaletteDecodeError::Truncated {
                    needed: expected_needed,
                    available,
                })
            );
        }

        let mut payload = palette_payload(&[]);
        payload[0] = 2;
        assert_eq!(
            PaletteView::open_payload(&payload, &PayloadLimits::HOST),
            Err(PaletteDecodeError::UnsupportedVersion(2))
        );

        let mut payload = palette_payload(&[]);
        payload[1] = ColorFormat::BGRA8888.to_u8();
        assert_eq!(
            PaletteView::open_payload(&payload, &PayloadLimits::HOST),
            Err(PaletteDecodeError::UnsupportedColorFormat(
                ColorFormat::BGRA8888.to_u8()
            ))
        );

        let mut payload = palette_payload(&[]);
        payload[2..4].copy_from_slice(&0x8001u16.to_le_bytes());
        assert_eq!(
            PaletteView::open_payload(&payload, &PayloadLimits::HOST),
            Err(PaletteDecodeError::UnknownFlags(0x8001))
        );

        let mut payload = palette_payload(&[]);
        payload[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            PaletteView::open_payload(&payload, &PayloadLimits::HOST),
            Err(PaletteDecodeError::TooManyColors {
                count: u32::MAX,
                limit: PayloadLimits::HOST.max_palette_colors(),
            })
        );
        assert_eq!(
            PaletteView::open_payload(
                &payload,
                &PayloadLimits::HOST.with_max_palette_colors(u32::MAX),
            ),
            Err(PaletteDecodeError::SizeOverflow)
        );
    }

    #[test]
    fn exact_length_precedes_crc_and_crc_covers_header_and_colors() {
        let payload = palette_payload(&[1, 2, 3, 4]);

        let mut short = payload.clone();
        short.pop();
        assert_eq!(
            PaletteView::open_payload(&short, &PayloadLimits::HOST),
            Err(PaletteDecodeError::PayloadLengthMismatch {
                expected: payload.len(),
                actual: short.len(),
            })
        );

        let mut trailing = payload.clone();
        trailing.push(0);
        assert_eq!(
            PaletteView::open_payload(&trailing, &PayloadLimits::HOST),
            Err(PaletteDecodeError::PayloadLengthMismatch {
                expected: payload.len(),
                actual: trailing.len(),
            })
        );

        for index in 1..payload.len() {
            let mut corrupted = payload.clone();
            corrupted[index] ^= 0x80;
            if index < 8 {
                continue;
            }
            assert!(matches!(
                PaletteView::open_payload(&corrupted, &PayloadLimits::HOST),
                Err(PaletteDecodeError::CrcMismatch { .. })
            ));
        }

        let mut count_mismatch = payload.clone();
        count_mismatch[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            PaletteView::open_payload(&count_mismatch, &PayloadLimits::HOST),
            Err(PaletteDecodeError::PayloadLengthMismatch {
                expected: 12,
                actual: payload.len(),
            })
        );

        let mut valid_change = payload.clone();
        valid_change[8] ^= 0x80;
        refresh_crc(&mut valid_change);
        assert_eq!(
            PaletteView::open_payload(&valid_change, &PayloadLimits::HOST)
                .unwrap()
                .colors()
                .as_bytes(),
            &[0x81, 2, 3, 4]
        );
    }

    #[test]
    fn chunk_accessor_is_typed_and_preserves_source_identity() {
        let payload = palette_payload(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let bytes = encode_chunks(&[(chunk_type::PALETTE, 0, &payload)]);
        let reader = Reader::open(&bytes).unwrap();
        let chunk = reader.chunks().next().unwrap();
        let palette = chunk.palette(&PayloadLimits::EMBEDDED).unwrap().unwrap();

        assert_eq!(chunk.chunk_type(), ChunkType::PALETTE);
        assert_eq!(
            palette.colors().as_bytes(),
            &payload[HEADER_LEN..HEADER_LEN + 8]
        );
        assert_eq!(
            palette.colors().as_bytes().as_ptr(),
            chunk.payload()[HEADER_LEN..].as_ptr()
        );

        let other = encode_chunks(&[(chunk_type::META, 0, b"not a palette")]);
        let other = Reader::open(&other).unwrap().chunks().next().unwrap();
        assert_eq!(other.palette(&PayloadLimits::EMBEDDED), Ok(None));
    }
}
