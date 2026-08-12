use alloc::vec::Vec;
use core::{mem::size_of, ops::Range};

use super::{
    AtlasHeader, FONT_CHUNK_HEADER_LEN, Font, FontChunkHeader, FontChunkKind, GlyphMetric,
    HEADER_LEN, METRIC_LEN, SUPPORTED_VERSION, read_header, read_metric,
};
use crate::reader::PayloadLimits;

/// Failure while validating or reading a MIRX FONT payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontReadError {
    Truncated {
        needed: usize,
        available: usize,
    },
    UnknownChunkKind(u8),
    UnsupportedVersion(u16),
    ReservedNonZero {
        offset: usize,
    },
    InvalidBitDepth {
        kind: FontChunkKind,
        actual: u8,
    },
    ChunkFormatMismatch {
        chunk: u8,
        atlas: u8,
    },
    ChunkSizeMismatch {
        chunk: u16,
        atlas: u16,
    },
    InvalidSourceSize {
        actual: u16,
    },
    EmptyGlyphTable,
    BytesPerGlyphMismatch {
        expected: u32,
        actual: u32,
    },
    MetricOffsetBeforeHeader {
        offset: u32,
    },
    DataOffsetBeforeHeader {
        offset: u32,
    },
    MetricOffsetUnaligned {
        offset: u32,
    },
    DataOffsetUnaligned {
        offset: u32,
    },
    MetricDataOverlap {
        metric_end: u64,
        data_offset: u32,
    },
    PayloadLengthMismatch {
        expected: usize,
        actual: usize,
    },
    TooManyGlyphs {
        count: u32,
        limit: u32,
    },
    DecodedBytesLimitExceeded {
        needed: usize,
        limit: usize,
    },
    InvalidUnicodeScalar {
        index: u32,
        codepoint: u32,
    },
    CodepointsNotStrictlyIncreasing {
        index: u32,
        previous: u32,
        actual: u32,
    },
    AllocationFailed,
    SizeOverflow,
}

struct ValidatedFontPayload {
    chunk_header: FontChunkHeader,
    atlas: AtlasHeader,
    metrics: Range<usize>,
    data: Range<usize>,
}

pub(super) fn validate_payload(
    payload: &[u8],
    limits: &PayloadLimits,
) -> Result<(), FontReadError> {
    validate_payload_layout(payload, limits).map(|_| ())
}

pub(super) fn decode_payload(
    payload: &[u8],
    limits: &PayloadLimits,
) -> Result<Font, FontReadError> {
    let ValidatedFontPayload {
        chunk_header,
        atlas,
        metrics: metric_range,
        data: data_range,
    } = validate_payload_layout(payload, limits)?;

    let glyph_count =
        usize::try_from(atlas.glyph_count).map_err(|_| FontReadError::SizeOverflow)?;
    let mut metrics = Vec::new();
    metrics
        .try_reserve_exact(glyph_count)
        .map_err(|_| FontReadError::AllocationFailed)?;
    for metric in payload[metric_range].chunks_exact(METRIC_LEN) {
        metrics.push(read_metric(metric));
    }

    let data_bytes = &payload[data_range];
    let mut data = Vec::new();
    data.try_reserve_exact(data_bytes.len())
        .map_err(|_| FontReadError::AllocationFailed)?;
    data.extend_from_slice(data_bytes);

    Ok(Font {
        chunk_header,
        atlas,
        metrics,
        data,
    })
}

