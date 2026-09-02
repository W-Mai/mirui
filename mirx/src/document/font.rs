use core::{convert::Infallible, mem::size_of};

use super::payload::resolve_node_payload;
use super::{Compatibility, Document, DocumentState};
use crate::payload::image::ImagePayloadError;
use crate::{
    ChunkFlags, ChunkId, ChunkType, EditError, Font, FontAccessError, FontEncodeError,
    FontReadError, GlyphMetric, PayloadLimits, TryEditError,
};

impl Document<'_> {
    /// Resolves and bounded-decodes one FONT node by its stable identity.
    ///
    /// The document's retained [`PayloadLimits`] profile bounds both owned
    /// FONT components. Preserved trailing bytes do not block typed reads.
    pub fn decode_font(&self, id: ChunkId) -> Result<Font, FontAccessError> {
        if matches!(self.compatibility, Compatibility::FutureReadOnly) {
            return Err(FontAccessError::FutureSemanticsUnsupported);
        }
        let DocumentState::Chunk(chunks) = &self.state else {
            return Err(FontAccessError::ChunkLayoutRequired);
        };
        let node = chunks
            .chunks
            .iter()
            .find(|node| node.id == id)
            .ok_or(FontAccessError::InvalidChunkId)?;
        if node.chunk_type != ChunkType::FONT {
            return Err(FontAccessError::UnexpectedChunkType {
                actual: node.chunk_type,
            });
        }

        let payload = resolve_node_payload(self, node).map_err(font_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(FontAccessError::NonContiguousPayload)?;
        Font::decode_with_limits(bytes, &self.payload_limits).map_err(Into::into)
    }

    /// Appends one checked FONT payload with no chunk flags.
    pub fn push_font(&mut self, font: &Font) -> Result<ChunkId, EditError> {
        self.push_font_with_flags(font, ChunkFlags::NONE)
    }

    /// Appends one checked FONT payload with explicit chunk flags.
    ///
    /// Structural gates and the document's retained resource profile are
    /// checked before one canonical payload allocation is committed.
    pub fn push_font_with_flags(
        &mut self,
        font: &Font,
        flags: ChunkFlags,
    ) -> Result<ChunkId, EditError> {
        let limits = self.payload_limits;
        self.push_typed_owned_with(ChunkType::FONT, flags, || {
            let plan = font.payload_plan().map_err(EditError::InvalidFont)?;
            validate_font_limits(font, limits).map_err(invalid_font_read_error)?;
            plan.payload_to_vec().map_err(font_encode_error_for_edit)
        })
    }

    /// Replaces one FONT payload without changing its identity or descriptor.
    ///
    /// Exact canonical bytes are a no-op. A semantically equivalent payload
    /// with offset gaps is rewritten into canonical contiguous form.
    pub fn replace_font(&mut self, id: ChunkId, font: &Font) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_typed_owned_with(
            id,
            ChunkType::FONT,
            || {
                let plan = font.payload_plan().map_err(EditError::InvalidFont)?;
                validate_font_limits(font, limits).map_err(invalid_font_read_error)?;
                Ok(plan)
            },
            |plan, existing| {
                Ok(existing
                    .bytes()
                    .is_some_and(|payload| (*plan).equals_payload(payload)))
            },
            |plan| plan.payload_to_vec().map_err(font_encode_error_for_edit),
            font_edit_resolution_error,
        )
    }

    /// Transactionally edits one owned FONT working value.
    ///
    /// Decode, callback, validation, reserve, or encode failure leaves the
    /// document node unchanged. Panics and callback side effects are not caught.
    pub fn edit_font(
        &mut self,
        id: ChunkId,
        edit: impl FnOnce(&mut Font),
    ) -> Result<(), EditError> {
        match self.try_edit_font(id, |font| {
            edit(font);
            Ok::<(), Infallible>(())
        }) {
            Ok(()) => Ok(()),
            Err(TryEditError::Edit(error)) => Err(error),
            Err(TryEditError::Callback(never)) => match never {},
        }
    }

    /// Transactionally edits one FONT with a fallible caller callback.
    ///
    /// A callback error is returned without post-validation or replacement.
    pub fn try_edit_font<E>(
        &mut self,
        id: ChunkId,
        edit: impl FnOnce(&mut Font) -> Result<(), E>,
    ) -> Result<(), TryEditError<E>> {
        self.ensure_mutable()?;
        let mut font = self.decode_font(id).map_err(font_access_error_for_edit)?;
        edit(&mut font).map_err(TryEditError::Callback)?;
        self.replace_font(id, &font).map_err(Into::into)
    }
}

