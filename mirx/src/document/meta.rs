use core::convert::Infallible;

use super::payload::resolve_node_payload;
use super::{Document, DocumentChunkRef, DocumentState, EditError, TryEditError};
use crate::meta::{Meta, MetaDecodeError, MetaEncodeError, MetaView};
use crate::payload::image::ImagePayloadError;
use crate::{ChunkFlags, ChunkId, ChunkType, meta::MetaAccessError};

impl<'a> DocumentChunkRef<'a> {
    /// Returns this chunk as a borrowed META view.
    pub fn meta(&self) -> Result<MetaView<'a>, MetaAccessError> {
        if self.chunk_type() != ChunkType::META {
            return Err(MetaAccessError::UnexpectedChunkType {
                actual: self.chunk_type(),
            });
        }
        let payload = resolve_node_payload(self.document(), self.node())
            .map_err(meta_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(MetaAccessError::NonContiguousPayload)?;
        MetaView::open_payload(bytes, &self.document().payload_limits).map_err(Into::into)
    }
}

impl Document<'_> {
    /// Resolves one validated, zero-allocation META view by stable identity.
    ///
    /// The document's retained resource profile bounds the entry scan and
    /// aggregate key/value bytes. Preserved trailing bytes do not block reads.
    pub(super) fn meta_at(&self, id: ChunkId) -> Result<MetaView<'_>, MetaAccessError> {
        let DocumentState::Chunk(chunks) = &self.state else {
            return Err(MetaAccessError::ChunkLayoutRequired);
        };
        let node = chunks
            .chunks
            .iter()
            .find(|node| node.id == id)
            .ok_or(MetaAccessError::InvalidChunkId)?;
        if node.chunk_type != ChunkType::META {
            return Err(MetaAccessError::UnexpectedChunkType {
                actual: node.chunk_type,
            });
        }

        let payload = resolve_node_payload(self, node).map_err(meta_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(MetaAccessError::NonContiguousPayload)?;
        MetaView::open_payload(bytes, &self.payload_limits).map_err(Into::into)
    }

    /// Appends one checked META payload with no chunk flags.
    pub fn push_meta(&mut self, meta: &Meta) -> Result<ChunkId, EditError> {
        self.push_meta_with_flags(meta, ChunkFlags::NONE)
    }

    /// Appends one checked META payload with explicit chunk flags.
    ///
    /// Structural gates and retained resource limits are checked before one
    /// canonical payload allocation is committed.
    pub fn push_meta_with_flags(
        &mut self,
        meta: &Meta,
        flags: ChunkFlags,
    ) -> Result<ChunkId, EditError> {
        let limits = self.payload_limits;
        self.push_typed_owned_with(ChunkType::META, flags, || {
            let plan = meta.payload_plan().map_err(EditError::InvalidMeta)?;
            plan.validate_limits(&limits)
                .map_err(invalid_meta_decode_error)?;
            plan.payload_to_vec().map_err(meta_encode_error_for_edit)
        })
    }

    /// Replaces one META payload without changing its identity or descriptor.
    ///
    /// Exact canonical bytes are a no-op. Successful changes preserve entry
    /// order and duplicate keys in one owned canonical payload.
    pub(super) fn replace_meta(&mut self, id: ChunkId, meta: &Meta) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_typed_owned_with(
            id,
            ChunkType::META,
            || {
                let plan = meta.payload_plan().map_err(EditError::InvalidMeta)?;
                plan.validate_limits(&limits)
                    .map_err(invalid_meta_decode_error)?;
                Ok(plan)
            },
            |plan, existing| {
                Ok(existing
                    .bytes()
                    .is_some_and(|payload| (*plan).equals_payload(payload)))
            },
            |plan| plan.payload_to_vec().map_err(meta_encode_error_for_edit),
            meta_edit_resolution_error,
        )
    }

    /// Transactionally edits one owned META working value.
    ///
    /// Decode, callback, validation, reserve, or encode failure leaves the
    /// document node unchanged. Panics and callback side effects are not caught.
    pub(super) fn edit_meta(
        &mut self,
        id: ChunkId,
        edit: impl FnOnce(&mut Meta),
    ) -> Result<(), EditError> {
        match self.try_edit_meta(id, |meta| {
            edit(meta);
            Ok::<(), Infallible>(())
        }) {
            Ok(()) => Ok(()),
            Err(TryEditError::Edit(error)) => Err(error),
            Err(TryEditError::Callback(never)) => match never {},
        }
    }

    /// Transactionally edits one META value with a fallible callback.
    ///
    /// A callback error is returned without post-validation or replacement.
    pub(super) fn try_edit_meta<E>(
        &mut self,
        id: ChunkId,
        edit: impl FnOnce(&mut Meta) -> Result<(), E>,
    ) -> Result<(), TryEditError<E>> {
        self.ensure_mutable()?;
        let limits = self.payload_limits;
        let view = self.meta_at(id).map_err(meta_access_error_for_edit)?;
        let mut meta =
            Meta::decode_view_with_limits(view, &limits).map_err(invalid_meta_decode_error)?;
        edit(&mut meta).map_err(TryEditError::Callback)?;
        self.replace_meta(id, &meta).map_err(Into::into)
    }
}