fn validate_payload_layout(
    payload: &[u8],
    limits: &PayloadLimits,
) -> Result<ValidatedFontPayload, FontReadError> {
    if payload.len() < FONT_CHUNK_HEADER_LEN {
        return Err(FontReadError::Truncated {
            needed: FONT_CHUNK_HEADER_LEN,
            available: payload.len(),
        });
    }
    let kind =
        FontChunkKind::from_u8(payload[0]).ok_or(FontReadError::UnknownChunkKind(payload[0]))?;
    let fixed_len = FONT_CHUNK_HEADER_LEN + HEADER_LEN;
    if payload.len() < fixed_len {
        return Err(FontReadError::Truncated {
            needed: fixed_len,
            available: payload.len(),
        });
    }

    let prefix = FontChunkHeader {
        kind,
        format: payload[1],
        size: u16::from_le_bytes([payload[2], payload[3]]),
    };
    let body = &payload[FONT_CHUNK_HEADER_LEN..];
    let atlas = read_header(&body[..HEADER_LEN]);

    validate_header_fields(prefix, atlas)?;

    validate_offset(atlas.metric_offset, true)?;
    validate_offset(atlas.data_offset, false)?;

    let metric_len = u64::from(atlas.glyph_count)
        .checked_mul(METRIC_LEN as u64)
        .ok_or(FontReadError::SizeOverflow)?;
    let metric_end = u64::from(atlas.metric_offset)
        .checked_add(metric_len)
        .ok_or(FontReadError::SizeOverflow)?;
    if metric_end > u64::from(atlas.data_offset) {
        return Err(FontReadError::MetricDataOverlap {
            metric_end,
            data_offset: atlas.data_offset,
        });
    }

    let data_len = u64::from(atlas.glyph_count)
        .checked_mul(u64::from(atlas.bytes_per_glyph))
        .ok_or(FontReadError::SizeOverflow)?;
    let data_end = u64::from(atlas.data_offset)
        .checked_add(data_len)
        .ok_or(FontReadError::SizeOverflow)?;
    let expected_payload_len = (FONT_CHUNK_HEADER_LEN as u64)
        .checked_add(data_end)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(FontReadError::SizeOverflow)?;
    if expected_payload_len != payload.len() {
        return Err(FontReadError::PayloadLengthMismatch {
            expected: expected_payload_len,
            actual: payload.len(),
        });
    }

    if atlas.glyph_count > limits.max_font_glyphs() {
        return Err(FontReadError::TooManyGlyphs {
            count: atlas.glyph_count,
            limit: limits.max_font_glyphs(),
        });
    }
    let glyph_count =
        usize::try_from(atlas.glyph_count).map_err(|_| FontReadError::SizeOverflow)?;
    let decoded_metric_bytes = glyph_count
        .checked_mul(size_of::<GlyphMetric>())
        .ok_or(FontReadError::SizeOverflow)?;
    let decoded_data_bytes = usize::try_from(data_len).map_err(|_| FontReadError::SizeOverflow)?;
    let decoded_bytes = decoded_metric_bytes
        .checked_add(decoded_data_bytes)
        .ok_or(FontReadError::SizeOverflow)?;
    if decoded_bytes > limits.max_decoded_bytes() {
        return Err(FontReadError::DecodedBytesLimitExceeded {
            needed: decoded_bytes,
            limit: limits.max_decoded_bytes(),
        });
    }

    let metric_offset =
        usize::try_from(atlas.metric_offset).map_err(|_| FontReadError::SizeOverflow)?;
    let metric_end = usize::try_from(metric_end).map_err(|_| FontReadError::SizeOverflow)?;
    validate_metrics(&body[metric_offset..metric_end])?;

    let data_offset =
        usize::try_from(atlas.data_offset).map_err(|_| FontReadError::SizeOverflow)?;
    let data_end = usize::try_from(data_end).map_err(|_| FontReadError::SizeOverflow)?;
    let metric_start = FONT_CHUNK_HEADER_LEN
        .checked_add(metric_offset)
        .ok_or(FontReadError::SizeOverflow)?;
    let metric_end = FONT_CHUNK_HEADER_LEN
        .checked_add(metric_end)
        .ok_or(FontReadError::SizeOverflow)?;
    let data_start = FONT_CHUNK_HEADER_LEN
        .checked_add(data_offset)
        .ok_or(FontReadError::SizeOverflow)?;
    let data_end = FONT_CHUNK_HEADER_LEN
        .checked_add(data_end)
        .ok_or(FontReadError::SizeOverflow)?;

    Ok(ValidatedFontPayload {
        chunk_header: prefix,
        atlas,
        metrics: metric_start..metric_end,
        data: data_start..data_end,
    })
}