fn validate_font_limits(font: &Font, limits: PayloadLimits) -> Result<(), FontReadError> {
    let glyph_count = font.atlas.glyph_count;
    if glyph_count > limits.max_font_glyphs() {
        return Err(FontReadError::TooManyGlyphs {
            count: glyph_count,
            limit: limits.max_font_glyphs(),
        });
    }
    let metric_bytes = font
        .metrics
        .len()
        .checked_mul(size_of::<GlyphMetric>())
        .ok_or(FontReadError::SizeOverflow)?;
    let decoded_bytes = metric_bytes
        .checked_add(font.data.len())
        .ok_or(FontReadError::SizeOverflow)?;
    if decoded_bytes > limits.max_decoded_bytes() {
        return Err(FontReadError::DecodedBytesLimitExceeded {
            needed: decoded_bytes,
            limit: limits.max_decoded_bytes(),
        });
    }
    Ok(())
}

fn invalid_font_read_error(error: FontReadError) -> EditError {
    EditError::InvalidFont(FontEncodeError::InvalidPayload(error))
}

fn font_encode_error_for_edit(error: FontEncodeError) -> EditError {
    match error {
        FontEncodeError::AllocationFailed => EditError::AllocationFailed,
        error => EditError::InvalidFont(error),
    }
}

fn font_access_resolution_error(_: ImagePayloadError) -> FontAccessError {
    FontAccessError::InvalidPayload(FontReadError::SizeOverflow)
}

fn font_edit_resolution_error(_: ImagePayloadError) -> EditError {
    invalid_font_read_error(FontReadError::SizeOverflow)
}

fn font_access_error_for_edit(error: FontAccessError) -> EditError {
    match error {
        FontAccessError::FutureSemanticsUnsupported => EditError::FutureSemanticsReadOnly,
        FontAccessError::ChunkLayoutRequired => EditError::ChunkLayoutRequired,
        FontAccessError::InvalidChunkId => EditError::InvalidChunkId,
        FontAccessError::UnexpectedChunkType { .. } => EditError::InvalidChunkType,
        FontAccessError::NonContiguousPayload => EditError::NonContiguousPayload {
            chunk_type: ChunkType::FONT,
        },
        FontAccessError::InvalidPayload(error) => invalid_font_read_error(error),
        FontAccessError::AllocationFailed => EditError::AllocationFailed,
    }
}

#[cfg(test)]
mod tests {
    use alloc::{borrow::Cow, vec, vec::Vec};
    use core::{cell::Cell, mem::size_of};

    use super::*;
    use crate::header::VERSION_MINOR;
    use crate::{
        AtlasHeader, ColorFormat, CompatibilityPolicy, CriticalAssumption, EncodeOptions,
        FONT_CHUNK_HEADER_LEN, FontChunkHeader, FontChunkKind, HEADER_LEN, ImageAsset, Layout,
        METRIC_LEN, OpenOptions, PRIMARY_FORMAT_NONE, PayloadOrigin, PrimaryHints, RawChunkPolicy,
        RawTypePolicy, RelocationAssumption, ReservedBitsPolicy, SUPPORTED_VERSION,
        TrailingBytesPolicy, crc32, encode_chunks, write_header, write_metric,
    };

    fn id(counter: u32) -> ChunkId {
        ChunkId::new(counter)
    }

    fn font(kind: FontChunkKind, bit_depth: u8) -> Font {
        let source_size = 4;
        let bytes_per_glyph =
            (u32::from(source_size) * u32::from(source_size) * u32::from(bit_depth)).div_ceil(8);
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
            metrics: vec![
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
            data: vec![0xa5; usize::try_from(bytes_per_glyph * 2).unwrap()],
        }
    }

    fn font_file(payload: &[u8], flags: ChunkFlags) -> Vec<u8> {
        encode_chunks(&[(ChunkType::FONT.raw(), flags.bits(), payload)])
    }

