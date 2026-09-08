use super::{ChunkRef, ContainerHeader, PayloadLimits, Reader};
use crate::frames::FramesError;
use crate::image::{ImageReadError, ImageRef};
use crate::{
    ChunkType, FontError, FontView, MetaDecodeError, PaletteDecodeError, ReadError, Scene,
    VectorReadError,
};

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
    Image(ImageReadError),
    Font(FontError),
    Vector(VectorReadError),
    Meta(MetaDecodeError),
    Palette(PaletteDecodeError),
    Frames(FramesError),
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
            if let Some(ImageRef::Encoded(image)) =
                chunk.image().map_err(PayloadValidationFailure::Image)?
            {
                image.preflight(limits).map_err(|error| {
                    PayloadValidationFailure::Image(ImageReadError::Encoded(error))
                })?;
            }
            Ok(PreflightStatus::Validated)
        }
        ChunkType::FONT => {
            FontView::open_at(chunk.payload(), chunk.payload_offset(), limits)
                .and_then(|font| font.preflight(limits))
                .map_err(PayloadValidationFailure::Font)?;
            Ok(PreflightStatus::Validated)
        }
        ChunkType::VECTOR => {
            Scene::preflight(chunk.payload(), limits).map_err(PayloadValidationFailure::Vector)?;
            Ok(PreflightStatus::Validated)
        }
        ChunkType::META => {
            chunk.meta(limits).map_err(PayloadValidationFailure::Meta)?;
            Ok(PreflightStatus::Validated)
        }
        ChunkType::PALETTE => {
            chunk
                .palette(limits)
                .map_err(PayloadValidationFailure::Palette)?;
            Ok(PreflightStatus::Validated)
        }
        ChunkType::FRAMES => {
            let frames = chunk
                .frames(limits)
                .map_err(PayloadValidationFailure::Frames)?
                .expect("FRAMES chunk must return a typed view");
            frames
                .validate_data()
                .map_err(PayloadValidationFailure::Frames)?;
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
    use crate::font::FontMetadataError;
    use crate::frames::{FrameSequence, FramesEncoder};
    use crate::header::{CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, VERSION_MINOR, chunk_type};
    use crate::image::{ColorDescription, SampleLayout, SurfaceDescriptor};
    use crate::{
        ChunkFlags, ColorFormat, FlatImageInput, ImageChunkInput, ReadOptions, SceneOp,
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
        use crate::font::{
            CmapEntry, FontAdvanceSource, FontAsset, FontFace, FontRepresentation, GlyphId,
            GlyphMap, GlyphSurfaceAsset, RasterMetrics, RawGlyphs, RepresentationAsset,
        };
        use crate::image::SampleLayout;
        let map = GlyphMap::cells(2, 2, 2).unwrap();
        let raw = RawGlyphs::builder(map, SampleLayout::A8)
            .build(&[0; 8])
            .unwrap();
        let surfaces = [GlyphSurfaceAsset::raw(raw)];
        let representations = [RepresentationAsset::new(
            FontRepresentation::coverage(8, 12, 8).unwrap(),
            0,
        )];
        let face = FontFace::new(
            1_000,
            GlyphId::NOTDEF,
            2,
            crate::Fixed::ONE,
            crate::Fixed::ZERO,
            crate::Fixed::ZERO,
        )
        .unwrap();
        let cmap = [
            CmapEntry::new('A', GlyphId::new(0)),
            CmapEntry::new('B', GlyphId::new(1)),
        ];
        let advances = [crate::Fixed::ONE; 2];
        let raster_metrics = [RasterMetrics::default(); 2];
        FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
            .with_rasters(&representations, &raster_metrics, &surfaces)
            .encode()
            .unwrap()
    }

    fn font_file(flags: u16) -> Vec<u8> {
        let payload = valid_font_payload();
        encode_chunks(&[(chunk_type::FONT, flags, &payload)])
    }

    fn vector_file(flags: u16, scene: &Scene) -> Vec<u8> {
        let payload = scene.encode().unwrap();
        encode_chunks(&[(chunk_type::VECTOR, flags, &payload)])
    }

    fn meta_file(flags: u16) -> Vec<u8> {
        const PAYLOAD: [u8; 18] = [
            0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x6b, 0x76,
            0x20, 0x0a, 0xe0, 0xcc,
        ];
        encode_chunks(&[(chunk_type::META, flags, &PAYLOAD)])
    }

    fn palette_file(flags: u16) -> Vec<u8> {
        let colors = [
            0x10, 0x20, 0x30, 0x40, 0xaa, 0xbb, 0xcc, 0xdd, 0x10, 0x20, 0x30, 0x40,
        ];
        let mut payload = alloc::vec![1, ColorFormat::RGBA8888.to_u8(), 0, 0, 3, 0, 0, 0,];
        payload.extend_from_slice(&colors);
        let checksum = crc32(&payload);
        payload.extend_from_slice(&checksum.to_le_bytes());
        encode_chunks(&[(chunk_type::PALETTE, flags, &payload)])
    }

    fn frames_file(flags: u16) -> Vec<u8> {
        let sequence = FrameSequence::new(1, 1_000, 40).unwrap();
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let mut encoder = FramesEncoder::new(sequence, surface).unwrap();
        encoder.push(&[0x10, 0x20]).unwrap();
        let encoded = encoder.finish().unwrap();
        encode_chunks(&[(chunk_type::FRAMES, flags, encoded.payload())])
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

    fn mutate_font_payload(bytes: &mut [u8], mutate: impl FnOnce(&mut [u8])) {
        let start = payload_start(bytes, 0);
        let size = u32::from_le_bytes(
            bytes[CHUNK_FILE_HEADER_LEN + 8..CHUNK_FILE_HEADER_LEN + 12]
                .try_into()
                .unwrap(),
        ) as usize;
        let payload = &mut bytes[start..start + size];
        mutate(payload);
        crate::media::refresh_checksums(payload);
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

    fn assert_critical_image_failure(bytes: &[u8], expected: ImageReadError) {
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

    fn assert_critical_font_failure(bytes: &[u8], expected: FontError) {
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

    fn assert_critical_meta_failure(bytes: &[u8], expected: MetaDecodeError) {
        let offset = payload_offset(bytes, 0);
        assert_eq!(
            Reader::open(bytes),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::META,
                    payload_offset: offset,
                },
                failure: PayloadValidationFailure::Meta(expected),
            }))
        );
    }

    fn assert_critical_palette_failure(bytes: &[u8], expected: PaletteDecodeError) {
        let offset = payload_offset(bytes, 0);
        assert_eq!(
            Reader::open(bytes),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::PALETTE,
                    payload_offset: offset,
                },
                failure: PayloadValidationFailure::Palette(expected),
            }))
        );
    }

    fn assert_critical_frames_failure(bytes: &[u8], expected: FramesError) {
        let offset = payload_offset(bytes, 0);
        assert_eq!(
            Reader::open(bytes),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::FRAMES,
                    payload_offset: offset,
                },
                failure: PayloadValidationFailure::Frames(expected),
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
                failure: PayloadValidationFailure::Font(FontError::Metadata(
                    FontMetadataError::TooManyGlyphs {
                        actual: 2,
                        limit: 1,
                    },
                )),
            }))
        );
    }

    #[test]
    fn malformed_font_is_strict_only_when_critical_or_explicitly_scanned() {
        let mut critical = font_file(ChunkFlags::CRITICAL.bits());
        mutate_font_payload(&mut critical, |payload| payload[0] = 2);
        assert_critical_font_failure(
            &critical,
            FontError::Media(crate::media::MediaPayloadError::UnsupportedVersion(2)),
        );

        let mut noncritical = font_file(0);
        mutate_font_payload(&mut noncritical, |payload| {
            let media = crate::media::MediaPayload::open(payload).unwrap();
            let offset = media
                .section(crate::media::MediaSectionKind::CMAP_INDEX)
                .unwrap()
                .descriptor()
                .offset() as usize;
            payload[offset + 6..offset + 10].copy_from_slice(&('A' as u32).to_le_bytes());
        });
        let reader = Reader::open(&noncritical).unwrap();
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Err(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::FONT,
                    payload_offset: payload_offset(&noncritical, 0),
                },
                failure: PayloadValidationFailure::Font(FontError::Metadata(
                    crate::font::FontMetadataError::Cmap(crate::font::CmapIndexError::NotSorted {
                        index: 1,
                        previous: 'A',
                        current: 'A',
                    }),
                )),
            })
        );
    }

    #[test]
    fn valid_critical_vector_passes_open_and_bounded_explicit_preflight() {
        let scene = Scene::from_ops(alloc::vec![SceneOp::PopClip]);
        let bytes = vector_file(ChunkFlags::CRITICAL.bits(), &scene);
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(
            reader
                .chunks()
                .next()
                .unwrap()
                .decode_vector(&PayloadLimits::EMBEDDED),
            Ok(Some(scene.clone()))
        );
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Ok(())
        );

        let decoded_bytes = core::mem::size_of::<SceneOp>();
        let exact = PayloadLimits::HOST
            .with_max_scene_ops(1)
            .with_max_decoded_bytes(decoded_bytes);
        assert_eq!(reader.validate_known_payloads(&exact), Ok(()));
        assert!(Reader::open_with(&bytes, &ReadOptions::new().with_payload_limits(exact)).is_ok());

        let limited = exact.with_max_scene_ops(0);
        assert_eq!(
            Reader::open_with(&bytes, &ReadOptions::new().with_payload_limits(limited),),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::VECTOR,
                    payload_offset: payload_offset(&bytes, 0),
                },
                failure: PayloadValidationFailure::Vector(VectorReadError::TooManySceneOps {
                    count: 1,
                    limit: 0
                },),
            }))
        );
    }

    #[test]
    fn malformed_vector_is_strict_only_when_critical_or_explicitly_scanned() {
        let mut critical = vector_file(ChunkFlags::CRITICAL.bits(), &Scene::default());
        set_payload_byte(&mut critical, 1, 2);
        assert_eq!(
            Reader::open(&critical),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::VECTOR,
                    payload_offset: payload_offset(&critical, 0),
                },
                failure: PayloadValidationFailure::Vector(VectorReadError::Codec(
                    crate::CodecError::UnknownVersion(2),
                )),
            }))
        );

        let mut noncritical = vector_file(0, &Scene::default());
        set_payload_byte(&mut noncritical, 1, 2);
        let reader = Reader::open(&noncritical).unwrap();
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Err(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::VECTOR,
                    payload_offset: payload_offset(&noncritical, 0),
                },
                failure: PayloadValidationFailure::Vector(VectorReadError::Codec(
                    crate::CodecError::UnknownVersion(2),
                )),
            })
        );
    }

    #[test]
    fn valid_critical_meta_passes_open_explicit_preflight_and_typed_access() {
        let bytes = meta_file(ChunkFlags::CRITICAL.bits());
        let limits = PayloadLimits::HOST
            .with_max_meta_entries(1)
            .with_max_meta_bytes(2)
            .with_max_decoded_bytes(0);
        let reader =
            Reader::open_with(&bytes, &ReadOptions::new().with_payload_limits(limits)).unwrap();
        assert_eq!(reader.validate_known_payloads(&limits), Ok(()));

        let chunk = reader.chunks().next().unwrap();
        let meta = chunk.meta(&limits).unwrap().unwrap();
        assert_eq!(
            meta.get_first("k").unwrap().value,
            crate::MetaValueRef::Text("v")
        );

        const EXTENSION: [u8; 21] = [
            0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x80, 0xa5, 0x04, 0x00, 0x00, 0x00, 0x78, 0xde,
            0xad, 0xbe, 0xef, 0xdd, 0x4a, 0x22, 0xdb,
        ];
        let extension =
            encode_chunks(&[(chunk_type::META, ChunkFlags::CRITICAL.bits(), &EXTENSION)]);
        assert!(Reader::open(&extension).is_ok());
    }

    #[test]
    fn malformed_meta_is_strict_only_when_critical_or_explicitly_scanned() {
        let mut critical = meta_file(ChunkFlags::CRITICAL.bits());
        set_payload_byte(&mut critical, 1, 0x80);
        assert_critical_meta_failure(&critical, MetaDecodeError::UnknownFlags(0x80));

        let mut unsupported = meta_file(ChunkFlags::CRITICAL.bits());
        set_payload_byte(&mut unsupported, 0, 2);
        assert_critical_meta_failure(&unsupported, MetaDecodeError::UnsupportedVersion(2));

        let mut noncritical = meta_file(0);
        set_payload_byte(&mut noncritical, 13, 0xff);
        let reader = Reader::open(&noncritical).unwrap();
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::HOST),
            Err(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::META,
                    payload_offset: payload_offset(&noncritical, 0),
                },
                failure: PayloadValidationFailure::Meta(MetaDecodeError::InvalidTextUtf8 {
                    index: 0,
                }),
            })
        );
    }

    #[test]
    fn valid_critical_palette_passes_bounded_preflight_and_typed_access() {
        let bytes = palette_file(ChunkFlags::CRITICAL.bits());
        let limits = PayloadLimits::EMBEDDED
            .with_max_palette_colors(3)
            .with_max_decoded_bytes(0);
        let reader =
            Reader::open_with(&bytes, &ReadOptions::new().with_payload_limits(limits)).unwrap();
        assert_eq!(reader.validate_known_payloads(&limits), Ok(()));

        let palette = reader
            .chunks()
            .next()
            .unwrap()
            .palette(&limits)
            .unwrap()
            .unwrap();
        assert_eq!(palette.len(), 3);
        assert_eq!(palette.colors().get(0), palette.colors().get(2));
    }

    #[test]
    fn malformed_palette_is_strict_only_when_critical_or_explicitly_scanned() {
        let mut critical = palette_file(ChunkFlags::CRITICAL.bits());
        set_payload_byte(&mut critical, 1, ColorFormat::BGRA8888.to_u8());
        assert_critical_palette_failure(
            &critical,
            PaletteDecodeError::UnsupportedColorFormat(ColorFormat::BGRA8888.to_u8()),
        );

        let mut limited = palette_file(ChunkFlags::CRITICAL.bits());
        set_payload_u32(&mut limited, 4, 4);
        assert_critical_palette_failure(
            &limited,
            PaletteDecodeError::PayloadLengthMismatch {
                expected: 28,
                actual: 24,
            },
        );
        assert!(matches!(
            Reader::open_with(
                &palette_file(ChunkFlags::CRITICAL.bits()),
                &ReadOptions::new()
                    .with_payload_limits(PayloadLimits::EMBEDDED.with_max_palette_colors(2),),
            ),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                failure: PayloadValidationFailure::Palette(PaletteDecodeError::TooManyColors {
                    count: 3,
                    limit: 2
                },),
                ..
            }))
        ));

        let mut noncritical = palette_file(0);
        set_payload_byte(&mut noncritical, 0, 2);
        let reader = Reader::open(&noncritical).unwrap();
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Err(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::PALETTE,
                    payload_offset: payload_offset(&noncritical, 0),
                },
                failure: PayloadValidationFailure::Palette(PaletteDecodeError::UnsupportedVersion(
                    2
                ),),
            })
        );
    }

    #[test]
    fn frames_preflight_is_bounded_and_exposes_typed_access() {
        let bytes = frames_file(ChunkFlags::CRITICAL.bits());
        let limits = PayloadLimits::EMBEDDED
            .with_max_frame_records(1)
            .with_max_decoded_bytes(0);
        let reader =
            Reader::open_with(&bytes, &ReadOptions::new().with_payload_limits(limits)).unwrap();
        assert_eq!(reader.validate_known_payloads(&limits), Ok(()));

        let frames = reader
            .chunks()
            .next()
            .unwrap()
            .frames(&limits)
            .unwrap()
            .unwrap();
        assert_eq!(frames.sequence().frame_count(), 1);
        assert_eq!(frames.surface().width(), 2);

        let limited = PayloadLimits::EMBEDDED.with_max_frame_records(0);
        assert!(matches!(
            Reader::open_with(&bytes, &ReadOptions::new().with_payload_limits(limited),),
            Err(ReadError::CriticalPayload(PayloadValidationError {
                failure: PayloadValidationFailure::Frames(FramesError::TooManyFrames {
                    count: 1,
                    limit: 0,
                }),
                ..
            }))
        ));
    }

    #[test]
    fn malformed_frames_is_strict_only_when_critical_or_explicitly_scanned() {
        let mut critical = frames_file(ChunkFlags::CRITICAL.bits());
        set_payload_byte(&mut critical, 0, 2);
        assert_critical_frames_failure(
            &critical,
            FramesError::Media(crate::media::MediaPayloadError::UnsupportedVersion(2)),
        );

        let mut noncritical = frames_file(0);
        set_payload_byte(&mut noncritical, 0, 2);
        let reader = Reader::open(&noncritical).unwrap();
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Err(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::FRAMES,
                    payload_offset: payload_offset(&noncritical, 0),
                },
                failure: PayloadValidationFailure::Frames(FramesError::Media(
                    crate::media::MediaPayloadError::UnsupportedVersion(2),
                )),
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
    fn critical_image_preflight_uses_the_sectioned_contract() {
        use crate::image::EncodedImageError;
        use crate::media::{MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MediaPayloadError};
        let short = encode_chunks(&[(
            chunk_type::IMAGE,
            ChunkFlags::CRITICAL.bits(),
            &[1, 0, 0, 0, 0],
        )]);
        assert_critical_image_failure(
            &short,
            ImageReadError::Media(MediaPayloadError::Truncated {
                needed: MEDIA_HEADER_LEN + MEDIA_CRC_LEN,
                available: 5,
            }),
        );
        // Header checksum, unsupported sections, section bounds, surface
        // metadata and pixel integrity all pass through the same validator.
        for (offset, value) in [
            (7, 1),
            (MEDIA_HEADER_LEN, 3),
            (MEDIA_HEADER_LEN + 4, 0xff),
            (32, 0xff),
            (64, 0xff),
        ] {
            let mut payload = valid_image_payload();
            payload[offset] = value;
            if offset != 7 && offset != 64 {
                crate::image::test_support::refresh_crc(&mut payload);
            }
            let error = ImageRef::open_at(&payload, 60).unwrap_err();
            if offset == MEDIA_HEADER_LEN {
                assert_eq!(
                    error,
                    ImageReadError::Encoded(EncodedImageError::MissingSection(
                        crate::media::MediaSectionKind::SURFACE
                    ))
                );
            }
            let file = encode_chunks(&[(chunk_type::IMAGE, ChunkFlags::CRITICAL.bits(), &payload)]);
            assert_critical_image_failure(&file, error);
        }
    }

    #[test]
    fn critical_image_checks_exact_payload_boundary_and_crc() {
        let valid = valid_image_payload();
        for mut payload in [
            valid[..valid.len() - 1].to_vec(),
            valid.clone(),
            valid.clone(),
        ] {
            if payload.len() == valid.len() {
                payload.push(0);
            }
            let expected = ImageRef::open_at(&payload, 60).unwrap_err();
            let bytes =
                encode_chunks(&[(chunk_type::IMAGE, ChunkFlags::CRITICAL.bits(), &payload)]);
            assert_critical_image_failure(&bytes, expected);
        }
        let mut corrupt = valid;
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        let error = ImageRef::open_at(&corrupt, 60).unwrap_err();
        let bytes = encode_chunks(&[(chunk_type::IMAGE, ChunkFlags::CRITICAL.bits(), &corrupt)]);
        assert_critical_image_failure(&bytes, error);
    }

    #[test]
    fn noncritical_malformed_image_opens_but_explicit_scan_reports_location() {
        use crate::media::{MEDIA_HEADER_LEN, MediaSectionKind};
        let mut payload = valid_image_payload();
        payload[MEDIA_HEADER_LEN..MEDIA_HEADER_LEN + 2]
            .copy_from_slice(&MediaSectionKind::CODINGS.raw().to_le_bytes());
        crate::image::test_support::refresh_crc(&mut payload);
        let bytes = encode_chunks(&[(chunk_type::IMAGE, 0, &payload)]);
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(reader.chunks().next().unwrap().payload(), payload);
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Err(PayloadValidationError {
                location: PayloadLocation::Chunk {
                    index: 0,
                    chunk_type: ChunkType::IMAGE,
                    payload_offset: payload_offset(&bytes, 0),
                },
                failure: PayloadValidationFailure::Image(ImageReadError::Encoded(
                    crate::image::EncodedImageError::MissingSection(MediaSectionKind::SURFACE)
                )),
            })
        );
    }

    #[test]
    fn custom_types_are_skipped_unless_critical() {
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
                failure: PayloadValidationFailure::Font(FontError::Media(
                    crate::media::MediaPayloadError::UnsupportedVersion(b'f')
                )),
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

            let mut valid_meta = meta_file(ChunkFlags::CRITICAL.bits());
            valid_meta[5] = minor;
            valid_meta[7] = flags;
            refresh_header_crc(&mut valid_meta);
            assert!(Reader::open(&valid_meta).is_ok());

            let mut valid_palette = palette_file(ChunkFlags::CRITICAL.bits());
            valid_palette[5] = minor;
            valid_palette[7] = flags;
            refresh_header_crc(&mut valid_palette);
            assert!(Reader::open(&valid_palette).is_ok());

            let mut malformed_meta = meta_file(ChunkFlags::CRITICAL.bits());
            set_payload_byte(&mut malformed_meta, 1, 1);
            malformed_meta[5] = minor;
            malformed_meta[7] = flags;
            refresh_header_crc(&mut malformed_meta);
            assert!(matches!(
                Reader::open(&malformed_meta),
                Err(ReadError::CriticalPayload(PayloadValidationError {
                    failure: PayloadValidationFailure::Meta(MetaDecodeError::UnknownFlags(1)),
                    ..
                }))
            ));

            let mut malformed_font = font_file(ChunkFlags::CRITICAL.bits());
            mutate_font_payload(&mut malformed_font, |payload| payload[0] = 2);
            malformed_font[5] = minor;
            malformed_font[7] = flags;
            refresh_header_crc(&mut malformed_font);
            assert!(matches!(
                Reader::open(&malformed_font),
                Err(ReadError::CriticalPayload(PayloadValidationError {
                    failure: PayloadValidationFailure::Font(FontError::Media(
                        crate::media::MediaPayloadError::UnsupportedVersion(2)
                    ),),
                    ..
                }))
            ));

            let mut valid_vector = vector_file(ChunkFlags::CRITICAL.bits(), &Scene::default());
            valid_vector[5] = minor;
            valid_vector[7] = flags;
            refresh_header_crc(&mut valid_vector);
            assert!(Reader::open(&valid_vector).is_ok());

            let mut malformed_vector = vector_file(ChunkFlags::CRITICAL.bits(), &Scene::default());
            set_payload_byte(&mut malformed_vector, 1, 2);
            malformed_vector[5] = minor;
            malformed_vector[7] = flags;
            refresh_header_crc(&mut malformed_vector);
            assert!(matches!(
                Reader::open(&malformed_vector),
                Err(ReadError::CriticalPayload(PayloadValidationError {
                    failure: PayloadValidationFailure::Vector(VectorReadError::Codec(
                        crate::CodecError::UnknownVersion(2),
                    )),
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

        let mut vector = vector_file(ChunkFlags::CRITICAL.bits(), &Scene::default());
        vector.extend_from_slice(b"tail");
        let reader = Reader::open_with(&vector, &options).unwrap();
        assert_eq!(reader.trailing_bytes(), b"tail");
        assert_eq!(
            reader.validate_known_payloads(&PayloadLimits::EMBEDDED),
            Ok(())
        );
    }
}
