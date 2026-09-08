use core::convert::Infallible;

use super::payload::resolve_node_payload;
use super::{Compatibility, Document, DocumentChunkRef, DocumentState};
use crate::palette::{Palette, PaletteDecodeError, PaletteEncodeError, PaletteView};
use crate::payload::image::ImagePayloadError;
use crate::{ChunkFlags, ChunkId, ChunkType, EditError, PaletteAccessError, TryEditError};

impl<'a> DocumentChunkRef<'a> {
    /// Returns this chunk as a borrowed PALETTE view.
    pub fn palette(&self) -> Result<PaletteView<'a>, PaletteAccessError> {
        if self.chunk_type() != ChunkType::PALETTE {
            return Err(PaletteAccessError::UnexpectedChunkType {
                actual: self.chunk_type(),
            });
        }
        if matches!(self.document().compatibility, Compatibility::FutureReadOnly) {
            return Err(PaletteAccessError::FutureSemanticsUnsupported);
        }
        let payload = resolve_node_payload(self.document(), self.node())
            .map_err(palette_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(PaletteAccessError::NonContiguousPayload)?;
        PaletteView::open_payload(bytes, &self.document().payload_limits).map_err(Into::into)
    }
}

impl Document<'_> {
    /// Resolves one validated, zero-allocation PALETTE view by stable identity.
    ///
    /// The document's retained resource profile bounds the color scan.
    /// Preserved trailing bytes do not block reads.
    pub(super) fn palette_at(&self, id: ChunkId) -> Result<PaletteView<'_>, PaletteAccessError> {
        if matches!(self.compatibility, Compatibility::FutureReadOnly) {
            return Err(PaletteAccessError::FutureSemanticsUnsupported);
        }
        let DocumentState::Chunk(chunks) = &self.state else {
            return Err(PaletteAccessError::ChunkLayoutRequired);
        };
        let node = chunks
            .chunks
            .iter()
            .find(|node| node.id == id)
            .ok_or(PaletteAccessError::InvalidChunkId)?;
        if node.chunk_type != ChunkType::PALETTE {
            return Err(PaletteAccessError::UnexpectedChunkType {
                actual: node.chunk_type,
            });
        }

        let payload = resolve_node_payload(self, node).map_err(palette_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(PaletteAccessError::NonContiguousPayload)?;
        PaletteView::open_payload(bytes, &self.payload_limits).map_err(Into::into)
    }

    /// Appends one checked PALETTE payload with no chunk flags.
    pub fn push_palette(&mut self, palette: &Palette) -> Result<ChunkId, EditError> {
        self.push_palette_with_flags(palette, ChunkFlags::NONE)
    }

    /// Appends one checked PALETTE payload with explicit chunk flags.
    ///
    /// Structural gates and retained resource limits are checked before one
    /// canonical payload allocation is committed.
    pub fn push_palette_with_flags(
        &mut self,
        palette: &Palette,
        flags: ChunkFlags,
    ) -> Result<ChunkId, EditError> {
        let limits = self.payload_limits;
        self.push_typed_owned_with(ChunkType::PALETTE, flags, || {
            let plan = palette.payload_plan().map_err(EditError::InvalidPalette)?;
            plan.validate_limits(&limits)
                .map_err(invalid_palette_decode_error)?;
            plan.payload_to_vec().map_err(palette_encode_error_for_edit)
        })
    }

    /// Replaces one PALETTE payload without changing its identity or descriptor.
    ///
    /// Exact canonical bytes are a no-op. Successful changes preserve color
    /// order and duplicates in one owned canonical payload.
    pub(super) fn replace_palette(
        &mut self,
        id: ChunkId,
        palette: &Palette,
    ) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_typed_owned_with(
            id,
            ChunkType::PALETTE,
            || {
                let plan = palette.payload_plan().map_err(EditError::InvalidPalette)?;
                plan.validate_limits(&limits)
                    .map_err(invalid_palette_decode_error)?;
                Ok(plan)
            },
            |plan, existing| {
                Ok(existing
                    .bytes()
                    .is_some_and(|payload| (*plan).equals_payload(payload)))
            },
            |plan| plan.payload_to_vec().map_err(palette_encode_error_for_edit),
            palette_edit_resolution_error,
        )
    }

    /// Transactionally edits one owned PALETTE working value.
    ///
    /// Decode, callback, validation, reserve, or encode failure leaves the
    /// document node unchanged. Panics and callback side effects are not caught.
    pub(super) fn edit_palette(
        &mut self,
        id: ChunkId,
        edit: impl FnOnce(&mut Palette),
    ) -> Result<(), EditError> {
        match self.try_edit_palette(id, |palette| {
            edit(palette);
            Ok::<(), Infallible>(())
        }) {
            Ok(()) => Ok(()),
            Err(TryEditError::Edit(error)) => Err(error),
            Err(TryEditError::Callback(never)) => match never {},
        }
    }

    /// Transactionally edits one PALETTE value with a fallible callback.
    ///
    /// A callback error is returned without post-validation or replacement.
    pub(super) fn try_edit_palette<E>(
        &mut self,
        id: ChunkId,
        edit: impl FnOnce(&mut Palette) -> Result<(), E>,
    ) -> Result<(), TryEditError<E>> {
        self.ensure_mutable()?;
        let limits = self.payload_limits;
        let view = self.palette_at(id).map_err(palette_access_error_for_edit)?;
        let mut palette = Palette::decode_view_with_limits(view, &limits)
            .map_err(invalid_palette_decode_error)?;
        edit(&mut palette).map_err(TryEditError::Callback)?;
        self.replace_palette(id, &palette).map_err(Into::into)
    }
}