    fn gapped_payload(font: &Font) -> Vec<u8> {
        let metric_offset = HEADER_LEN + 4;
        let data_offset = metric_offset + font.metrics.len() * METRIC_LEN + 4;
        let mut payload = vec![0; FONT_CHUNK_HEADER_LEN + data_offset + font.data.len()];
        font.chunk_header
            .write(&mut payload[..FONT_CHUNK_HEADER_LEN]);
        let mut atlas = font.atlas;
        atlas.metric_offset = u32::try_from(metric_offset).unwrap();
        atlas.data_offset = u32::try_from(data_offset).unwrap();
        write_header(
            &mut payload[FONT_CHUNK_HEADER_LEN..FONT_CHUNK_HEADER_LEN + HEADER_LEN],
            &atlas,
        );
        for (index, metric) in font.metrics.iter().enumerate() {
            let start = FONT_CHUNK_HEADER_LEN + metric_offset + index * METRIC_LEN;
            write_metric(&mut payload[start..start + METRIC_LEN], metric);
        }
        let data_start = FONT_CHUNK_HEADER_LEN + data_offset;
        payload[data_start..].copy_from_slice(&font.data);
        payload
    }

    fn explicit_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
    }

    fn preserve_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::Infer,
            critical_semantics: CriticalAssumption::Infer,
            reserved_flag_bits: ReservedBitsPolicy::Preserve,
        }
    }

    fn refresh_chunk_header_crc(source: &mut [u8]) {
        let checksum = crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn typed_push_and_query_round_trip_every_supported_font_shape() {
        for (kind, depths) in [
            (FontChunkKind::Grayscale, &[1, 2, 4, 8][..]),
            (FontChunkKind::Sdf, &[4, 8][..]),
        ] {
            for &bit_depth in depths {
                let expected = font(kind, bit_depth);
                let mut document = Document::new();
                let flags = if bit_depth == 8 {
                    ChunkFlags::CRITICAL
                } else {
                    ChunkFlags::NONE
                };
                let font_id = document.push_font_with_flags(&expected, flags).unwrap();

                assert_eq!(document.decode_font(font_id).unwrap(), expected);
                assert_eq!(document.get(font_id).unwrap().flags(), flags);
                assert_eq!(
                    document.get(font_id).unwrap().payload_origin(),
                    PayloadOrigin::OWNED
                );

                let encoded = document.encode(&EncodeOptions::new()).unwrap();
                let reopened = Document::open(&encoded).unwrap();
                let reopened_id = reopened.chunks().next().unwrap().id();
                assert_eq!(reopened.decode_font(reopened_id).unwrap(), expected);
            }
        }
    }

    #[test]
    fn retained_limits_bound_reads_and_typed_writes() {
        let expected = font(FontChunkKind::Sdf, 4);
        let payload = expected.encode_payload().unwrap();
        let source = font_file(&payload, ChunkFlags::NONE);
        let decoded_bytes = expected.metrics.len() * size_of::<GlyphMetric>() + expected.data.len();
        let exact = PayloadLimits::HOST
            .with_max_font_glyphs(2)
            .with_max_decoded_bytes(decoded_bytes);
        let low_glyphs = exact.with_max_font_glyphs(1);

        let exact_options = OpenOptions::new().with_payload_limits(exact);
        let exact_document = Document::open_with(&source, &exact_options).unwrap();
        let font_id = exact_document.chunks().next().unwrap().id();
        assert_eq!(exact_document.payload_limits(), exact);
        assert_eq!(exact_document.decode_font(font_id).unwrap(), expected);

        let low_options = OpenOptions::new().with_payload_limits(low_glyphs);
        let low_document = Document::open_with(&source, &low_options).unwrap();
        let font_id = low_document.chunks().next().unwrap().id();
        assert_eq!(
            low_document.decode_font(font_id),
            Err(FontAccessError::InvalidPayload(
                FontReadError::TooManyGlyphs { count: 2, limit: 1 }
            ))
        );

        let mut authored = Document::new_with_limits(low_glyphs);
        assert_eq!(
            authored.push_font(&expected),
            Err(EditError::InvalidFont(FontEncodeError::InvalidPayload(
                FontReadError::TooManyGlyphs { count: 2, limit: 1 }
            )))
        );
        assert_eq!(authored.chunks().len(), 0);

        let mut authored = Document::new_with_limits(exact);
        let font_id = authored.push_font(&expected).unwrap();
        assert_eq!(authored.decode_font(font_id).unwrap(), expected);
    }

    #[test]
    fn raw_font_inference_uses_the_retained_validation_profile() {
        let expected = font(FontChunkKind::Sdf, 4);
        let payload = expected.encode_payload().unwrap();
        let mut document = Document::new();
        let font_id = document
            .push_raw(crate::RawChunkInput {
                chunk_type: ChunkType::FONT,
                flags: ChunkFlags::CRITICAL,
                payload: crate::PayloadInput::Borrowed(&payload),
                policy: RawChunkPolicy::infer(),
            })
            .unwrap();
        assert_eq!(document.decode_font(font_id).unwrap(), expected);

        let low = PayloadLimits::EMBEDDED.with_max_font_glyphs(1);
        let mut limited = Document::new_with_limits(low);
        assert_eq!(
            limited.push_raw(crate::RawChunkInput {
                chunk_type: ChunkType::FONT,
                flags: ChunkFlags::NONE,
                payload: crate::PayloadInput::Borrowed(&payload),
                policy: RawChunkPolicy::infer(),
            }),
            Err(EditError::InvalidFont(FontEncodeError::InvalidPayload(
                FontReadError::TooManyGlyphs { count: 2, limit: 1 }
            )))
        );
        assert_eq!(limited.chunks().len(), 0);

        let opaque = limited
            .push_raw(crate::RawChunkInput {
                chunk_type: ChunkType::FONT,
                flags: ChunkFlags::NONE,
                payload: crate::PayloadInput::Borrowed(&payload),
                policy: explicit_policy(),
            })
            .unwrap();
        assert_eq!(
            limited.decode_font(opaque),
            Err(FontAccessError::InvalidPayload(
                FontReadError::TooManyGlyphs { count: 2, limit: 1 }
            ))
        );
    }

    #[test]
    fn canonical_replace_is_a_storage_noop_but_gaps_are_compacted() {
        let expected = font(FontChunkKind::Sdf, 4);
        let canonical = expected.encode_payload().unwrap();
        let source = font_file(&canonical, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let font_id = document.chunks().next().unwrap().id();
        let payload_pointer = document
            .get(font_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr();
        let decoded = document.decode_font(font_id).unwrap();

        document.replace_font(font_id, &decoded).unwrap();
        document.edit_font(font_id, |_| {}).unwrap();

        assert!(!document.is_dirty());
        assert_eq!(
            document.get(font_id).unwrap().payload_origin(),
            PayloadOrigin::ORIGINAL_SOURCE
        );
        assert_eq!(
            document
                .get(font_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            payload_pointer
        );

        let gapped = gapped_payload(&expected);
        let source = font_file(&gapped, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let font_id = document.chunks().next().unwrap().id();
        let decoded = document.decode_font(font_id).unwrap();
        document.replace_font(font_id, &decoded).unwrap();
        assert!(document.is_dirty());
        assert_eq!(
            document.get(font_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        assert_eq!(
            document.get(font_id).unwrap().payload_bytes().unwrap(),
            canonical
        );
    }

    #[test]
    fn edits_commit_only_after_callback_and_font_validation_succeed() {
        let expected = font(FontChunkKind::Sdf, 4);
        let source = font_file(&expected.encode_payload().unwrap(), ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let font_id = document.chunks().next().unwrap().id();
        let original_pointer = document
            .get(font_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr();

        assert_eq!(
            document.try_edit_font(font_id, |working| {
                working.data[0] ^= 0xff;
                Err("rejected")
            }),
            Err(TryEditError::Callback("rejected"))
        );
        assert_eq!(
            document
                .get(font_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            original_pointer
        );
        assert!(!document.is_dirty());

        assert!(matches!(
            document.edit_font(font_id, |working| working.metrics.clear()),
            Err(EditError::InvalidFont(
                FontEncodeError::GlyphCountMismatch { .. }
            ))
        ));
        assert_eq!(
            document
                .get(font_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            original_pointer
        );
        assert!(!document.is_dirty());

        document
            .edit_font(font_id, |working| working.data[0] ^= 0xff)
            .unwrap();
        assert!(document.is_dirty());
        assert_eq!(document.decode_font(font_id).unwrap().data[0], 0x5a);
    }

    #[test]
    fn edit_blockers_never_invoke_the_callback() {
        let expected = font(FontChunkKind::Sdf, 4);
        let payload = expected.encode_payload().unwrap();
        let source = font_file(&payload, ChunkFlags::NONE);
        let called = Cell::new(false);

        let main = [1, 2, 3, 4];
        let mut flat = Document::new_flat(ImageAsset::new(
            2,
            2,
            ColorFormat::A8,
            2,
            Cow::Borrowed(&main),
        ))
        .unwrap();
        assert_eq!(
            flat.try_edit_font(id(0), |_| {
                called.set(true);
                Ok::<(), ()>(())
            }),
            Err(TryEditError::Edit(EditError::ChunkLayoutRequired))
        );
        assert!(!called.replace(false));

        let mut document = Document::open(&source).unwrap();
        assert_eq!(
            document.try_edit_font(id(99), |_| {
                called.set(true);
                Ok::<(), ()>(())
            }),
            Err(TryEditError::Edit(EditError::InvalidChunkId))
        );
        assert!(!called.replace(false));

        let mut wrong_type = Document::new();
        let meta = wrong_type
            .push_raw(crate::RawChunkInput {
                chunk_type: ChunkType::META,
                flags: ChunkFlags::NONE,
                payload: crate::PayloadInput::Borrowed(b"meta"),
                policy: explicit_policy(),
            })
            .unwrap();
        assert_eq!(
            wrong_type.try_edit_font(meta, |_| {
                called.set(true);
                Ok::<(), ()>(())
            }),
            Err(TryEditError::Edit(EditError::InvalidChunkType))
        );
        assert!(!called.replace(false));

        let low = PayloadLimits::EMBEDDED.with_max_font_glyphs(1);
        let options = OpenOptions::new().with_payload_limits(low);
        let mut limited = Document::open_with(&source, &options).unwrap();
        let font_id = limited.chunks().next().unwrap().id();
        assert_eq!(
            limited.try_edit_font(font_id, |_| {
                called.set(true);
                Ok::<(), ()>(())
            }),
            Err(TryEditError::Edit(EditError::InvalidFont(
                FontEncodeError::InvalidPayload(FontReadError::TooManyGlyphs {
                    count: 2,
                    limit: 1,
                })
            )))
        );
        assert!(!called.replace(false));

        let mut trailing_source = source.clone();
        trailing_source.extend_from_slice(b"tail");
        let options = OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let mut trailing = Document::open_with(&trailing_source, &options).unwrap();
        let font_id = trailing.chunks().next().unwrap().id();
        assert_eq!(
            trailing.try_edit_font(font_id, |_| {
                called.set(true);
                Ok::<(), ()>(())
            }),
            Err(TryEditError::Edit(
                EditError::PreservedTrailingBytesReadOnly
            ))
        );
        assert!(!called.replace(false));

        trailing_source[5] = VERSION_MINOR + 1;
        refresh_chunk_header_crc(&mut trailing_source);
        let mut future = Document::open_with(&trailing_source, &options).unwrap();
        let font_id = future.chunks().next().unwrap().id();
        assert_eq!(
            future.try_edit_font(font_id, |_| {
                called.set(true);
                Ok::<(), ()>(())
            }),
            Err(TryEditError::Edit(EditError::FutureSemanticsReadOnly))
        );
        assert!(!called.get());
    }

    #[test]
    fn reserved_font_flags_require_a_grant_only_for_changed_payloads() {
        let expected = font(FontChunkKind::Sdf, 4);
        let reserved = ChunkFlags::from_bits_retain(2);
        let source = font_file(&expected.encode_payload().unwrap(), reserved);
        let mut document = Document::open(&source).unwrap();
        let font_id = document.chunks().next().unwrap().id();
        let payload_pointer = document
            .get(font_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr();
        let mut changed = document.decode_font(font_id).unwrap();

        document.replace_font(font_id, &changed).unwrap();
        assert!(!document.is_dirty());
        assert_eq!(document.get(font_id).unwrap().flags(), reserved);
        assert_eq!(
            document
                .get(font_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            payload_pointer
        );

        changed.data[0] ^= 0xff;
        assert_eq!(
            document.replace_font(font_id, &changed),
            Err(EditError::ReservedFlagBits { bits: 2 })
        );
        assert_eq!(
            document.edit_font(font_id, |working| working.data[0] ^= 0xff),
            Err(EditError::ReservedFlagBits { bits: 2 })
        );
        assert!(!document.is_dirty());
        assert_eq!(
            document
                .get(font_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            payload_pointer
        );

        let policies = [RawTypePolicy {
            chunk_type: ChunkType::FONT,
            policy: preserve_policy(),
        }];
        let options = OpenOptions::new().with_raw_type_policies(&policies);
        let mut granted = Document::open_with(&source, &options).unwrap();
        let font_id = granted.chunks().next().unwrap().id();
        granted.replace_font(font_id, &changed).unwrap();
        assert!(granted.is_dirty());
        assert_eq!(granted.get(font_id).unwrap().flags(), reserved);
        assert_eq!(granted.decode_font(font_id).unwrap(), changed);
    }

    #[test]
    fn edit_does_not_call_the_callback_for_an_invalid_existing_font() {
        let source = font_file(b"\0\0\0\0", ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let font_id = document.chunks().next().unwrap().id();
        let called = Cell::new(false);

        assert!(matches!(
            document.edit_font(font_id, |_| called.set(true)),
            Err(EditError::InvalidFont(FontEncodeError::InvalidPayload(
                FontReadError::Truncated { .. }
            )))
        ));
        assert!(!called.get());
        assert!(!document.is_dirty());

        let replacement = font(FontChunkKind::Grayscale, 4);
        document.replace_font(font_id, &replacement).unwrap();
        assert_eq!(document.decode_font(font_id).unwrap(), replacement);
    }

    #[test]
    fn font_access_reports_container_identity_type_and_payload_boundaries() {
        let expected = font(FontChunkKind::Sdf, 4);
        let payload = expected.encode_payload().unwrap();
        let source = encode_chunks(&[
            (ChunkType::META.raw(), 0, b"meta"),
            (ChunkType::FONT.raw(), 0, b"\0\0\0\0"),
        ]);
        let document = Document::open(&source).unwrap();
        let mut chunks = document.chunks();
        let meta = chunks.next().unwrap().id();
        let malformed = chunks.next().unwrap().id();
        assert_eq!(
            document.decode_font(meta),
            Err(FontAccessError::UnexpectedChunkType {
                actual: ChunkType::META
            })
        );
        assert_eq!(
            document.decode_font(id(99)),
            Err(FontAccessError::InvalidChunkId)
        );
        assert!(matches!(
            document.decode_font(malformed),
            Err(FontAccessError::InvalidPayload(
                FontReadError::Truncated { .. }
            ))
        ));

        let flat_main = [1, 2, 3, 4];
        let flat = Document::new_flat(ImageAsset::new(
            2,
            2,
            ColorFormat::A8,
            2,
            Cow::Borrowed(&flat_main),
        ))
        .unwrap();
        assert_eq!(
            flat.decode_font(id(0)),
            Err(FontAccessError::ChunkLayoutRequired)
        );

        let mut trailing_source = font_file(&payload, ChunkFlags::NONE);
        trailing_source.extend_from_slice(b"tail");
        let options = OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let trailing = Document::open_with(&trailing_source, &options).unwrap();
        let font_id = trailing.chunks().next().unwrap().id();
        assert_eq!(trailing.decode_font(font_id).unwrap(), expected);

        trailing_source[5] = VERSION_MINOR + 1;
        refresh_chunk_header_crc(&mut trailing_source);
        let future = Document::open_with(&trailing_source, &options).unwrap();
        let font_id = future.chunks().next().unwrap().id();
        assert_eq!(
            future.decode_font(font_id),
            Err(FontAccessError::FutureSemanticsUnsupported)
        );

        let normalized_options =
            options.with_compatibility(CompatibilityPolicy::NormalizeToCurrent);
        let normalized = Document::open_with(&trailing_source, &normalized_options).unwrap();
        let font_id = normalized.chunks().next().unwrap().id();
        assert_eq!(normalized.decode_font(font_id).unwrap(), expected);
    }

    #[test]
    fn promoted_image_retyped_as_font_is_explicit_until_replaced() {
        let main = [1, 2, 3, 4];
        let mut document = Document::new_flat(ImageAsset::new(
            2,
            2,
            ColorFormat::A8,
            2,
            Cow::Borrowed(&main),
        ))
        .unwrap();
        let promoted = document.promote_to_chunk().unwrap().unwrap();
        document
            .set_type(promoted, ChunkType::FONT, explicit_policy())
            .unwrap();
        assert_eq!(
            document.decode_font(promoted),
            Err(FontAccessError::NonContiguousPayload)
        );
        assert_eq!(
            document.edit_font(promoted, |_| {}),
            Err(EditError::NonContiguousPayload {
                chunk_type: ChunkType::FONT
            })
        );
        assert_eq!(
            document.primary_hints(),
            PrimaryHints::new(PRIMARY_FORMAT_NONE, 0, 0, 0)
        );

        let replacement = font(FontChunkKind::Sdf, 4);
        document.replace_font(promoted, &replacement).unwrap();
        assert_eq!(document.decode_font(promoted).unwrap(), replacement);
        assert_eq!(
            document.get(promoted).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("replacement must retain CHUNK layout")
        };
        assert!(chunks.promoted_flat.is_none());
        assert_eq!(document.layout(), Layout::Chunk);
    }

    #[test]
    fn structural_edit_errors_precede_invalid_font_validation() {
        let mut invalid = font(FontChunkKind::Sdf, 4);
        invalid.metrics.clear();

        let main = [1, 2, 3, 4];
        let mut flat = Document::new_flat(ImageAsset::new(
            2,
            2,
            ColorFormat::A8,
            2,
            Cow::Borrowed(&main),
        ))
        .unwrap();
        assert_eq!(
            flat.replace_font(id(0), &invalid),
            Err(EditError::ChunkLayoutRequired)
        );

        let mut chunk = Document::new();
        assert_eq!(
            chunk.replace_font(id(0), &invalid),
            Err(EditError::InvalidChunkId)
        );
        let meta = chunk
            .push_raw(crate::RawChunkInput {
                chunk_type: ChunkType::META,
                flags: ChunkFlags::NONE,
                payload: crate::PayloadInput::Borrowed(b"meta"),
                policy: explicit_policy(),
            })
            .unwrap();
        assert_eq!(
            chunk.replace_font(meta, &invalid),
            Err(EditError::InvalidChunkType)
        );
    }

    #[test]
    fn typed_push_preserves_structural_priority_and_flat_atomicity() {
        let mut invalid = font(FontChunkKind::Sdf, 4);
        invalid.metrics.clear();

        let mut chunk = Document::new();
        chunk.next_id = u32::MAX;
        assert_eq!(
            chunk.push_font_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::ChunkIdExhausted)
        );
        assert_eq!(chunk.chunks().len(), 0);
        assert_eq!(chunk.next_id, u32::MAX);

        let main = [1, 2, 3, 4];
        let mut flat = Document::new_flat(ImageAsset::new(
            2,
            2,
            ColorFormat::A8,
            2,
            Cow::Borrowed(&main),
        ))
        .unwrap();
        let main_pointer = flat.flat_image().unwrap().main().as_ptr();
        flat.next_id = u32::MAX - 1;
        assert_eq!(flat.push_font(&invalid), Err(EditError::ChunkIdExhausted));
        assert_eq!(flat.layout(), Layout::Flat);
        assert_eq!(flat.flat_image().unwrap().main().as_ptr(), main_pointer);
        assert_eq!(flat.next_id, u32::MAX - 1);

        let mut reserved = Document::new();
        assert_eq!(
            reserved.push_font_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::ReservedFlagBits { bits: 2 })
        );
        assert_eq!(reserved.chunks().len(), 0);

        let valid = font(FontChunkKind::Sdf, 4);
        let mut source = font_file(&valid.encode_payload().unwrap(), ChunkFlags::NONE);
        source.extend_from_slice(b"tail");
        let options = OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let mut trailing = Document::open_with(&source, &options).unwrap();
        let before_count = trailing.chunks().len();
        assert_eq!(
            trailing.push_font_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::PreservedTrailingBytesReadOnly)
        );
        assert_eq!(trailing.chunks().len(), before_count);

        source[5] = VERSION_MINOR + 1;
        refresh_chunk_header_crc(&mut source);
        let mut future = Document::open_with(&source, &options).unwrap();
        let before_count = future.chunks().len();
        assert_eq!(
            future.push_font_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::FutureSemanticsReadOnly)
        );
        assert_eq!(future.chunks().len(), before_count);
    }
}