fn invalid_meta_decode_error(error: MetaDecodeError) -> EditError {
    match error {
        MetaDecodeError::AllocationFailed => EditError::AllocationFailed,
        error => EditError::InvalidMeta(MetaEncodeError::InvalidPayload(error)),
    }
}

fn meta_encode_error_for_edit(error: MetaEncodeError) -> EditError {
    match error {
        MetaEncodeError::AllocationFailed
        | MetaEncodeError::InvalidPayload(MetaDecodeError::AllocationFailed) => {
            EditError::AllocationFailed
        }
        error => EditError::InvalidMeta(error),
    }
}

fn meta_access_resolution_error(_: ImagePayloadError) -> MetaAccessError {
    MetaAccessError::InvalidPayload(MetaDecodeError::SizeOverflow)
}

fn meta_edit_resolution_error(_: ImagePayloadError) -> EditError {
    invalid_meta_decode_error(MetaDecodeError::SizeOverflow)
}

fn meta_access_error_for_edit(error: MetaAccessError) -> EditError {
    match error {
        MetaAccessError::ChunkLayoutRequired => EditError::ChunkLayoutRequired,
        MetaAccessError::InvalidChunkId => EditError::InvalidChunkId,
        MetaAccessError::UnexpectedChunkType { .. } => EditError::InvalidChunkType,
        MetaAccessError::NonContiguousPayload => EditError::NonContiguousPayload {
            chunk_type: ChunkType::META,
        },
        MetaAccessError::InvalidPayload(error) => invalid_meta_decode_error(error),
    }
}

#[cfg(test)]
mod tests {
    use alloc::{borrow::Cow, vec, vec::Vec};
    use core::{cell::Cell, mem::size_of};

    use super::*;
    use crate::document::{
        CriticalAssumption, EncodeOptions, OpenOptions, PayloadInput, PayloadOrigin, RawChunkInput,
        RawChunkPolicy, RawTypePolicy, RelocationAssumption, ReservedBitsPolicy,
    };
    use crate::meta::{MetaEntry, MetaValue, MetaValueRef};
    use crate::{ColorFormat, ImageAsset, PayloadLimits, crc32, encode_chunks};

    fn id(counter: u32) -> ChunkId {
        ChunkId::new(counter)
    }

    fn sample_meta() -> Meta {
        Meta::from_entries(vec![
            MetaEntry::text("tag", "first"),
            MetaEntry::bytes("tag", vec![0x00, 0xff]),
            MetaEntry::extension("vendor", 0x80, 0xa5, vec![1, 2, 3]),
        ])
    }

    fn decoded_bytes(meta: &Meta) -> usize {
        meta.entries.len() * size_of::<MetaEntry>()
            + meta
                .entries
                .iter()
                .map(|entry| {
                    entry.key.len()
                        + match &entry.value {
                            MetaValue::Text(value) => value.len(),
                            MetaValue::Bytes(value) | MetaValue::Extension { bytes: value, .. } => {
                                value.len()
                            }
                        }
                })
                .sum::<usize>()
    }

    fn meta_file(payload: &[u8], flags: ChunkFlags) -> Vec<u8> {
        encode_chunks(&[(ChunkType::META.raw(), flags.bits(), payload)])
    }