fn invalid_palette_decode_error(error: PaletteDecodeError) -> EditError {
    match error {
        PaletteDecodeError::AllocationFailed => EditError::AllocationFailed,
        error => EditError::InvalidPalette(PaletteEncodeError::InvalidPayload(error)),
    }
}

fn palette_encode_error_for_edit(error: PaletteEncodeError) -> EditError {
    match error {
        PaletteEncodeError::AllocationFailed
        | PaletteEncodeError::InvalidPayload(PaletteDecodeError::AllocationFailed) => {
            EditError::AllocationFailed
        }
        error => EditError::InvalidPalette(error),
    }
}

fn palette_access_resolution_error(_: ImagePayloadError) -> PaletteAccessError {
    PaletteAccessError::InvalidPayload(PaletteDecodeError::SizeOverflow)
}

fn palette_edit_resolution_error(_: ImagePayloadError) -> EditError {
    invalid_palette_decode_error(PaletteDecodeError::SizeOverflow)
}

fn palette_access_error_for_edit(error: PaletteAccessError) -> EditError {
    match error {
        PaletteAccessError::FutureSemanticsUnsupported => EditError::FutureSemanticsReadOnly,
        PaletteAccessError::ChunkLayoutRequired => EditError::ChunkLayoutRequired,
        PaletteAccessError::InvalidChunkId => EditError::InvalidChunkId,
        PaletteAccessError::UnexpectedChunkType { .. } => EditError::InvalidChunkType,
        PaletteAccessError::NonContiguousPayload => EditError::NonContiguousPayload {
            chunk_type: ChunkType::PALETTE,
        },
        PaletteAccessError::InvalidPayload(error) => invalid_palette_decode_error(error),
    }
}

#[cfg(test)]
mod tests {
    use alloc::{borrow::Cow, vec, vec::Vec};
    use core::{cell::Cell, mem::size_of};

    use super::*;
    use crate::{
        Color, ColorFormat, CriticalAssumption, EncodeOptions, ImageAsset, OpenOptions,
        PayloadInput, PayloadLimits, PayloadOrigin, RawChunkInput, RawChunkPolicy, RawTypePolicy,
        RelocationAssumption, ReservedBitsPolicy, crc32, encode_chunks,
    };

