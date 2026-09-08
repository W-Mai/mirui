use alloc::vec::Vec;

#[cfg(test)]
use super::DocumentState;
use super::payload::resolve_node_payload;
use super::{Compatibility, Document, DocumentChunkRef};
use crate::{ChunkFlags, ChunkId, ChunkType, EditError, ImageDecodeError, ImageEncodeError};

#[cfg(test)]
use crate::ImageAsset;
use crate::image::{EncodedImageAsset, ImageRef, ImageSource};

impl<'a> DocumentChunkRef<'a> {
    /// Returns this chunk as a borrowed IMAGE view.
    pub fn image(&self) -> Result<ImageRef<'a>, ImageDecodeError> {
        if self.chunk_type() != ChunkType::IMAGE {
            return Err(ImageDecodeError::UnexpectedChunkType {
                actual: self.chunk_type(),
            });
        }
        if matches!(self.document().compatibility, Compatibility::FutureReadOnly) {
            return Err(ImageDecodeError::FutureSemanticsUnsupported);
        }
        resolve_node_payload(self.document(), self.node())?
            .image_view()
            .map_err(Into::into)
    }
}

impl Document<'_> {
    /// Resolves one IMAGE node by its stable document-session identity.
    ///
    /// RAW and promoted samples remain borrowed. Encoded metadata retains the
    /// source position for explicit group, integrity and decode checks, without
    /// allocating decoded samples. Preserved future container semantics must
    /// be normalized before typed payload access.
    #[cfg(test)]
    pub(super) fn image(&self, id: ChunkId) -> Result<ImageRef<'_>, ImageDecodeError> {
        if matches!(self.compatibility, Compatibility::FutureReadOnly) {
            return Err(ImageDecodeError::FutureSemanticsUnsupported);
        }
        let DocumentState::Chunk(chunks) = &self.state else {
            return Err(ImageDecodeError::ChunkLayoutRequired);
        };
        let node = chunks
            .chunks
            .iter()
            .find(|node| node.id == id)
            .ok_or(ImageDecodeError::InvalidChunkId)?;
        if node.chunk_type != ChunkType::IMAGE {
            return Err(ImageDecodeError::UnexpectedChunkType {
                actual: node.chunk_type,
            });
        }
        resolve_node_payload(self, node)?
            .image_view()
            .map_err(Into::into)
    }

    /// Appends a checked IMAGE payload with no chunk flags.
    pub fn push_image(
        &mut self,
        image: &(impl ImageSource + ?Sized),
    ) -> Result<ChunkId, EditError> {
        self.push_image_with_flags(image, ChunkFlags::NONE)
    }

    /// Appends a checked IMAGE payload with explicit chunk flags.
    ///
    /// The asset is encoded into one owned payload after structural edit gates
    /// have passed. A FLAT document is promoted atomically before insertion.
    pub fn push_image_with_flags(
        &mut self,
        image: &(impl ImageSource + ?Sized),
        flags: ChunkFlags,
    ) -> Result<ChunkId, EditError> {
        self.push_typed_owned_with(ChunkType::IMAGE, flags, || encode_image_for_edit(image))
    }

    /// Appends already encoded IMAGE samples after bounded syntax validation.
    pub fn push_encoded_image(
        &mut self,
        image: &EncodedImageAsset<'_>,
    ) -> Result<ChunkId, EditError> {
        self.push_encoded_image_with_flags(image, ChunkFlags::NONE)
    }

    /// Appends encoded IMAGE storage with explicit flags and one payload allocation.
    /// Structural gates, metadata, codec syntax and limits are checked first.
    pub fn push_encoded_image_with_flags(
        &mut self,
        image: &EncodedImageAsset<'_>,
        flags: ChunkFlags,
    ) -> Result<ChunkId, EditError> {
        let limits = self.payload_limits;
        self.push_typed_owned_with(ChunkType::IMAGE, flags, || {
            image
                .preflight(&limits)
                .map_err(|error| image_encode_error_for_edit(error.into()))?;
            image
                .encode()
                .map_err(|error| image_encode_error_for_edit(error.into()))
        })
    }

    pub(super) fn replace_encoded_image(
        &mut self,
        id: ChunkId,
        image: &EncodedImageAsset<'_>,
    ) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_typed_owned_with(
            id,
            ChunkType::IMAGE,
            || {
                image
                    .preflight(&limits)
                    .map_err(|error| image_encode_error_for_edit(error.into()))?;
                Ok(*image)
            },
            |image, existing| {
                existing
                    .equals_encoded(*image)
                    .map_err(|error| image_encode_error_for_edit(error.into()))
            },
            |image| {
                image
                    .encode()
                    .map_err(|error| image_encode_error_for_edit(error.into()))
            },
            EditError::InvalidPayload,
        )
    }

    /// Replaces one IMAGE payload without changing its identity or descriptor.
    ///
    /// Exact canonical bytes are a no-op. Successful changes store one owned
    /// payload and refresh derived primary hints when the node is primary.
    pub(super) fn replace_image(
        &mut self,
        id: ChunkId,
        image: &(impl ImageSource + ?Sized),
    ) -> Result<(), EditError> {
        self.replace_typed_owned_with(
            id,
            ChunkType::IMAGE,
            || {
                let view = image.view().map_err(EditError::InvalidPayload)?;
                view.encoded_len()
                    .map_err(|error| image_encode_error_for_edit(error.into()))?;
                Ok(view)
            },
            |plan, existing| Ok(existing.equals_surface(*plan)),
            |plan| {
                plan.encode()
                    .map_err(|error| image_encode_error_for_edit(error.into()))
            },
            EditError::InvalidPayload,
        )
    }
}