    fn explicit_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
    }

    #[test]
    fn typed_push_query_and_reopen_preserve_order_duplicates_and_extensions() {
        let expected = sample_meta();
        let mut document = Document::new();
        let meta_id = document
            .push_meta_with_flags(&expected, ChunkFlags::CRITICAL)
            .unwrap();

        assert_eq!(meta_id, id(0));
        assert_eq!(
            document.get(meta_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        assert_eq!(
            document
                .get(meta_id)
                .unwrap()
                .meta()
                .unwrap()
                .get_all("tag")
                .map(|entry| entry.value)
                .collect::<Vec<_>>(),
            [
                MetaValueRef::Text("first"),
                MetaValueRef::Bytes(&[0x00, 0xff]),
            ]
        );

        let encoded = document.encode(&EncodeOptions::new()).unwrap();
        let reopened = Document::open(&encoded).unwrap();
        let reopened_id = reopened.chunks().next().unwrap().id();
        let view = reopened.meta_at(reopened_id).unwrap();
        assert_eq!(view.len(), 3);
        assert_eq!(
            view.get_last("vendor").unwrap().value,
            MetaValueRef::Extension {
                kind: 0x80,
                flags: 0xa5,
                bytes: &[1, 2, 3],
            }
        );
    }

    #[test]
    fn exact_replacement_is_a_storage_noop_and_changed_replacement_keeps_identity() {
        let expected = sample_meta();
        let payload = expected.encode_payload().unwrap();
        let source = meta_file(&payload, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let meta_id = document.chunks().next().unwrap().id();
        let pointer = document
            .get(meta_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr();

        document.replace_meta(meta_id, &expected).unwrap();
        assert!(!document.is_dirty());
        assert_eq!(
            document
                .get(meta_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            pointer
        );

        let mut changed = expected.clone();
        changed.entries[0] = MetaEntry::text("tag", "changed");
        document.replace_meta(meta_id, &changed).unwrap();
        assert!(document.is_dirty());
        assert_eq!(document.chunks().next().unwrap().id(), meta_id);
        assert_eq!(
            document.get(meta_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        assert_eq!(
            document
                .meta_at(meta_id)
                .unwrap()
                .get_first("tag")
                .unwrap()
                .value,
            MetaValueRef::Text("changed")
        );
    }

    #[test]
    fn edits_commit_only_after_callback_and_validation_succeed() {
        let payload = sample_meta().encode_payload().unwrap();
        let source = meta_file(&payload, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let meta_id = document.chunks().next().unwrap().id();

        document
            .edit_meta(meta_id, |meta| {
                meta.insert(1, MetaEntry::text("tag", "middle")).unwrap();
            })
            .unwrap();
        assert_eq!(
            document
                .meta_at(meta_id)
                .unwrap()
                .get_all("tag")
                .map(|entry| entry.value)
                .collect::<Vec<_>>(),
            [
                MetaValueRef::Text("first"),
                MetaValueRef::Text("middle"),
                MetaValueRef::Bytes(&[0x00, 0xff]),
            ]
        );

        let before = document.encode(&EncodeOptions::new()).unwrap();
        assert_eq!(
            document.try_edit_meta(meta_id, |_| Err::<(), _>("stop")),
            Err(TryEditError::Callback("stop"))
        );
        assert_eq!(document.encode(&EncodeOptions::new()).unwrap(), before);

        assert!(matches!(
            document.edit_meta(meta_id, |meta| {
                meta.entries.push(MetaEntry::text("", "invalid"));
            }),
            Err(EditError::InvalidMeta(MetaEncodeError::InvalidPayload(
                MetaDecodeError::EmptyKey { .. }
            )))
        ));
        assert_eq!(document.encode(&EncodeOptions::new()).unwrap(), before);

        let called = Cell::new(false);
        assert_eq!(
            document.edit_meta(id(99), |_| called.set(true)),
            Err(EditError::InvalidChunkId)
        );
        assert!(!called.get());
    }

    #[test]
    fn retained_limits_separate_borrowed_reads_from_owned_edits() {
        let expected = sample_meta();
        let payload = expected.encode_payload().unwrap();
        let source = meta_file(&payload, ChunkFlags::NONE);
        let meta_bytes = expected
            .entries
            .iter()
            .map(|entry| {
                entry.key.len()
                    + match &entry.value {
                        MetaValue::Text(value) => value.len(),
                        MetaValue::Bytes(value) | MetaValue::Extension { bytes: value, .. } => {
                            value.len()
                        }
                    }
            })
            .sum::<usize>();
        let decoded = decoded_bytes(&expected);
        let exact = PayloadLimits::HOST
            .with_max_meta_entries(3)
            .with_max_meta_bytes(meta_bytes)
            .with_max_decoded_bytes(decoded);
        let borrowed_only = exact.with_max_decoded_bytes(0);
        let options = OpenOptions::new().with_payload_limits(borrowed_only);
        let mut document = Document::open_with(&source, &options).unwrap();
        let meta_id = document.chunks().next().unwrap().id();

        assert_eq!(document.meta_at(meta_id).unwrap().len(), 3);
        let called = Cell::new(false);
        assert_eq!(
            document.edit_meta(meta_id, |_| called.set(true)),
            Err(EditError::InvalidMeta(MetaEncodeError::InvalidPayload(
                MetaDecodeError::DecodedBytesLimitExceeded {
                    needed: decoded,
                    limit: 0,
                }
            )))
        );
        assert!(!called.get());

        let mut authored = Document::new_with_limits(exact.with_max_meta_entries(2));
        assert_eq!(
            authored.push_meta(&expected),
            Err(EditError::InvalidMeta(MetaEncodeError::InvalidPayload(
                MetaDecodeError::TooManyEntries { count: 3, limit: 2 }
            )))
        );
        assert_eq!(authored.chunks().len(), 0);

        let mut authored = Document::new_with_limits(exact.with_max_meta_bytes(meta_bytes - 1));
        assert!(matches!(
            authored.push_meta(&expected),
            Err(EditError::InvalidMeta(MetaEncodeError::InvalidPayload(
                MetaDecodeError::MetaBytesLimitExceeded { .. }
            )))
        ));
        assert_eq!(authored.chunks().len(), 0);
    }

    #[test]
    fn raw_inference_recognizes_valid_meta_and_keeps_invalid_payloads_explicit() {
        let payload = sample_meta().encode_payload().unwrap();
        let mut document = Document::new();
        let valid_id = document
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::META,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&payload),
                policy: RawChunkPolicy::infer(),
            })
            .unwrap();
        assert_eq!(document.meta_at(valid_id).unwrap().len(), 3);

        let mut invalid = payload.clone();
        let last = invalid.len() - 1;
        invalid[last] ^= 0x80;
        assert!(matches!(
            document.push_raw(RawChunkInput {
                chunk_type: ChunkType::META,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&invalid),
                policy: RawChunkPolicy::infer(),
            }),
            Err(EditError::InvalidMeta(MetaEncodeError::InvalidPayload(
                MetaDecodeError::CrcMismatch { .. }
            )))
        ));
        assert_eq!(document.chunks().len(), 1);

        let opaque_id = document
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::META,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&invalid),
                policy: explicit_policy(),
            })
            .unwrap();
        assert!(matches!(
            document.meta_at(opaque_id),
            Err(MetaAccessError::InvalidPayload(
                MetaDecodeError::CrcMismatch { .. }
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
            flat.meta_at(id(0)),
            Err(MetaAccessError::ChunkLayoutRequired)
        );

        let expected = sample_meta();
        let payload = expected.encode_payload().unwrap();
        let source = meta_file(&payload, ChunkFlags::from_bits_retain(0x0002));
        let mut document = Document::open(&source).unwrap();
        let meta_id = document.chunks().next().unwrap().id();
        assert_eq!(
            document.meta_at(id(99)),
            Err(MetaAccessError::InvalidChunkId)
        );

        document.replace_meta(meta_id, &expected).unwrap();
        assert!(!document.is_dirty());
        let mut changed = expected.clone();
        changed.entries.push(MetaEntry::text("new", "value"));
        assert_eq!(
            document.replace_meta(meta_id, &changed),
            Err(EditError::ReservedFlagBits { bits: 0x0002 })
        );
        assert!(!document.is_dirty());

        let policies = [RawTypePolicy {
            chunk_type: ChunkType::META,
            policy: RawChunkPolicy {
                relocation: RelocationAssumption::Infer,
                critical_semantics: CriticalAssumption::Infer,
                reserved_flag_bits: ReservedBitsPolicy::Preserve,
            },
        }];
        let options = OpenOptions::new().with_raw_type_policies(&policies);
        let mut preserving = Document::open_with(&source, &options).unwrap();
        let meta_id = preserving.chunks().next().unwrap().id();
        preserving.replace_meta(meta_id, &changed).unwrap();
        assert_eq!(preserving.get(meta_id).unwrap().flags().bits(), 0x0002);
        assert_eq!(preserving.meta_at(meta_id).unwrap().len(), 4);

        let malformed = {
            let mut bytes = payload.clone();
            bytes[1] = 1;
            let covered_len = bytes.len() - 4;
            let checksum = crc32(&bytes[..covered_len]);
            bytes[covered_len..].copy_from_slice(&checksum.to_le_bytes());
            bytes
        };
        let source = meta_file(&malformed, ChunkFlags::NONE);
        let opaque = Document::open(&source).unwrap();
        let meta_id = opaque.chunks().next().unwrap().id();
        assert_eq!(
            opaque.meta_at(meta_id),
            Err(MetaAccessError::InvalidPayload(
                MetaDecodeError::UnknownFlags(1)
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
            wrong_type.meta_at(font_id),
            Err(MetaAccessError::UnexpectedChunkType {
                actual: ChunkType::FONT,
            })
        );
    }
}