    fn id(counter: u32) -> ChunkId {
        ChunkId::new(counter)
    }

    fn sample_palette() -> Palette {
        Palette::from_colors(vec![
            Color::rgba(0x10, 0x20, 0x30, 0x40),
            Color::rgba(0xaa, 0xbb, 0xcc, 0xdd),
            Color::rgba(0x10, 0x20, 0x30, 0x40),
        ])
    }

    fn palette_file(payload: &[u8], flags: ChunkFlags) -> Vec<u8> {
        encode_chunks(&[(ChunkType::PALETTE.raw(), flags.bits(), payload)])
    }

    fn explicit_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
    }

    #[test]
    fn typed_push_query_and_reopen_preserve_order_duplicates_and_alpha() {
        let expected = sample_palette();
        let mut document = Document::new();
        let palette_id = document
            .push_palette_with_flags(&expected, ChunkFlags::CRITICAL)
            .unwrap();

        assert_eq!(palette_id, id(0));
        assert_eq!(
            document.get(palette_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        assert_eq!(
            document
                .get(palette_id)
                .unwrap()
                .palette()
                .unwrap()
                .colors()
                .iter()
                .collect::<Vec<_>>(),
            expected.colors
        );

        let encoded = document.encode(&EncodeOptions::new()).unwrap();
        let reopened = Document::open(&encoded).unwrap();
        let reopened_id = reopened.chunks().next().unwrap().id();
        let view = reopened.palette_at(reopened_id).unwrap();
        assert_eq!(view.len(), 3);
        assert_eq!(view.colors().get(0), view.colors().get(2));
        assert_eq!(view.colors().get(1).unwrap().a, 0xdd);
    }

    #[test]
    fn exact_replacement_is_a_storage_noop_and_changed_replacement_keeps_identity() {
        let expected = sample_palette();
        let payload = expected.encode_payload().unwrap();
        let source = palette_file(&payload, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let palette_id = document.chunks().next().unwrap().id();
        let pointer = document
            .get(palette_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr();

        document.replace_palette(palette_id, &expected).unwrap();
        assert!(!document.is_dirty());
        assert_eq!(
            document
                .get(palette_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            pointer
        );

        let mut changed = expected.clone();
        changed.colors[1] = Color::rgba(1, 2, 3, 4);
        document.replace_palette(palette_id, &changed).unwrap();
        assert!(document.is_dirty());
        assert_eq!(document.chunks().next().unwrap().id(), palette_id);
        assert_eq!(
            document.get(palette_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        assert_eq!(
            document.palette_at(palette_id).unwrap().colors().get(1),
            Some(Color::rgba(1, 2, 3, 4))
        );
    }

    #[test]
    fn edits_commit_only_after_callback_and_validation_succeed() {
        let payload = sample_palette().encode_payload().unwrap();
        let source = palette_file(&payload, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let palette_id = document.chunks().next().unwrap().id();

        document
            .edit_palette(palette_id, |palette| {
                palette
                    .insert(1, Color::rgba(0x01, 0x02, 0x03, 0x04))
                    .unwrap();
                palette.move_color(3, 0).unwrap();
            })
            .unwrap();
        assert_eq!(
            document
                .palette_at(palette_id)
                .unwrap()
                .colors()
                .iter()
                .collect::<Vec<_>>(),
            [
                Color::rgba(0x10, 0x20, 0x30, 0x40),
                Color::rgba(0x10, 0x20, 0x30, 0x40),
                Color::rgba(0x01, 0x02, 0x03, 0x04),
                Color::rgba(0xaa, 0xbb, 0xcc, 0xdd),
            ]
        );

        let before = document.encode(&EncodeOptions::new()).unwrap();
        assert_eq!(
            document.try_edit_palette(palette_id, |_| Err::<(), _>("stop")),
            Err(TryEditError::Callback("stop"))
        );
        assert_eq!(document.encode(&EncodeOptions::new()).unwrap(), before);

        let called = Cell::new(false);
        assert_eq!(
            document.edit_palette(id(99), |_| called.set(true)),
            Err(EditError::InvalidChunkId)
        );
        assert!(!called.get());
    }

    #[test]
    fn retained_limits_separate_borrowed_reads_from_owned_edits_and_writes() {
        let expected = sample_palette();
        let payload = expected.encode_payload().unwrap();
        let source = palette_file(&payload, ChunkFlags::NONE);
        let decoded = expected.colors.len() * size_of::<Color>();
        let exact = PayloadLimits::HOST
            .with_max_palette_colors(3)
            .with_max_decoded_bytes(decoded);
        let borrowed_only = exact.with_max_decoded_bytes(0);
        let options = OpenOptions::new().with_payload_limits(borrowed_only);
        let mut document = Document::open_with(&source, &options).unwrap();
        let palette_id = document.chunks().next().unwrap().id();

        assert_eq!(document.palette_at(palette_id).unwrap().len(), 3);
        let called = Cell::new(false);
        assert_eq!(
            document.edit_palette(palette_id, |_| called.set(true)),
            Err(EditError::InvalidPalette(
                PaletteEncodeError::InvalidPayload(PaletteDecodeError::DecodedBytesLimitExceeded {
                    needed: decoded,
                    limit: 0,
                })
            ))
        );
        assert!(!called.get());

        let mut authored = Document::new_with_limits(exact.with_max_palette_colors(2));
        assert_eq!(
            authored.push_palette(&expected),
            Err(EditError::InvalidPalette(
                PaletteEncodeError::InvalidPayload(PaletteDecodeError::TooManyColors {
                    count: 3,
                    limit: 2,
                })
            ))
        );
        assert_eq!(authored.chunks().len(), 0);

        let mut authored = Document::new_with_limits(exact.with_max_decoded_bytes(decoded - 1));
        assert!(matches!(
            authored.push_palette(&expected),
            Err(EditError::InvalidPalette(
                PaletteEncodeError::InvalidPayload(
                    PaletteDecodeError::DecodedBytesLimitExceeded { .. }
                )
            ))
        ));
        assert_eq!(authored.chunks().len(), 0);
    }

    #[test]
    fn raw_inference_recognizes_valid_palette_and_keeps_invalid_payloads_explicit() {
        let payload = sample_palette().encode_payload().unwrap();
        let mut document = Document::new();
        let valid_id = document
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::PALETTE,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&payload),
                policy: RawChunkPolicy::infer(),
            })
            .unwrap();
        assert_eq!(document.palette_at(valid_id).unwrap().len(), 3);

        let mut invalid = payload.clone();
        let last = invalid.len() - 1;
        invalid[last] ^= 0x80;
        assert!(matches!(
            document.push_raw(RawChunkInput {
                chunk_type: ChunkType::PALETTE,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&invalid),
                policy: RawChunkPolicy::infer(),
            }),
            Err(EditError::InvalidPalette(
                PaletteEncodeError::InvalidPayload(PaletteDecodeError::CrcMismatch { .. })
            ))
        ));
        assert_eq!(document.chunks().len(), 1);

        let opaque_id = document
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::PALETTE,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&invalid),
                policy: explicit_policy(),
            })
            .unwrap();
        assert!(matches!(
            document.palette_at(opaque_id),
            Err(PaletteAccessError::InvalidPayload(
                PaletteDecodeError::CrcMismatch { .. }
            ))
        ));
    }

    #[test]
    fn access_errors_and_reserved_flag_rewrites_are_explicit() {
        let pixels = [0u8; 1];
        let flat = Document::new_flat(ImageAsset::new(
            1,
            1,
            ColorFormat::A8,
            ColorFormat::A8.minimum_stride(1).unwrap(),
            Cow::Borrowed(&pixels),
        ))
        .unwrap();
        assert_eq!(
            flat.palette_at(id(0)),
            Err(PaletteAccessError::ChunkLayoutRequired)
        );

        let expected = sample_palette();
        let payload = expected.encode_payload().unwrap();
        let source = palette_file(&payload, ChunkFlags::from_bits_retain(0x0002));
        let mut document = Document::open(&source).unwrap();
        let palette_id = document.chunks().next().unwrap().id();
        assert_eq!(
            document.palette_at(id(99)),
            Err(PaletteAccessError::InvalidChunkId)
        );

        document.replace_palette(palette_id, &expected).unwrap();
        assert!(!document.is_dirty());
        let mut changed = expected.clone();
        changed.colors.push(Color::rgb(1, 2, 3));
        assert_eq!(
            document.replace_palette(palette_id, &changed),
            Err(EditError::ReservedFlagBits { bits: 0x0002 })
        );
        assert!(!document.is_dirty());

        let policies = [RawTypePolicy {
            chunk_type: ChunkType::PALETTE,
            policy: RawChunkPolicy {
                relocation: RelocationAssumption::Infer,
                critical_semantics: CriticalAssumption::Infer,
                reserved_flag_bits: ReservedBitsPolicy::Preserve,
            },
        }];
        let options = OpenOptions::new().with_raw_type_policies(&policies);
        let mut preserving = Document::open_with(&source, &options).unwrap();
        let palette_id = preserving.chunks().next().unwrap().id();
        preserving.replace_palette(palette_id, &changed).unwrap();
        assert_eq!(preserving.get(palette_id).unwrap().flags().bits(), 0x0002);
        assert_eq!(preserving.palette_at(palette_id).unwrap().len(), 4);

        let malformed = {
            let mut bytes = payload.clone();
            bytes[2] = 1;
            let covered_len = bytes.len() - 4;
            let checksum = crc32(&bytes[..covered_len]);
            bytes[covered_len..].copy_from_slice(&checksum.to_le_bytes());
            bytes
        };
        let source = palette_file(&malformed, ChunkFlags::NONE);
        let opaque = Document::open(&source).unwrap();
        let palette_id = opaque.chunks().next().unwrap().id();
        assert_eq!(
            opaque.palette_at(palette_id),
            Err(PaletteAccessError::InvalidPayload(
                PaletteDecodeError::UnknownFlags(1)
            ))
        );

        let mut wrong_type = Document::new();
        let font_id = wrong_type
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::FONT,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Borrowed(b"opaque"),
                policy: explicit_policy(),
            })
            .unwrap();
        assert_eq!(
            wrong_type.palette_at(font_id),
            Err(PaletteAccessError::UnexpectedChunkType {
                actual: ChunkType::FONT,
            })
        );

        let mut future = palette_file(&payload, ChunkFlags::NONE);
        future[5] = crate::VERSION_MINOR + 1;
        let checksum = crc32(&future[..40]);
        future[40..44].copy_from_slice(&checksum.to_le_bytes());
        let future = Document::open(&future).unwrap();
        let palette_id = future.chunks().next().unwrap().id();
        assert_eq!(
            future.palette_at(palette_id),
            Err(PaletteAccessError::FutureSemanticsUnsupported)
        );

        let mut segmented = Document::new_flat(ImageAsset::new(
            1,
            1,
            ColorFormat::A8,
            1,
            Cow::Borrowed(&pixels),
        ))
        .unwrap();
        segmented.push_palette(&expected).unwrap();
        let image_id = segmented.chunks().next().unwrap().id();
        segmented.clear_primary().unwrap();
        segmented
            .set_type(image_id, ChunkType::PALETTE, explicit_policy())
            .unwrap();
        assert_eq!(
            segmented.palette_at(image_id),
            Err(PaletteAccessError::NonContiguousPayload)
        );
    }
}