pub(super) fn validate_header_fields(
    prefix: FontChunkHeader,
    atlas: AtlasHeader,
) -> Result<(), FontReadError> {
    if atlas.version != SUPPORTED_VERSION {
        return Err(FontReadError::UnsupportedVersion(atlas.version));
    }
    if let Some(offset) = reserved_offset(atlas) {
        return Err(FontReadError::ReservedNonZero { offset });
    }
    if !valid_bit_depth(prefix.kind, atlas.bit_depth) {
        return Err(FontReadError::InvalidBitDepth {
            kind: prefix.kind,
            actual: atlas.bit_depth,
        });
    }
    if atlas.source_size == 0 {
        return Err(FontReadError::InvalidSourceSize {
            actual: atlas.source_size,
        });
    }
    let expected_bytes_per_glyph = checked_bytes_per_glyph(atlas.source_size, atlas.bit_depth)
        .ok_or(FontReadError::SizeOverflow)?;
    if atlas.bytes_per_glyph != expected_bytes_per_glyph {
        return Err(FontReadError::BytesPerGlyphMismatch {
            expected: expected_bytes_per_glyph,
            actual: atlas.bytes_per_glyph,
        });
    }
    if prefix.format != atlas.bit_depth {
        return Err(FontReadError::ChunkFormatMismatch {
            chunk: prefix.format,
            atlas: atlas.bit_depth,
        });
    }
    if prefix.size != atlas.source_size {
        return Err(FontReadError::ChunkSizeMismatch {
            chunk: prefix.size,
            atlas: atlas.source_size,
        });
    }
    if atlas.glyph_count == 0 {
        return Err(FontReadError::EmptyGlyphTable);
    }
    Ok(())
}

pub(super) fn reserved_offset(atlas: AtlasHeader) -> Option<usize> {
    if atlas._pad0 != 0 {
        return Some(FONT_CHUNK_HEADER_LEN + 3);
    }
    if atlas._pad1 != 0 {
        let [low, _high] = atlas._pad1.to_le_bytes();
        return Some(FONT_CHUNK_HEADER_LEN + 30 + usize::from(low == 0));
    }
    None
}

pub(super) fn valid_bit_depth(kind: FontChunkKind, bit_depth: u8) -> bool {
    match kind {
        FontChunkKind::Sdf => matches!(bit_depth, 4 | 8),
        FontChunkKind::Grayscale => matches!(bit_depth, 1 | 2 | 4 | 8),
    }
}

pub(crate) fn checked_bytes_per_glyph(source_size: u16, bit_depth: u8) -> Option<u32> {
    let side = u64::from(source_size);
    let bits = side.checked_mul(side)?.checked_mul(u64::from(bit_depth))?;
    u32::try_from(bits.checked_add(7)? / 8).ok()
}

fn validate_offset(offset: u32, metric: bool) -> Result<(), FontReadError> {
    if offset < HEADER_LEN as u32 {
        return Err(if metric {
            FontReadError::MetricOffsetBeforeHeader { offset }
        } else {
            FontReadError::DataOffsetBeforeHeader { offset }
        });
    }
    if offset & 3 != 0 {
        return Err(if metric {
            FontReadError::MetricOffsetUnaligned { offset }
        } else {
            FontReadError::DataOffsetUnaligned { offset }
        });
    }
    Ok(())
}

fn validate_metrics(metrics: &[u8]) -> Result<(), FontReadError> {
    let mut previous = None;
    for (index, metric) in metrics.chunks_exact(METRIC_LEN).enumerate() {
        let codepoint = u32::from_le_bytes(metric[..4].try_into().expect("complete metric"));
        let index = u32::try_from(index).map_err(|_| FontReadError::SizeOverflow)?;
        validate_metric_codepoint(previous, index, codepoint)?;
        previous = Some(codepoint);
    }
    Ok(())
}

