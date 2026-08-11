use alloc::vec::Vec;

use super::preflight::{validate_header_fields, validate_metric_codepoint};
use super::{
    FONT_CHUNK_HEADER_LEN, Font, FontReadError, HEADER_LEN, METRIC_LEN, write_header, write_metric,
};

/// Failure while encoding a canonical MIRX FONT payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontEncodeError {
    InvalidPayload(FontReadError),
    GlyphCountMismatch { declared: u32, actual: usize },
    DataLengthMismatch { expected: usize, actual: usize },
    BufferTooSmall { needed: usize, available: usize },
    AllocationFailed,
}

impl From<FontReadError> for FontEncodeError {
    fn from(value: FontReadError) -> Self {
        Self::InvalidPayload(value)
    }
}

impl Font {
    /// Returns the exact size of this font's canonical FONT payload.
    pub fn encoded_payload_len(&self) -> Result<usize, FontEncodeError> {
        Ok(self.payload_plan()?.encoded_len())
    }

    /// Encodes a canonical FONT payload into the start of `out`.
    ///
    /// The complete font is validated before output capacity is inspected.
    /// Errors leave `out` unchanged, and success preserves any unused suffix.
    pub fn encode_payload_into(&self, out: &mut [u8]) -> Result<usize, FontEncodeError> {
        self.payload_plan()?.copy_payload_into(out)
    }

    /// Allocates and encodes one exact-length canonical FONT payload.
    pub fn encode_payload(&self) -> Result<Vec<u8>, FontEncodeError> {
        self.payload_plan()?.payload_to_vec()
    }

    pub(crate) fn payload_plan(&self) -> Result<FontPayloadPlan<'_>, FontEncodeError> {
        FontPayloadPlan::new(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FontWireLayout {
    metric_offset: u32,
    data_offset: u32,
    data_len: usize,
    payload_size: u32,
}

fn checked_font_layout(
    glyph_count: u32,
    bytes_per_glyph: u32,
) -> Result<FontWireLayout, FontEncodeError> {
    let metric_bytes = u64::from(glyph_count)
        .checked_mul(METRIC_LEN as u64)
        .ok_or_else(size_overflow)?;
    let data_offset = (HEADER_LEN as u64)
        .checked_add(metric_bytes)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(size_overflow)?;
    let data_bytes = u64::from(glyph_count)
        .checked_mul(u64::from(bytes_per_glyph))
        .ok_or_else(size_overflow)?;
    let data_len = usize::try_from(data_bytes).map_err(|_| size_overflow())?;
    let body_end = u64::from(data_offset)
        .checked_add(data_bytes)
        .ok_or_else(size_overflow)?;
    let payload_size = (FONT_CHUNK_HEADER_LEN as u64)
        .checked_add(body_end)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(size_overflow)?;
    usize::try_from(payload_size).map_err(|_| size_overflow())?;

    Ok(FontWireLayout {
        metric_offset: HEADER_LEN as u32,
        data_offset,
        data_len,
        payload_size,
    })
}

fn size_overflow() -> FontEncodeError {
    FontReadError::SizeOverflow.into()
}

#[derive(Clone, Copy)]
pub(crate) struct FontPayloadPlan<'a> {
    font: &'a Font,
    layout: FontWireLayout,
}

impl<'a> FontPayloadPlan<'a> {
    fn new(font: &'a Font) -> Result<Self, FontEncodeError> {
        validate_header_fields(font.chunk_header, font.atlas)?;

        let metric_count = u32::try_from(font.metrics.len()).map_err(|_| size_overflow())?;
        if font.atlas.glyph_count != metric_count {
            return Err(FontEncodeError::GlyphCountMismatch {
                declared: font.atlas.glyph_count,
                actual: font.metrics.len(),
            });
        }

        let layout = checked_font_layout(metric_count, font.atlas.bytes_per_glyph)?;
        if font.data.len() != layout.data_len {
            return Err(FontEncodeError::DataLengthMismatch {
                expected: layout.data_len,
                actual: font.data.len(),
            });
        }

        let mut previous = None;
        for (index, metric) in font.metrics.iter().enumerate() {
            let index = u32::try_from(index).map_err(|_| size_overflow())?;
            validate_metric_codepoint(previous, index, metric.codepoint)
                .map_err(FontEncodeError::InvalidPayload)?;
            previous = Some(metric.codepoint);
        }

        Ok(Self { font, layout })
    }

