use super::{ChunkRef, ContainerHeader, PayloadLimits, Reader};
use crate::{ChunkType, Font, FontReadError, ImagePayloadError, ReadError};

/// Source location of a payload validation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PayloadLocation {
    FlatImage,
    Chunk {
        index: u16,
        chunk_type: ChunkType,
        payload_offset: u32,
    },
}

/// Semantic failure reported by bounded payload validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PayloadValidationFailure {
    UnsupportedStandardPayload,
    Image(ImagePayloadError),
    Font(FontReadError),
}

/// A payload validation failure bound to its MIRX source location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadValidationError {
    location: PayloadLocation,
    failure: PayloadValidationFailure,
}

impl PayloadValidationError {
    pub const fn location(&self) -> PayloadLocation {
        self.location
    }

    pub const fn failure(&self) -> PayloadValidationFailure {
        self.failure
    }

    const fn at_flat(failure: PayloadValidationFailure) -> Self {
        Self {
            location: PayloadLocation::FlatImage,
            failure,
        }
    }

    fn at_chunk(chunk: ChunkRef<'_>, failure: PayloadValidationFailure) -> Self {
        Self {
            location: PayloadLocation::Chunk {
                index: u16::try_from(chunk.index()).expect("validated MIRX chunk count"),
                chunk_type: chunk.chunk_type(),
                payload_offset: chunk.payload_offset(),
            },
            failure,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreflightStatus {
    Validated,
    UnsupportedStandard,
    Custom,
}

impl<'a> Reader<'a> {
    /// Validates every implemented standard payload without allocating.
    ///
    /// Custom chunk types are skipped. A standard type whose preflight is not
    /// implemented is reported explicitly instead of being treated as valid.
    pub fn validate_known_payloads(
        &self,
        limits: &PayloadLimits,
    ) -> Result<(), PayloadValidationError> {
        if matches!(self.header, ContainerHeader::Flat(_)) {
            return if self.has_future_semantics {
                Err(PayloadValidationError::at_flat(
                    PayloadValidationFailure::UnsupportedStandardPayload,
                ))
            } else {
                Ok(())
            };
        }

        for chunk in self.chunks() {
            match preflight_chunk(chunk, limits) {
                Ok(PreflightStatus::Validated | PreflightStatus::Custom) => {}
                Ok(PreflightStatus::UnsupportedStandard) => {
                    return Err(PayloadValidationError::at_chunk(
                        chunk,
                        PayloadValidationFailure::UnsupportedStandardPayload,
                    ));
                }
                Err(failure) => {
                    return Err(PayloadValidationError::at_chunk(chunk, failure));
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_critical_payloads(
        &self,
        limits: &PayloadLimits,
    ) -> Result<(), ReadError> {
        for chunk in self.chunks().filter(|chunk| chunk.flags().is_critical()) {
            require_understood_critical(chunk, preflight_chunk(chunk, limits))?;
        }
        Ok(())
    }
}

pub(crate) fn require_understood_critical(
    chunk: ChunkRef<'_>,
    preflight: Result<PreflightStatus, PayloadValidationFailure>,
) -> Result<(), ReadError> {
    match preflight {
        Ok(PreflightStatus::Validated) => Ok(()),
        Ok(PreflightStatus::UnsupportedStandard) => Err(ReadError::CriticalPayload(
            PayloadValidationError::at_chunk(
                chunk,
                PayloadValidationFailure::UnsupportedStandardPayload,
            ),
        )),
        Ok(PreflightStatus::Custom) => Err(ReadError::UnknownCriticalChunk {
            index: u16::try_from(chunk.index()).expect("validated MIRX chunk count"),
            chunk_type: chunk.chunk_type(),
            payload_offset: chunk.payload_offset(),
        }),
        Err(failure) => Err(ReadError::CriticalPayload(
            PayloadValidationError::at_chunk(chunk, failure),
        )),
    }
}

pub(crate) fn preflight_chunk(
    chunk: ChunkRef<'_>,
    limits: &PayloadLimits,
) -> Result<PreflightStatus, PayloadValidationFailure> {
    match chunk.chunk_type() {
        ChunkType::IMAGE => {
            chunk.image().map_err(PayloadValidationFailure::Image)?;
            Ok(PreflightStatus::Validated)
        }
        ChunkType::FONT => {
            Font::preflight(chunk.payload(), limits).map_err(PayloadValidationFailure::Font)?;
            Ok(PreflightStatus::Validated)
        }
        chunk_type if is_standard(chunk_type) => Ok(PreflightStatus::UnsupportedStandard),
        _ => Ok(PreflightStatus::Custom),
    }
}

const fn is_standard(chunk_type: ChunkType) -> bool {
    matches!(
        chunk_type,
        ChunkType::IMAGE
            | ChunkType::FRAMES
            | ChunkType::VECTOR
            | ChunkType::FONT
            | ChunkType::META
            | ChunkType::PALETTE
    )
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::header::{CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, VERSION_MINOR, chunk_type};
    use crate::{
        AtlasHeader, ChunkFlags, ColorFormat, FlatImageInput, FontChunkHeader, FontChunkKind,
        GlyphMetric, HEADER_LEN, ImageChunkInput, METRIC_LEN, ReadOptions, SUPPORTED_VERSION,
        TrailingBytesPolicy, crc32, encode_chunk_image, encode_chunks, encode_flat,
    };

    fn valid_image_payload() -> Vec<u8> {
        let pixels = [1, 2, 3, 4];
        let file = encode_chunk_image(&ImageChunkInput {
            width: 2,
            height: 2,
            format: ColorFormat::A8,
            stride: 2,
            main: &pixels,
            extra: None,
        });
        let start = u32::from_le_bytes(
            file[CHUNK_FILE_HEADER_LEN + 4..CHUNK_FILE_HEADER_LEN + 8]
                .try_into()
                .unwrap(),
        ) as usize;
        let size = u32::from_le_bytes(
            file[CHUNK_FILE_HEADER_LEN + 8..CHUNK_FILE_HEADER_LEN + 12]
                .try_into()
                .unwrap(),
        ) as usize;
        file[start..start + size].to_vec()
    }

    fn image_file(flags: u16) -> Vec<u8> {
        let payload = valid_image_payload();
        encode_chunks(&[(chunk_type::IMAGE, flags, &payload)])
    }

    fn valid_font_payload() -> Vec<u8> {
        Font {
            chunk_header: FontChunkHeader {
                kind: FontChunkKind::Sdf,
                format: 4,
                size: 4,
            },
            atlas: AtlasHeader {
                version: SUPPORTED_VERSION,
                bit_depth: 4,
                _pad0: 0,
                source_size: 4,
                spread: 2,
                glyph_count: 2,
                metric_offset: HEADER_LEN as u32,
                data_offset: (HEADER_LEN + 2 * METRIC_LEN) as u32,
                bytes_per_glyph: 8,
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
            data: alloc::vec![0; 16],
        }
        .encode()
    }

    fn font_file(flags: u16) -> Vec<u8> {
        let payload = valid_font_payload();
        encode_chunks(&[(chunk_type::FONT, flags, &payload)])
    }

    fn payload_offset(bytes: &[u8], index: usize) -> u32 {
        let entry = CHUNK_FILE_HEADER_LEN + index * CHUNK_TABLE_ENTRY_LEN;
        u32::from_le_bytes(bytes[entry + 4..entry + 8].try_into().unwrap())
    }

    fn payload_start(bytes: &[u8], index: usize) -> usize {
        payload_offset(bytes, index) as usize
    }

    fn set_payload_byte(bytes: &mut [u8], relative: usize, value: u8) {
        let start = payload_start(bytes, 0);
        bytes[start + relative] = value;
    }

    fn set_payload_u32(bytes: &mut [u8], relative: usize, value: u32) {
        let start = payload_start(bytes, 0);
        bytes[start + relative..start + relative + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn mark_first_chunk_critical(bytes: &mut [u8]) {
        bytes[CHUNK_FILE_HEADER_LEN + 2..CHUNK_FILE_HEADER_LEN + 4]
            .copy_from_slice(&ChunkFlags::CRITICAL.bits().to_le_bytes());
    }

    fn refresh_header_crc(bytes: &mut [u8]) {
        let checksum = crc32(&bytes[..40]);
        bytes[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn refresh_flat_crc(bytes: &mut [u8]) {
        let checksum = crc32(&bytes[..24]);
        bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
    }

    fn assert_critical_image_failure(bytes: &[u8], expected: ImagePayloadError) {
        let offset = payload_offset(bytes, 0);
        assert_eq!(
            Reader::open(bytes),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::IMAGE,
                    payload_offset: offset,
                },
                failure: PayloadValidationFailure::Image(expected),
            }))
        );
    }

    fn assert_critical_font_failure(bytes: &[u8], expected: FontReadError) {
        let offset = payload_offset(bytes, 0);
        assert_eq!(
            Reader::open(bytes),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::FONT,
                    payload_offset: offset,
                },
                failure: PayloadValidationFailure::Font(expected),
            }))
        );
    }

    #[test]
    fn valid_critical_image_passes_open_and_explicit_preflight() {
        let bytes = image_file(ChunkFlags::CRITICAL.bits());
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Ok(())
        );

        let options = ReadOptions::new().with_payload_limits(PayloadLimits::HOST);
        assert_eq!(options.payload_limits(), PayloadLimits::HOST);
        assert!(Reader::open_with(&bytes, &options).is_ok());

        // IMAGE validation only borrows its planes, so an allocation budget of
        // zero does not reject the payload.
        let zero_allocation = PayloadLimits::EMBEDDED.with_max_decoded_bytes(0);
        assert_eq!(reader.validate_known_payloads(&zero_allocation), Ok(()));
        assert!(
            Reader::open_with(
                &bytes,
                &ReadOptions::new().with_payload_limits(zero_allocation),
            )
            .is_ok()
        );
    }

    #[test]
    fn critical_image_accepts_inline_palette_and_alpha_planes() {
        let indexed_main = [0; 4];
        let palette = [0; 64];
        let mut indexed = encode_chunk_image(&ImageChunkInput {
            width: 3,
            height: 2,
            format: ColorFormat::I4,
            stride: 2,
            main: &indexed_main,
            extra: Some(&palette),
        });
        mark_first_chunk_critical(&mut indexed);
        assert!(Reader::open(&indexed).is_ok());

        let rgb = [0; 16];
        let alpha = [0; 6];
        let mut rgb565a8 = encode_chunk_image(&ImageChunkInput {
            width: 3,
            height: 2,
            format: ColorFormat::RGB565A8,
            stride: 8,
            main: &rgb,
            extra: Some(&alpha),
        });
        mark_first_chunk_critical(&mut rgb565a8);
        assert!(Reader::open(&rgb565a8).is_ok());
    }

    #[test]
    fn valid_critical_font_passes_open_and_bounded_explicit_preflight() {
        let bytes = font_file(ChunkFlags::CRITICAL.bits());
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Ok(())
        );

        let exact = PayloadLimits::HOST
            .with_max_font_glyphs(2)
            .with_max_decoded_bytes(32);
        assert_eq!(reader.validate_known_payloads(&exact), Ok(()));
        assert!(Reader::open_with(&bytes, &ReadOptions::new().with_payload_limits(exact)).is_ok());

        let glyph_limited = exact.with_max_font_glyphs(1);
        assert_eq!(
            Reader::open_with(
                &bytes,
                &ReadOptions::new().with_payload_limits(glyph_limited),
            ),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::FONT,
                    payload_offset: payload_offset(&bytes, 0),
                },
                failure: PayloadValidationFailure::Font(FontReadError::TooManyGlyphs {
                    count: 2,
                    limit: 1,
                }),
            }))
        );
    }

    #[test]
    fn malformed_font_is_strict_only_when_critical_or_explicitly_scanned() {
        let mut critical = font_file(ChunkFlags::CRITICAL.bits());
        set_payload_byte(&mut critical, 4, 2);
        assert_critical_font_failure(&critical, FontReadError::UnsupportedVersion(2));

        let mut noncritical = font_file(0);
        set_payload_u32(&mut noncritical, 44, 'A' as u32);
        let reader = Reader::open(&noncritical).unwrap();
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Err(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::FONT,
                    payload_offset: payload_offset(&noncritical, 0),
                },
                failure: PayloadValidationFailure::Font(
                    FontReadError::CodepointsNotStrictlyIncreasing {
                        index: 1,
                        previous: 'A' as u32,
                        actual: 'A' as u32,
                    },
                ),
            })
        );
    }

    #[test]
    fn current_flat_is_already_validated_but_future_flat_is_unsupported() {
        let pixel = [0];
        let current = encode_flat(&FlatImageInput {
            width: 1,
            height: 1,
            stride: 1,
            format: ColorFormat::A8,
            main: &pixel,
            extra: None,
        });
        assert_eq!(
            Reader::open(&current)
                .unwrap()
                .validate_known_payloads(&PayloadLimits::EMBEDDED),
            Ok(())
        );

        for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
            let mut future = current.clone();
            future[5] = minor;
            future[7] = flags;
            refresh_flat_crc(&mut future);
            let reader = Reader::open(&future).unwrap();
            assert_eq!(reader.flat_image(), None);
            assert_eq!(
                reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
                Err(PayloadValidationError {
                    location: PayloadLocation::FlatImage,
                    failure: PayloadValidationFailure::UnsupportedStandardPayload,
                })
            );
        }
    }

    #[test]
    fn critical_image_checks_header_fields_before_plane_sizes() {
        let mut truncated =
            encode_chunks(&[(chunk_type::IMAGE, ChunkFlags::CRITICAL.bits(), b"short")]);
        assert_critical_image_failure(
            &truncated,
            ImagePayloadError::Truncated {
                needed: 32,
                available: 5,
            },
        );

        truncated = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_byte(&mut truncated, 10, 1);
        assert_critical_image_failure(
            &truncated,
            ImagePayloadError::ReservedNonZero { offset: 10 },
        );

        let mut compressed = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_byte(&mut compressed, 9, 7);
        assert_critical_image_failure(&compressed, ImagePayloadError::UnsupportedCompression(7));

        let mut unknown_format = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_byte(&mut unknown_format, 8, 0xfe);
        assert_critical_image_failure(&unknown_format, ImagePayloadError::UnknownColorFormat(0xfe));

        let mut small_stride = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_u32(&mut small_stride, 12, 1);
        assert_critical_image_failure(
            &small_stride,
            ImagePayloadError::StrideTooSmall {
                minimum: 2,
                actual: 1,
            },
        );
    }

    #[test]
    fn critical_image_checks_data_offset_alignment_and_padding() {
        let mut before_header = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_u32(&mut before_header, 16, 31);
        assert_critical_image_failure(
            &before_header,
            ImagePayloadError::DataOffsetBeforeHeader { offset: 31 },
        );

        let mut out_of_bounds = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_u32(&mut out_of_bounds, 16, 40);
        assert_critical_image_failure(
            &out_of_bounds,
            ImagePayloadError::Truncated {
                needed: 40,
                available: 36,
            },
        );

        let mut unaligned = image_file(ChunkFlags::CRITICAL.bits());
        let absolute_offset = payload_offset(&unaligned, 0) + 33;
        set_payload_u32(&mut unaligned, 16, 33);
        assert_critical_image_failure(
            &unaligned,
            ImagePayloadError::DataOffsetUnaligned { absolute_offset },
        );

        let mut nonzero_padding = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_u32(&mut nonzero_padding, 16, 36);
        set_payload_byte(&mut nonzero_padding, 32, 1);
        assert_critical_image_failure(
            &nonzero_padding,
            ImagePayloadError::PaddingNonZero { offset: 32 },
        );
    }

    #[test]
    fn critical_image_checks_derived_plane_sizes_and_exact_end() {
        let mut bad_extra = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_u32(&mut bad_extra, 24, 1);
        assert_critical_image_failure(
            &bad_extra,
            ImagePayloadError::ExtraDataSizeMismatch {
                expected: 0,
                actual: 1,
            },
        );

        let mut bad_data = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_u32(&mut bad_data, 20, 3);
        assert_critical_image_failure(
            &bad_data,
            ImagePayloadError::DataSizeMismatch {
                expected: 4,
                actual: 3,
            },
        );

        let mut trailing_payload = valid_image_payload();
        trailing_payload.push(0);
        let trailing = encode_chunks(&[(
            chunk_type::IMAGE,
            ChunkFlags::CRITICAL.bits(),
            &trailing_payload,
        )]);
        assert_critical_image_failure(
            &trailing,
            ImagePayloadError::PayloadLengthMismatch {
                expected: 36,
                actual: 37,
            },
        );

        let mut short_payload = valid_image_payload();
        short_payload.pop();
        let short = encode_chunks(&[(
            chunk_type::IMAGE,
            ChunkFlags::CRITICAL.bits(),
            &short_payload,
        )]);
        assert_critical_image_failure(
            &short,
            ImagePayloadError::PayloadLengthMismatch {
                expected: 36,
                actual: 35,
            },
        );

        let mut overflow = image_file(ChunkFlags::CRITICAL.bits());
        set_payload_u32(&mut overflow, 4, 2);
        set_payload_u32(&mut overflow, 12, u32::MAX);
        assert_critical_image_failure(&overflow, ImagePayloadError::SizeOverflow);
    }

    #[test]
    fn noncritical_malformed_image_opens_but_explicit_scan_reports_location() {
        let mut bytes = image_file(0);
        set_payload_byte(&mut bytes, 9, 3);
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(reader.chunks().next().unwrap().payload().len(), 36);
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Err(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::IMAGE,
                    payload_offset: payload_offset(&bytes, 0),
                },
                failure: PayloadValidationFailure::Image(
                    ImagePayloadError::UnsupportedCompression(3),
                ),
            })
        );
    }

    #[test]
    fn unsupported_standard_types_are_distinct_from_custom_types() {
        for raw_type in [
            chunk_type::FRAMES,
            chunk_type::VECTOR,
            chunk_type::META,
            chunk_type::PALETTE,
        ] {
            let critical = encode_chunks(&[(raw_type, ChunkFlags::CRITICAL.bits(), b"opaque")]);
            let chunk_type = ChunkType::new(raw_type).unwrap();
            let expected = PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type,
                    payload_offset: payload_offset(&critical, 0),
                },
                failure: PayloadValidationFailure::UnsupportedStandardPayload,
            };
            assert_eq!(
                Reader::open(&critical),
                Err(ReadError::CriticalPayload(expected))
            );

            let noncritical = encode_chunks(&[(raw_type, 0, b"opaque")]);
            let reader = Reader::open(&noncritical).unwrap();
            assert_eq!(
                reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
                Err(expected)
            );
        }

        let custom = encode_chunks(&[(0xbeef, 0, b"opaque")]);
        assert_eq!(
            Reader::open(&custom)
                .unwrap()
                .validate_known_payloads(&PayloadLimits::EMBEDDED),
            Ok(())
        );

        let critical_custom = encode_chunks(&[(0xbeef, 0xa501, b"opaque")]);
        assert_eq!(
            Reader::open(&critical_custom),
            Err(ReadError::UnknownCriticalChunk {
                index: 0,
                chunk_type: ChunkType::new(0xbeef).unwrap(),
                payload_offset: payload_offset(&critical_custom, 0),
            })
        );
    }

    #[test]
    fn open_scans_only_critical_entries_but_explicit_scan_uses_table_order() {
        let image = valid_image_payload();
        let bytes = encode_chunks(&[
            (chunk_type::FONT, 0, b"font"),
            (chunk_type::IMAGE, ChunkFlags::CRITICAL.bits(), &image),
        ]);
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::HOST),
            Err(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::FONT,
                    payload_offset: payload_offset(&bytes, 0),
                },
                failure: PayloadValidationFailure::Font(FontReadError::UnknownChunkKind(b'f')),
            })
        );
    }

    #[test]
    fn future_container_semantics_do_not_disable_critical_checks() {
        for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
            let mut valid = image_file(ChunkFlags::CRITICAL.bits());
            valid[5] = minor;
            valid[7] = flags;
            refresh_header_crc(&mut valid);
            assert!(Reader::open(&valid).is_ok());

            let mut unsupported =
                encode_chunks(&[(chunk_type::META, ChunkFlags::CRITICAL.bits(), b"opaque")]);
            unsupported[5] = minor;
            unsupported[7] = flags;
            refresh_header_crc(&mut unsupported);
            assert!(matches!(
                Reader::open(&unsupported),
                Err(ReadError::CriticalPayload(PayloadValidationError {
                    failure: PayloadValidationFailure::UnsupportedStandardPayload,
                    ..
                }))
            ));

            let mut malformed_font = font_file(ChunkFlags::CRITICAL.bits());
            set_payload_byte(&mut malformed_font, 4, 2);
            malformed_font[5] = minor;
            malformed_font[7] = flags;
            refresh_header_crc(&mut malformed_font);
            assert!(matches!(
                Reader::open(&malformed_font),
                Err(ReadError::CriticalPayload(PayloadValidationError {
                    failure: PayloadValidationFailure::Font(FontReadError::UnsupportedVersion(2),),
                    ..
                }))
            ));
        }
    }

    #[test]
    fn preserved_trailing_bytes_are_not_part_of_critical_payload_preflight() {
        let mut bytes = image_file(ChunkFlags::CRITICAL.bits());
        bytes.extend_from_slice(b"tail");
        assert!(matches!(
            Reader::open(&bytes),
            Err(ReadError::TrailingBytes { .. })
        ));

        let options = ReadOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let reader = Reader::open_with(&bytes, &options).unwrap();
        assert_eq!(reader.trailing_bytes(), b"tail");
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Ok(())
        );

        let mut font = font_file(ChunkFlags::CRITICAL.bits());
        font.extend_from_slice(b"tail");
        let reader = Reader::open_with(&font, &options).unwrap();
        assert_eq!(reader.trailing_bytes(), b"tail");
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Ok(())
        );
    }
}