fn encode_image_for_edit(image: &(impl ImageSource + ?Sized)) -> Result<Vec<u8>, EditError> {
    image
        .view()
        .map_err(EditError::InvalidPayload)?
        .encode()
        .map_err(|error| image_encode_error_for_edit(error.into()))
}

fn image_encode_error_for_edit(error: ImageEncodeError) -> EditError {
    match error {
        ImageEncodeError::InvalidPayload(error) => EditError::InvalidPayload(error),
        ImageEncodeError::AllocationFailed => EditError::AllocationFailed,
        ImageEncodeError::BufferTooSmall { .. } => {
            unreachable!("allocating IMAGE encoder has no caller-provided buffer")
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;
    use alloc::vec;

    use super::*;
    use crate::header::{CHUNK_FILE_HEADER_LEN, VERSION_MINOR};
    use crate::{
        ColorFormat, CompatibilityPolicy, CriticalAssumption, EncodeOptions, ImagePayloadError,
        Layout, OpenOptions, PayloadOrigin, RawChunkPolicy, RawTypePolicy, Reader,
        RelocationAssumption, ReservedBitsPolicy, TrailingBytesPolicy, crc32, encode_chunks,
    };

    const FORMATS: [ColorFormat; 16] = [
        ColorFormat::I1,
        ColorFormat::I2,
        ColorFormat::I4,
        ColorFormat::I8,
        ColorFormat::A1,
        ColorFormat::A2,
        ColorFormat::A4,
        ColorFormat::A8,
        ColorFormat::L8,
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB565A8,
        ColorFormat::RGB888,
        ColorFormat::XRGB8888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ];

    fn id(counter: u32) -> ChunkId {
        ChunkId::new(counter)
    }

    fn borrowed_asset<'a>(
        width: u32,
        height: u32,
        format: ColorFormat,
        stride: u32,
        main: &'a [u8],
        extra: Option<&'a [u8]>,
    ) -> ImageAsset<'a> {
        let asset = ImageAsset::new(width, height, format, stride, Cow::Borrowed(main));
        match extra {
            Some(extra) => asset.with_extra(Cow::Borrowed(extra)),
            None => asset,
        }
    }

    fn a8_asset(main: &[u8], width: u32, height: u32) -> ImageAsset<'_> {
        let stride = ColorFormat::A8.minimum_stride(width).unwrap();
        borrowed_asset(width, height, ColorFormat::A8, stride, main, None)
    }

    fn invalid_asset() -> ImageAsset<'static> {
        a8_asset(&[0], 2, 2)
    }

    fn image_file(payload: &[u8], flags: ChunkFlags) -> Vec<u8> {
        encode_chunks(&[(ChunkType::IMAGE.raw(), flags.bits(), payload)])
    }

    fn refresh_chunk_header_crc(source: &mut [u8]) {
        let checksum = crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn payload_offset(source: &[u8], index: usize) -> usize {
        let entry = CHUNK_FILE_HEADER_LEN + index * crate::CHUNK_TABLE_ENTRY_LEN;
        u32::from_le_bytes(source[entry + 4..entry + 8].try_into().unwrap()) as usize
    }

    #[test]
    fn typed_push_round_trips_every_format_from_short_lived_planes() {
        for (index, format) in FORMATS.into_iter().enumerate() {
            let width = 3;
            let height = 2;
            let stride = format.minimum_stride(width).unwrap() + 1;
            let main_len = usize::try_from(stride * height).unwrap();
            let extra_len =
                usize::try_from(format.extra_size(width, height, stride).unwrap()).unwrap();
            let expected_main = vec![index as u8 + 1; main_len];
            let expected_extra = vec![0x80 | index as u8; extra_len];
            let flags = if index % 2 == 0 {
                ChunkFlags::NONE
            } else {
                ChunkFlags::CRITICAL
            };
            let mut document = Document::new();

            let inserted = {
                let main = expected_main.clone();
                let extra = expected_extra.clone();
                let asset = borrowed_asset(
                    width,
                    height,
                    format,
                    stride,
                    &main,
                    (!extra.is_empty()).then_some(extra.as_slice()),
                );
                document.push_image_with_flags(&asset, flags).unwrap()
            };

            assert_eq!(document.primary(), None, "{format:?}");
            let chunk = document.get(inserted).unwrap();
            assert_eq!(chunk.flags(), flags, "{format:?}");
            assert_eq!(chunk.payload_origin(), PayloadOrigin::OWNED, "{format:?}");
            let image = document
                .get(inserted)
                .unwrap()
                .image()
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap();
            assert_eq!(image.width(), width, "{format:?}");
            assert_eq!(image.height(), height, "{format:?}");
            assert_eq!(image.format(), format, "{format:?}");
            assert_eq!(image.stride(), stride, "{format:?}");
            assert_eq!(image.main(), expected_main, "{format:?}");
            assert_eq!(
                image.extra(),
                (!expected_extra.is_empty()).then_some(expected_extra.as_slice()),
                "{format:?}"
            );

            let encoded = document.encode(&EncodeOptions::new()).unwrap();
            let reopened = Reader::open(&encoded).unwrap();
            let chunk = reopened.chunks().next().unwrap();
            assert_eq!(chunk.flags(), flags, "{format:?}");
            let reopened_image = chunk
                .image()
                .unwrap()
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap();
            assert_eq!(reopened_image.main(), expected_main, "{format:?}");
            assert_eq!(
                reopened_image.extra(),
                (!expected_extra.is_empty()).then_some(expected_extra.as_slice()),
                "{format:?}"
            );
        }
    }

    #[test]
    fn typed_query_preserves_borrowed_owned_and_promoted_plane_pointers() {
        let main = [1, 2, 3, 4];
        let mut authored = Document::new();
        authored
            .push_raw(crate::RawChunkInput {
                chunk_type: ChunkType::META,
                flags: ChunkFlags::NONE,
                payload: crate::PayloadInput::Borrowed(b"opaque"),
                policy: RawChunkPolicy {
                    relocation: RelocationAssumption::AssumeRelocatable,
                    critical_semantics: CriticalAssumption::Infer,
                    reserved_flag_bits: ReservedBitsPolicy::Reject,
                },
            })
            .unwrap();
        authored.push_image(&a8_asset(&main, 2, 2)).unwrap();
        let source = authored.encode(&EncodeOptions::new()).unwrap();
        let expected = Reader::open(&source)
            .unwrap()
            .chunks()
            .nth(1)
            .unwrap()
            .image()
            .unwrap()
            .unwrap()
            .raw()
            .unwrap()
            .packed()
            .unwrap();
        let expected_pointer = expected.main().as_ptr();
        let document = Document::open(&source).unwrap();
        let image_id = document
            .chunks_of_type(ChunkType::IMAGE)
            .next()
            .unwrap()
            .id();
        assert_eq!(
            document
                .image(image_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main()
                .as_ptr(),
            expected_pointer
        );

        let owned = source.clone();
        let owned_base = owned.as_ptr() as usize;
        let main_offset = {
            let image = Reader::open(&owned)
                .unwrap()
                .chunks()
                .nth(1)
                .unwrap()
                .image()
                .unwrap()
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap();
            image.main().as_ptr() as usize - owned_base
        };
        let owned_document = Document::from_vec(owned).unwrap();
        let owned_id = owned_document
            .chunks_of_type(ChunkType::IMAGE)
            .next()
            .unwrap()
            .id();
        assert_eq!(
            owned_document
                .image(owned_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main()
                .as_ptr() as usize,
            owned_base + main_offset
        );

        let borrowed_payload = a8_asset(&main, 2, 2).encode_payload().unwrap();
        let owned_payload = borrowed_payload.clone();
        let owned_payload_pointer = owned_payload.as_ptr();
        let mut mixed = Document::new();
        let borrowed_id = mixed
            .push_raw(crate::RawChunkInput {
                chunk_type: ChunkType::IMAGE,
                flags: ChunkFlags::NONE,
                payload: crate::PayloadInput::Borrowed(&borrowed_payload),
                policy: RawChunkPolicy::infer(),
            })
            .unwrap();
        let owned_id = mixed
            .push_raw(crate::RawChunkInput {
                chunk_type: ChunkType::IMAGE,
                flags: ChunkFlags::NONE,
                payload: crate::PayloadInput::Owned(owned_payload),
                policy: RawChunkPolicy::infer(),
            })
            .unwrap();
        assert_eq!(
            mixed
                .get(borrowed_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            borrowed_payload.as_ptr()
        );
        assert_eq!(
            mixed
                .get(owned_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            owned_payload_pointer
        );
        assert_eq!(
            mixed
                .image(borrowed_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main(),
            main
        );
        assert_eq!(
            mixed
                .image(owned_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main(),
            main
        );

        let promoted_main = [9, 8, 7, 6];
        let inserted_main = [4, 5, 6, 7];
        let mut promoted = Document::new_flat(a8_asset(&promoted_main, 2, 2)).unwrap();
        assert_eq!(
            promoted.image(id(0)),
            Err(ImageDecodeError::ChunkLayoutRequired)
        );
        let inserted = promoted
            .push_image(&a8_asset(&inserted_main, 2, 2))
            .unwrap();
        let promoted_id = promoted.primary().unwrap();
        assert_eq!(promoted_id, id(0));
        assert_eq!(inserted, id(1));
        assert_eq!(
            promoted
                .image(promoted_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main()
                .as_ptr(),
            promoted_main.as_ptr()
        );
        assert_eq!(
            promoted.get(promoted_id).unwrap().payload_origin(),
            PayloadOrigin::SYNTHESIZED
        );
        assert_eq!(
            promoted.get(inserted).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
    }

    #[test]
    fn typed_query_reports_lookup_type_payload_and_compatibility_boundaries() {
        let source = encode_chunks(&[
            (ChunkType::META.raw(), 0, b"not an image"),
            (ChunkType::IMAGE.raw(), 0, b"short"),
        ]);
        let document = Document::open(&source).unwrap();
        let mut chunks = document.chunks();
        let meta = chunks.next().unwrap();
        let image = chunks.next().unwrap();
        assert_eq!(
            document.image(meta.id()),
            Err(ImageDecodeError::UnexpectedChunkType {
                actual: ChunkType::META,
            })
        );
        assert_eq!(
            document.image(id(99)),
            Err(ImageDecodeError::InvalidChunkId)
        );
        assert!(matches!(
            document.image(image.id()),
            Err(ImageDecodeError::InvalidPayload(ImagePayloadError::Media(
                _
            )))
        ));

        let valid_main = [1, 2, 3, 4];
        let valid_payload = a8_asset(&valid_main, 2, 2).encode_payload().unwrap();
        let mut with_trailing = image_file(&valid_payload, ChunkFlags::NONE);
        with_trailing.extend_from_slice(b"tail");
        let options = OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let trailing = Document::open_with(&with_trailing, &options).unwrap();
        let trailing_id = trailing.chunks().next().unwrap().id();
        assert_eq!(
            trailing
                .image(trailing_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main(),
            valid_main
        );

        with_trailing[5] = VERSION_MINOR + 1;
        refresh_chunk_header_crc(&mut with_trailing);
        let future = Document::open_with(&with_trailing, &options).unwrap();
        let future_id = future.chunks().next().unwrap().id();
        assert_eq!(
            future.image(future_id),
            Err(ImageDecodeError::FutureSemanticsUnsupported)
        );

        let normalized_options =
            options.with_compatibility(CompatibilityPolicy::NormalizeToCurrent);
        let normalized = Document::open_with(&with_trailing, &normalized_options).unwrap();
        let normalized_id = normalized.chunks().next().unwrap().id();
        assert_eq!(
            normalized
                .image(normalized_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main(),
            valid_main
        );
    }

    #[test]
    fn typed_query_keeps_absolute_alignment_checks_for_source_payloads() {
        let main = [1, 2, 3, 4];
        let payload = a8_asset(&main, 2, 2).encode_payload().unwrap();
        let payload = crate::image::test_support::pad_data(payload, 0, 2);
        let mut source = image_file(&payload, ChunkFlags::NONE);
        let entry = CHUNK_FILE_HEADER_LEN;
        let original_payload_offset = payload_offset(&source, 0);
        source.insert(original_payload_offset, 0);
        let shifted_payload_offset = original_payload_offset + 1;
        source[entry + 4..entry + 8]
            .copy_from_slice(&u32::try_from(shifted_payload_offset).unwrap().to_le_bytes());
        let file_size = u32::try_from(source.len()).unwrap();
        source[16..20].copy_from_slice(&file_size.to_le_bytes());
        refresh_chunk_header_crc(&mut source);
        assert_eq!(
            &source[shifted_payload_offset..shifted_payload_offset + payload.len()],
            payload
        );
        let mut document = Document::open(&source).unwrap();
        let image_id = document.chunks().next().unwrap().id();
        let absolute_offset = u32::try_from(
            shifted_payload_offset + crate::image::test_support::data_offset(&payload),
        )
        .unwrap();
        assert_eq!(
            document.image(image_id),
            Err(ImageDecodeError::InvalidPayload(ImagePayloadError::Media(
                crate::image::RawImageViewError::PlaneFileAddressUnaligned {
                    index: 0,
                    absolute_offset,
                    alignment: crate::ByteAlignment::new(4).unwrap()
                }
            )))
        );

        document
            .replace_image(image_id, &a8_asset(&main, 2, 2))
            .unwrap();
        assert!(document.is_dirty());
        assert_eq!(
            document
                .image(image_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main(),
            main
        );
        assert_eq!(
            document.get(image_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
    }

    #[test]
    fn typed_push_preserves_structural_error_priority_and_flat_atomicity() {
        let invalid = invalid_asset();

        let mut chunk = Document::new();
        chunk.next_id = u32::MAX;
        assert_eq!(
            chunk.push_image_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::ChunkIdExhausted)
        );
        assert_eq!(chunk.chunks().len(), 0);
        assert_eq!(chunk.next_id, u32::MAX);

        let flat_main = [1, 2, 3, 4];
        let mut flat = Document::new_flat(a8_asset(&flat_main, 2, 2)).unwrap();
        flat.next_id = u32::MAX - 1;
        assert_eq!(flat.push_image(&invalid), Err(EditError::ChunkIdExhausted));
        assert_eq!(flat.layout(), Layout::Flat);
        assert_eq!(
            flat.flat_image().unwrap().main().as_ptr(),
            flat_main.as_ptr()
        );
        assert_eq!(flat.next_id, u32::MAX - 1);

        let mut reserved = Document::new();
        assert_eq!(
            reserved.push_image_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::ReservedFlagBits { bits: 2 })
        );
        assert_eq!(reserved.chunks().len(), 0);

        let valid_payload = a8_asset(&flat_main, 2, 2).encode_payload().unwrap();
        let mut trailing_source = image_file(&valid_payload, ChunkFlags::NONE);
        trailing_source.extend_from_slice(b"tail");
        let options = OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let mut trailing = Document::open_with(&trailing_source, &options).unwrap();
        assert_eq!(
            trailing.push_image_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::PreservedTrailingBytesReadOnly)
        );

        trailing_source[5] = VERSION_MINOR + 1;
        refresh_chunk_header_crc(&mut trailing_source);
        let mut future = Document::open_with(&trailing_source, &options).unwrap();
        assert_eq!(
            future.push_image_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::FutureSemanticsReadOnly)
        );
    }

    #[test]
    fn canonical_typed_replacement_is_an_allocation_free_storage_noop() {
        let main = [1, 2, 3, 4];
        let payload = a8_asset(&main, 2, 2).encode_payload().unwrap();
        let source = image_file(&payload, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let image_id = document.chunks().next().unwrap().id();
        let before = document.get(image_id).unwrap();
        let payload_pointer = before.payload_bytes().unwrap().as_ptr();
        let main_pointer = document
            .image(image_id)
            .unwrap()
            .raw()
            .unwrap()
            .packed()
            .unwrap()
            .main()
            .as_ptr();

        document
            .replace_image(image_id, &a8_asset(&main, 2, 2))
            .unwrap();

        let after = document.get(image_id).unwrap();
        assert!(!document.is_dirty());
        assert_eq!(after.payload_origin(), PayloadOrigin::ORIGINAL_SOURCE);
        assert_eq!(after.payload_bytes().unwrap().as_ptr(), payload_pointer);
        assert_eq!(
            document
                .image(image_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main()
                .as_ptr(),
            main_pointer
        );
        let finished = document.finish().unwrap();
        assert!(matches!(finished, Cow::Borrowed(_)));
        assert_eq!(finished.as_ref(), source);
    }

    #[test]
    fn typed_replacement_repairs_payloads_updates_primary_and_clears_promotion() {
        let malformed = image_file(b"malformed", ChunkFlags::NONE);
        let mut repaired = Document::open(&malformed).unwrap();
        let repaired_id = repaired.chunks().next().unwrap().id();
        let replacement = [5, 6, 7, 8, 9, 10];
        repaired
            .replace_image(repaired_id, &a8_asset(&replacement, 3, 2))
            .unwrap();
        assert_eq!(repaired.primary(), Some(repaired_id));
        assert_eq!(repaired.primary_hints().width(), 3);
        assert_eq!(repaired.primary_hints().height(), 2);
        assert_eq!(
            repaired
                .image(repaired_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main(),
            replacement
        );
        assert_eq!(
            repaired.get(repaired_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );

        let original = [1, 2, 3, 4];
        let changed = [9, 8, 7, 6, 5, 4];
        let mut exact_promoted = Document::new_flat(a8_asset(&original, 2, 2)).unwrap();
        let exact_promoted_id = exact_promoted.promote_to_chunk().unwrap().unwrap();
        let exact_pointer = exact_promoted
            .image(exact_promoted_id)
            .unwrap()
            .raw()
            .unwrap()
            .packed()
            .unwrap()
            .main()
            .as_ptr();
        exact_promoted
            .replace_image(exact_promoted_id, &a8_asset(&original, 2, 2))
            .unwrap();
        assert_eq!(
            exact_promoted
                .get(exact_promoted_id)
                .unwrap()
                .payload_origin(),
            PayloadOrigin::SYNTHESIZED
        );
        assert_eq!(
            exact_promoted
                .image(exact_promoted_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main()
                .as_ptr(),
            exact_pointer
        );
        let DocumentState::Chunk(exact_chunks) = &exact_promoted.state else {
            panic!("promotion must retain CHUNK layout")
        };
        assert!(exact_chunks.promoted_flat.is_some());

        let mut promoted = Document::new_flat(a8_asset(&original, 2, 2)).unwrap();
        let promoted_id = promoted.promote_to_chunk().unwrap().unwrap();
        let original_pointer = promoted
            .image(promoted_id)
            .unwrap()
            .raw()
            .unwrap()
            .packed()
            .unwrap()
            .main()
            .as_ptr();
        promoted
            .replace_image(promoted_id, &a8_asset(&changed, 3, 2))
            .unwrap();
        assert_eq!(promoted.primary(), Some(promoted_id));
        assert_eq!(promoted.primary_hints().width(), 3);
        assert_eq!(
            promoted.get(promoted_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        assert_ne!(
            promoted
                .image(promoted_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main()
                .as_ptr(),
            original_pointer
        );
        let DocumentState::Chunk(chunks) = &promoted.state else {
            panic!("promotion must retain CHUNK layout")
        };
        assert!(chunks.promoted_flat.is_none());
    }

    #[test]
    fn typed_replace_checks_layout_id_and_type_before_the_asset() {
        let invalid = invalid_asset();
        let flat_main = [1, 2, 3, 4];
        let mut flat = Document::new_flat(a8_asset(&flat_main, 2, 2)).unwrap();
        assert_eq!(
            flat.replace_image(id(0), &invalid),
            Err(EditError::ChunkLayoutRequired)
        );

        let mut document = Document::new();
        assert_eq!(
            document.replace_image(id(0), &invalid),
            Err(EditError::InvalidChunkId)
        );
        let meta = document
            .push_raw(crate::RawChunkInput {
                chunk_type: ChunkType::META,
                flags: ChunkFlags::NONE,
                payload: crate::PayloadInput::Borrowed(b"opaque"),
                policy: RawChunkPolicy {
                    relocation: RelocationAssumption::AssumeRelocatable,
                    critical_semantics: CriticalAssumption::Infer,
                    reserved_flag_bits: ReservedBitsPolicy::Reject,
                },
            })
            .unwrap();
        assert_eq!(
            document.replace_image(meta, &invalid),
            Err(EditError::InvalidChunkType)
        );
        assert_eq!(
            document.get(meta).unwrap().payload_bytes(),
            Some(b"opaque".as_slice())
        );
    }

    #[test]
    fn typed_replace_requires_or_reuses_reserved_bit_preservation_grants() {
        let original = [1, 2, 3, 4];
        let changed = [4, 3, 2, 1];
        let payload = a8_asset(&original, 2, 2).encode_payload().unwrap();
        let source = image_file(&payload, ChunkFlags::from_bits_retain(2));

        let mut exact = Document::open(&source).unwrap();
        let exact_id = exact.chunks().next().unwrap().id();
        exact
            .replace_image(exact_id, &a8_asset(&original, 2, 2))
            .unwrap();
        assert!(!exact.is_dirty());
        assert_eq!(
            exact.get(exact_id).unwrap().payload_origin(),
            PayloadOrigin::ORIGINAL_SOURCE
        );

        let mut rejected = Document::open(&source).unwrap();
        let rejected_id = rejected.chunks().next().unwrap().id();
        assert_eq!(
            rejected.replace_image(rejected_id, &a8_asset(&changed, 2, 2)),
            Err(EditError::ReservedFlagBits { bits: 2 })
        );
        assert!(!rejected.is_dirty());
        assert_eq!(
            rejected.get(rejected_id).unwrap().payload_origin(),
            PayloadOrigin::ORIGINAL_SOURCE
        );

        let policies = [RawTypePolicy {
            chunk_type: ChunkType::IMAGE,
            policy: RawChunkPolicy {
                relocation: RelocationAssumption::Infer,
                critical_semantics: CriticalAssumption::Infer,
                reserved_flag_bits: ReservedBitsPolicy::Preserve,
            },
        }];
        let options = OpenOptions::new().with_raw_type_policies(&policies);
        let mut preserved = Document::open_with(&source, &options).unwrap();
        let preserved_id = preserved.chunks().next().unwrap().id();
        preserved
            .replace_image(preserved_id, &a8_asset(&changed, 2, 2))
            .unwrap();
        let chunk = preserved.get(preserved_id).unwrap();
        assert_eq!(chunk.flags().bits(), 2);
        assert_eq!(chunk.payload_origin(), PayloadOrigin::OWNED);
        assert_eq!(
            preserved
                .image(preserved_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main(),
            changed
        );
    }
}