#[allow(clippy::collapsible_if)] // Let chains are newer than Rust 1.85.
pub(super) fn validate_metric_codepoint(
    previous: Option<u32>,
    index: u32,
    codepoint: u32,
) -> Result<(), FontReadError> {
    if char::from_u32(codepoint).is_none() {
        return Err(FontReadError::InvalidUnicodeScalar { index, codepoint });
    }
    if let Some(previous) = previous {
        if codepoint <= previous {
            return Err(FontReadError::CodepointsNotStrictlyIncreasing {
                index,
                previous,
                actual: codepoint,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::{AtlasHeader, Font, FontChunkHeader, GlyphMetric};

    const ATLAS_START: usize = FONT_CHUNK_HEADER_LEN;
    const FIRST_METRIC: usize = FONT_CHUNK_HEADER_LEN + HEADER_LEN;

    fn payload(kind: FontChunkKind, bit_depth: u8) -> Vec<u8> {
        let source_size = 4;
        let bytes_per_glyph = checked_bytes_per_glyph(source_size, bit_depth).unwrap();
        Font {
            chunk_header: FontChunkHeader {
                kind,
                format: bit_depth,
                size: source_size,
            },
            atlas: AtlasHeader {
                version: SUPPORTED_VERSION,
                bit_depth,
                _pad0: 0,
                source_size,
                spread: 2,
                glyph_count: 2,
                metric_offset: HEADER_LEN as u32,
                data_offset: (HEADER_LEN + 2 * METRIC_LEN) as u32,
                bytes_per_glyph,
                ascender: 3,
                descender: 1,
                line_height: 4,
                _pad1: 0,
            },
            metrics: alloc::vec![
                GlyphMetric {
                    codepoint: 'A' as u32,
                    advance: 4,
                    bearing_x: 0,
                    bearing_y: 3,
                },
                GlyphMetric {
                    codepoint: 'B' as u32,
                    advance: 4,
                    bearing_x: 0,
                    bearing_y: 3,
                },
            ],
            data: alloc::vec![0; usize::try_from(bytes_per_glyph * 2).unwrap()],
        }
        .encode()
    }

    fn set_atlas_u16(payload: &mut [u8], offset: usize, value: u16) {
        payload[ATLAS_START + offset..ATLAS_START + offset + 2]
            .copy_from_slice(&value.to_le_bytes());
    }

    fn set_atlas_u32(payload: &mut [u8], offset: usize, value: u32) {
        payload[ATLAS_START + offset..ATLAS_START + offset + 4]
            .copy_from_slice(&value.to_le_bytes());
    }

    fn set_codepoint(payload: &mut [u8], index: usize, value: u32) {
        let start = FIRST_METRIC + index * METRIC_LEN;
        payload[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn payload_with_gaps(leading: usize, middle: usize) -> Vec<u8> {
        let canonical = payload(FontChunkKind::Sdf, 4);
        let metric_end = FIRST_METRIC + 2 * METRIC_LEN;
        let mut padded = Vec::new();
        padded.extend_from_slice(&canonical[..FIRST_METRIC]);
        padded.resize(padded.len() + leading, 0);
        padded.extend_from_slice(&canonical[FIRST_METRIC..metric_end]);
        padded.resize(padded.len() + middle, 0);
        padded.extend_from_slice(&canonical[metric_end..]);
        set_atlas_u32(&mut padded, 12, (HEADER_LEN + leading) as u32);
        set_atlas_u32(
            &mut padded,
            16,
            (HEADER_LEN + leading + 2 * METRIC_LEN + middle) as u32,
        );
        padded
    }

    #[test]
    fn accepts_every_shipped_kind_depth_and_exact_budgets() {
        for (kind, depths) in [
            (FontChunkKind::Grayscale, &[1, 2, 4, 8][..]),
            (FontChunkKind::Sdf, &[4, 8][..]),
        ] {
            for &bit_depth in depths {
                let payload = payload(kind, bit_depth);
                let bytes_per_glyph = checked_bytes_per_glyph(4, bit_depth).unwrap();
                let decoded_bytes =
                    2 * size_of::<GlyphMetric>() + usize::try_from(2 * bytes_per_glyph).unwrap();
                let limits = PayloadLimits::EMBEDDED
                    .with_max_font_glyphs(2)
                    .with_max_decoded_bytes(decoded_bytes);
                assert_eq!(Font::preflight(&payload, &limits), Ok(()));
            }
        }
    }

    #[test]
    fn distinguishes_prefix_kind_and_fixed_header_failures() {
        let valid = payload(FontChunkKind::Sdf, 4);
        for available in 0..FONT_CHUNK_HEADER_LEN {
            assert_eq!(
                Font::preflight(&valid[..available], &PayloadLimits::HOST),
                Err(FontReadError::Truncated {
                    needed: FONT_CHUNK_HEADER_LEN,
                    available,
                })
            );
        }
        for available in FONT_CHUNK_HEADER_LEN..FONT_CHUNK_HEADER_LEN + HEADER_LEN {
            assert_eq!(
                Font::preflight(&valid[..available], &PayloadLimits::HOST),
                Err(FontReadError::Truncated {
                    needed: FONT_CHUNK_HEADER_LEN + HEADER_LEN,
                    available,
                })
            );
        }

        let mut unknown = valid;
        unknown[0] = 9;
        assert_eq!(
            Font::preflight(&unknown[..FONT_CHUNK_HEADER_LEN], &PayloadLimits::HOST),
            Err(FontReadError::UnknownChunkKind(9))
        );
    }

    #[test]
    fn validates_version_reserved_geometry_and_mirrored_prefix() {
        let valid = payload(FontChunkKind::Sdf, 4);

        let mut version = valid.clone();
        set_atlas_u16(&mut version, 0, 2);
        assert_eq!(
            Font::preflight(&version, &PayloadLimits::HOST),
            Err(FontReadError::UnsupportedVersion(2))
        );

        for offset in [7, 34, 35] {
            let mut reserved = valid.clone();
            reserved[offset] = 1;
            assert_eq!(
                Font::preflight(&reserved, &PayloadLimits::HOST),
                Err(FontReadError::ReservedNonZero { offset })
            );
        }

        let mut depth = valid.clone();
        depth[ATLAS_START + 2] = 2;
        assert_eq!(
            Font::preflight(&depth, &PayloadLimits::HOST),
            Err(FontReadError::InvalidBitDepth {
                kind: FontChunkKind::Sdf,
                actual: 2,
            })
        );

        let mut zero_size = valid.clone();
        zero_size[2..4].copy_from_slice(&0u16.to_le_bytes());
        set_atlas_u16(&mut zero_size, 4, 0);
        assert_eq!(
            Font::preflight(&zero_size, &PayloadLimits::HOST),
            Err(FontReadError::InvalidSourceSize { actual: 0 })
        );

        for actual in [7, 9] {
            let mut geometry = valid.clone();
            set_atlas_u32(&mut geometry, 20, actual);
            assert_eq!(
                Font::preflight(&geometry, &PayloadLimits::HOST),
                Err(FontReadError::BytesPerGlyphMismatch {
                    expected: 8,
                    actual,
                })
            );
        }

        let mut format = valid.clone();
        format[1] = 8;
        assert_eq!(
            Font::preflight(&format, &PayloadLimits::HOST),
            Err(FontReadError::ChunkFormatMismatch { chunk: 8, atlas: 4 })
        );

        let mut size = valid.clone();
        size[2..4].copy_from_slice(&5u16.to_le_bytes());
        assert_eq!(
            Font::preflight(&size, &PayloadLimits::HOST),
            Err(FontReadError::ChunkSizeMismatch { chunk: 5, atlas: 4 })
        );

        let mut empty = valid;
        set_atlas_u32(&mut empty, 8, 0);
        assert_eq!(
            Font::preflight(&empty, &PayloadLimits::HOST),
            Err(FontReadError::EmptyGlyphTable)
        );

        assert_eq!(checked_bytes_per_glyph(u16::MAX, 8), Some(4_294_836_225));
    }

    #[test]
    fn validates_offsets_overlap_and_exact_payload_end() {
        let valid = payload(FontChunkKind::Sdf, 4);

        for (atlas_offset, value, expected) in [
            (
                12,
                28,
                FontReadError::MetricOffsetBeforeHeader { offset: 28 },
            ),
            (12, 33, FontReadError::MetricOffsetUnaligned { offset: 33 }),
            (16, 28, FontReadError::DataOffsetBeforeHeader { offset: 28 }),
            (16, 49, FontReadError::DataOffsetUnaligned { offset: 49 }),
        ] {
            let mut invalid = valid.clone();
            set_atlas_u32(&mut invalid, atlas_offset, value);
            assert_eq!(
                Font::preflight(&invalid, &PayloadLimits::HOST),
                Err(expected)
            );
        }

        let mut overlap = valid.clone();
        set_atlas_u32(&mut overlap, 16, 44);
        assert_eq!(
            Font::preflight(&overlap, &PayloadLimits::HOST),
            Err(FontReadError::MetricDataOverlap {
                metric_end: 48,
                data_offset: 44,
            })
        );

        let mut short = valid.clone();
        short.pop();
        assert_eq!(
            Font::preflight(&short, &PayloadLimits::HOST),
            Err(FontReadError::PayloadLengthMismatch {
                expected: 68,
                actual: 67,
            })
        );

        let mut trailing = valid;
        trailing.push(0);
        assert_eq!(
            Font::preflight(&trailing, &PayloadLimits::HOST),
            Err(FontReadError::PayloadLengthMismatch {
                expected: 68,
                actual: 69,
            })
        );
    }

    #[test]
    fn aligned_offset_gaps_are_nonsemantic_and_not_scanned() {
        let mut padded = payload_with_gaps(4, 4);
        padded[38] = 1;
        padded[58] = 2;
        assert_eq!(Font::preflight(&padded, &PayloadLimits::HOST), Ok(()));
    }

    #[test]
    fn bounded_decode_reads_only_validated_metric_and_data_ranges() {
        let canonical = payload(FontChunkKind::Sdf, 4);
        let expected = Font::decode(&canonical).unwrap();
        let mut padded = payload_with_gaps(4, 4);
        padded[38] = 0xff;
        padded[58] = 0xee;

        let decoded = Font::decode_with_limits(&padded, &PayloadLimits::HOST).unwrap();

        assert_eq!(decoded.chunk_header, expected.chunk_header);
        assert_eq!(decoded.metrics, expected.metrics);
        assert_eq!(decoded.data, expected.data);
        assert_eq!(decoded.atlas.metric_offset, (HEADER_LEN + 4) as u32);
        assert_eq!(
            decoded.atlas.data_offset,
            (HEADER_LEN + 4 + 2 * METRIC_LEN + 4) as u32
        );
    }

    #[test]
    fn enforces_glyph_and_decoded_component_budgets_before_scanning() {
        let payload = payload(FontChunkKind::Sdf, 4);
        let decoded_bytes = 2 * size_of::<GlyphMetric>() + 16;
        let exact = PayloadLimits::HOST
            .with_max_font_glyphs(2)
            .with_max_decoded_bytes(decoded_bytes);
        assert_eq!(Font::preflight(&payload, &exact), Ok(()));
        assert!(Font::decode_with_limits(&payload, &exact).is_ok());

        let glyph_limited = exact
            .with_max_font_glyphs(1)
            .with_max_decoded_bytes(decoded_bytes - 1);
        assert_eq!(
            Font::preflight(&payload, &glyph_limited),
            Err(FontReadError::TooManyGlyphs { count: 2, limit: 1 })
        );

        let byte_limited = exact.with_max_decoded_bytes(decoded_bytes - 1);
        assert_eq!(
            Font::preflight(&payload, &byte_limited),
            Err(FontReadError::DecodedBytesLimitExceeded {
                needed: decoded_bytes,
                limit: decoded_bytes - 1,
            })
        );
        assert_eq!(
            Font::decode_with_limits(&payload, &byte_limited),
            Err(FontReadError::DecodedBytesLimitExceeded {
                needed: decoded_bytes,
                limit: decoded_bytes - 1,
            })
        );

        let zero_bytes = exact.with_max_decoded_bytes(0);
        assert_eq!(
            Font::preflight(&payload, &zero_bytes),
            Err(FontReadError::DecodedBytesLimitExceeded {
                needed: decoded_bytes,
                limit: 0,
            })
        );
    }

    #[test]
    fn requires_unicode_scalars_in_strictly_increasing_order() {
        let valid = payload(FontChunkKind::Sdf, 4);

        for codepoint in [0xd800, 0x11_0000] {
            let mut invalid = valid.clone();
            set_codepoint(&mut invalid, 0, codepoint);
            assert_eq!(
                Font::preflight(&invalid, &PayloadLimits::HOST),
                Err(FontReadError::InvalidUnicodeScalar {
                    index: 0,
                    codepoint,
                })
            );
        }

        for (first, second) in [('A' as u32, 'A' as u32), ('B' as u32, 'A' as u32)] {
            let mut unordered = valid.clone();
            set_codepoint(&mut unordered, 0, first);
            set_codepoint(&mut unordered, 1, second);
            assert_eq!(
                Font::preflight(&unordered, &PayloadLimits::HOST),
                Err(FontReadError::CodepointsNotStrictlyIncreasing {
                    index: 1,
                    previous: first,
                    actual: second,
                })
            );
        }
    }
}