    pub(crate) fn encoded_len(self) -> usize {
        usize::try_from(self.layout.payload_size).expect("validated FONT payload size fits usize")
    }

    pub(crate) fn copy_payload_into(self, out: &mut [u8]) -> Result<usize, FontEncodeError> {
        let needed = self.encoded_len();
        if out.len() < needed {
            return Err(FontEncodeError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        self.emit_payload(&mut out[..needed]);
        Ok(needed)
    }

    pub(crate) fn payload_to_vec(self) -> Result<Vec<u8>, FontEncodeError> {
        let needed = self.encoded_len();
        let mut out = Vec::new();
        out.try_reserve_exact(needed)
            .map_err(|_| FontEncodeError::AllocationFailed)?;
        out.resize(needed, 0);
        self.emit_payload(&mut out);
        Ok(out)
    }

    pub(crate) fn emit_payload(self, out: &mut [u8]) {
        debug_assert_eq!(out.len(), self.encoded_len());
        out.fill(0);

        let font = self.font;
        font.chunk_header.write(&mut out[..FONT_CHUNK_HEADER_LEN]);

        let mut atlas = font.atlas;
        atlas.metric_offset = self.layout.metric_offset;
        atlas.data_offset = self.layout.data_offset;
        let header_end = FONT_CHUNK_HEADER_LEN + HEADER_LEN;
        write_header(&mut out[FONT_CHUNK_HEADER_LEN..header_end], &atlas);

        for (index, metric) in font.metrics.iter().enumerate() {
            let start = header_end + index * METRIC_LEN;
            write_metric(&mut out[start..start + METRIC_LEN], metric);
        }

        let data_start = FONT_CHUNK_HEADER_LEN
            + usize::try_from(self.layout.data_offset)
                .expect("validated FONT data offset fits usize");
        debug_assert_eq!(data_start, header_end + font.metrics.len() * METRIC_LEN);
        out[data_start..].copy_from_slice(&font.data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::preflight::checked_bytes_per_glyph;
    use crate::{
        AtlasHeader, FontChunkHeader, FontChunkKind, FontReadError, GlyphMetric, PayloadLimits,
        SUPPORTED_VERSION, read_header, read_metric,
    };

    fn sample_font(kind: FontChunkKind, bit_depth: u8) -> Font {
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
                    bearing_x: -1,
                    bearing_y: 3,
                },
                GlyphMetric {
                    codepoint: 'B' as u32,
                    advance: 5,
                    bearing_x: 1,
                    bearing_y: 2,
                },
            ],
            data: alloc::vec![0xa5; usize::try_from(bytes_per_glyph * 2).unwrap()],
        }
    }

    fn assert_invalid(font: &Font, expected: FontEncodeError) {
        let mut out = [0xa5; 1];
        assert_eq!(font.encoded_payload_len(), Err(expected));
        assert_eq!(font.encode_payload_into(&mut out), Err(expected));
        assert_eq!(font.encode_payload(), Err(expected));
        assert_eq!(out, [0xa5; 1]);
    }

    fn invalid_payload(error: FontReadError) -> FontEncodeError {
        FontEncodeError::InvalidPayload(error)
    }

    #[test]
    fn canonical_payload_covers_every_kind_and_depth() {
        for (kind, depths) in [
            (FontChunkKind::Grayscale, &[1, 2, 4, 8][..]),
            (FontChunkKind::Sdf, &[4, 8][..]),
        ] {
            for &bit_depth in depths {
                let mut font = sample_font(kind, bit_depth);
                font.atlas.metric_offset = 128;
                font.atlas.data_offset = 256;

                let needed = FONT_CHUNK_HEADER_LEN
                    + HEADER_LEN
                    + font.metrics.len() * METRIC_LEN
                    + font.data.len();
                assert_eq!(
                    font.encoded_payload_len(),
                    Ok(needed),
                    "{kind:?}/{bit_depth}"
                );
                let payload = font.encode_payload().unwrap();
                assert_eq!(payload.len(), needed, "{kind:?}/{bit_depth}");
                assert_eq!(Font::preflight(&payload, &PayloadLimits::HOST), Ok(()));

                let atlas = read_header(
                    &payload[FONT_CHUNK_HEADER_LEN..FONT_CHUNK_HEADER_LEN + HEADER_LEN],
                );
                assert_eq!(atlas.metric_offset, HEADER_LEN as u32);
                assert_eq!(
                    atlas.data_offset,
                    (HEADER_LEN + font.metrics.len() * METRIC_LEN) as u32
                );
                assert_eq!(
                    read_metric(
                        &payload[FONT_CHUNK_HEADER_LEN + HEADER_LEN
                            ..FONT_CHUNK_HEADER_LEN + HEADER_LEN + METRIC_LEN]
                    ),
                    font.metrics[0]
                );

                let decoded = Font::decode(&payload).unwrap();
                let mut expected = font.clone();
                expected.atlas.metric_offset = HEADER_LEN as u32;
                expected.atlas.data_offset =
                    (HEADER_LEN + expected.metrics.len() * METRIC_LEN) as u32;
                assert_eq!(decoded, expected);
            }
        }
    }

    #[test]
    fn checked_and_legacy_bytes_match_for_canonical_valid_fonts() {
        let font = sample_font(FontChunkKind::Sdf, 4);
        assert_eq!(font.encode_payload().unwrap(), font.encode());
    }

    #[test]
    fn decoded_offset_gaps_are_compacted_during_checked_encoding() {
        let font = sample_font(FontChunkKind::Sdf, 4);
        let mut gapped = alloc::vec![0; 76];
        font.chunk_header
            .write(&mut gapped[..FONT_CHUNK_HEADER_LEN]);

        let mut atlas = font.atlas;
        atlas.metric_offset = 36;
        atlas.data_offset = 56;
        write_header(
            &mut gapped[FONT_CHUNK_HEADER_LEN..FONT_CHUNK_HEADER_LEN + HEADER_LEN],
            &atlas,
        );
        gapped[36] = 0x11;
        for (index, metric) in font.metrics.iter().enumerate() {
            let start = FONT_CHUNK_HEADER_LEN
                + usize::try_from(atlas.metric_offset).unwrap()
                + index * METRIC_LEN;
            write_metric(&mut gapped[start..start + METRIC_LEN], metric);
        }
        gapped[56] = 0x22;
        let data_start = FONT_CHUNK_HEADER_LEN + usize::try_from(atlas.data_offset).unwrap();
        gapped[data_start..].copy_from_slice(&font.data);

        assert_eq!(Font::preflight(&gapped, &PayloadLimits::HOST), Ok(()));
        let decoded = Font::decode(&gapped).unwrap();
        assert_eq!(decoded.atlas.metric_offset, 36);
        assert_eq!(decoded.atlas.data_offset, 56);
        assert_eq!(decoded.encode_payload().unwrap(), font.encode());
    }

    #[test]
    fn encode_into_is_failure_atomic_and_preserves_the_suffix() {
        let font = sample_font(FontChunkKind::Sdf, 4);
        let expected = font.encode_payload().unwrap();
        let needed = expected.len();

        let mut short = alloc::vec![0xa5; needed - 1];
        let before = short.clone();
        assert_eq!(
            font.encode_payload_into(&mut short),
            Err(FontEncodeError::BufferTooSmall {
                needed,
                available: needed - 1,
            })
        );
        assert_eq!(short, before);

        let mut out = alloc::vec![0xa5; needed + 3];
        assert_eq!(font.encode_payload_into(&mut out), Ok(needed));
        assert_eq!(&out[..needed], expected);
        assert_eq!(&out[needed..], &[0xa5; 3]);
    }

    #[test]
    fn rejects_fixed_header_and_geometry_invariants() {
        let base = sample_font(FontChunkKind::Sdf, 4);

        let mut version = base.clone();
        version.atlas.version = 2;
        assert_invalid(
            &version,
            invalid_payload(FontReadError::UnsupportedVersion(2)),
        );

        for (pad, offset) in [(1, 34), (0x100, 35)] {
            let mut reserved = base.clone();
            reserved.atlas._pad1 = pad;
            assert_invalid(
                &reserved,
                invalid_payload(FontReadError::ReservedNonZero { offset }),
            );
        }
        let mut reserved = base.clone();
        reserved.atlas._pad0 = 1;
        assert_invalid(
            &reserved,
            invalid_payload(FontReadError::ReservedNonZero { offset: 7 }),
        );

        let mut depth = base.clone();
        depth.atlas.bit_depth = 2;
        assert_invalid(
            &depth,
            invalid_payload(FontReadError::InvalidBitDepth {
                kind: FontChunkKind::Sdf,
                actual: 2,
            }),
        );

        let mut source = base.clone();
        source.chunk_header.size = 0;
        source.atlas.source_size = 0;
        assert_invalid(
            &source,
            invalid_payload(FontReadError::InvalidSourceSize { actual: 0 }),
        );

        for actual in [7, 9] {
            let mut geometry = base.clone();
            geometry.atlas.bytes_per_glyph = actual;
            assert_invalid(
                &geometry,
                invalid_payload(FontReadError::BytesPerGlyphMismatch {
                    expected: 8,
                    actual,
                }),
            );
        }

        let mut format = base.clone();
        format.chunk_header.format = 8;
        assert_invalid(
            &format,
            invalid_payload(FontReadError::ChunkFormatMismatch { chunk: 8, atlas: 4 }),
        );

        let mut size = base;
        size.chunk_header.size = 5;
        assert_invalid(
            &size,
            invalid_payload(FontReadError::ChunkSizeMismatch { chunk: 5, atlas: 4 }),
        );
    }

    #[test]
    fn rejects_count_and_data_length_mismatches() {
        let base = sample_font(FontChunkKind::Sdf, 4);

        for declared in [1, 3] {
            let mut count = base.clone();
            count.atlas.glyph_count = declared;
            assert_invalid(
                &count,
                FontEncodeError::GlyphCountMismatch {
                    declared,
                    actual: 2,
                },
            );
        }

        let mut empty = base.clone();
        empty.atlas.glyph_count = 0;
        empty.metrics.clear();
        empty.data.clear();
        assert_invalid(&empty, invalid_payload(FontReadError::EmptyGlyphTable));

        let mut short = base.clone();
        short.data.pop();
        assert_invalid(
            &short,
            FontEncodeError::DataLengthMismatch {
                expected: 16,
                actual: 15,
            },
        );

        let mut trailing = base;
        trailing.data.push(0);
        assert_invalid(
            &trailing,
            FontEncodeError::DataLengthMismatch {
                expected: 16,
                actual: 17,
            },
        );
    }

    #[test]
    fn rejects_invalid_or_unordered_codepoints() {
        let base = sample_font(FontChunkKind::Sdf, 4);

        for codepoint in [0xd800, 0x11_0000] {
            let mut invalid = base.clone();
            invalid.metrics[0].codepoint = codepoint;
            assert_invalid(
                &invalid,
                invalid_payload(FontReadError::InvalidUnicodeScalar {
                    index: 0,
                    codepoint,
                }),
            );
        }

        for (first, second) in [('A' as u32, 'A' as u32), ('B' as u32, 'A' as u32)] {
            let mut unordered = base.clone();
            unordered.metrics[0].codepoint = first;
            unordered.metrics[1].codepoint = second;
            assert_invalid(
                &unordered,
                invalid_payload(FontReadError::CodepointsNotStrictlyIncreasing {
                    index: 1,
                    previous: first,
                    actual: second,
                }),
            );
        }
    }

    #[test]
    fn checked_layout_rejects_unrepresentable_u32_payloads() {
        assert_eq!(
            checked_font_layout(2, 8),
            Ok(FontWireLayout {
                metric_offset: 32,
                data_offset: 48,
                data_len: 16,
                payload_size: 68,
            })
        );
        assert_eq!(
            checked_font_layout(u32::MAX, 1),
            Err(invalid_payload(FontReadError::SizeOverflow))
        );
        assert_eq!(
            checked_font_layout(1, u32::MAX),
            Err(invalid_payload(FontReadError::SizeOverflow))
        );

        let max_glyphs = (u32::MAX - 36) / 9;
        let boundary = checked_font_layout(max_glyphs, 1).unwrap();
        assert_eq!(
            u64::from(boundary.payload_size),
            36 + u64::from(max_glyphs) * 9
        );
        assert_eq!(
            checked_font_layout(max_glyphs + 1, 1),
            Err(invalid_payload(FontReadError::SizeOverflow))
        );
    }
}
