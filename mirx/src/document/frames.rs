use alloc::vec::Vec;
use core::convert::Infallible;

use super::payload::resolve_node_payload;
use super::{Compatibility, Document, DocumentState};
use crate::payload::image::ImagePayloadError;
use crate::{
    ChunkFlags, ChunkId, ChunkType, EditError, FramesAccessError, FramesAsset, FramesDecodeError,
    FramesEncodeError, FramesView, TryEditError,
};

impl Document<'_> {
    /// Resolves one validated, zero-allocation FRAMES view by stable identity.
    ///
    /// The document's retained resource profile bounds the frame-table scan.
    pub fn frames(&self, id: ChunkId) -> Result<FramesView<'_>, FramesAccessError> {
        if matches!(self.compatibility, Compatibility::FutureReadOnly) {
            return Err(FramesAccessError::FutureSemanticsUnsupported);
        }
        let DocumentState::Chunk(chunks) = &self.state else {
            return Err(FramesAccessError::ChunkLayoutRequired);
        };
        let node = chunks
            .chunks
            .iter()
            .find(|node| node.id == id)
            .ok_or(FramesAccessError::InvalidChunkId)?;
        if node.chunk_type != ChunkType::FRAMES {
            return Err(FramesAccessError::UnexpectedChunkType {
                actual: node.chunk_type,
            });
        }

        let payload = resolve_node_payload(self, node).map_err(frames_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(FramesAccessError::NonContiguousPayload)?;
        FramesView::open_payload(bytes, &self.payload_limits).map_err(Into::into)
    }

    /// Appends one checked FRAMES payload and returns its stable identity.
    pub fn push_frames(
        &mut self,
        flags: ChunkFlags,
        frames: &FramesAsset<'_>,
    ) -> Result<ChunkId, EditError> {
        let limits = self.payload_limits;
        self.push_typed_owned_with(ChunkType::FRAMES, flags, || {
            frames
                .encode_payload_with_limits(limits)
                .map_err(frames_encode_error_for_edit)
        })
    }

    /// Replaces one FRAMES payload without changing its identity or descriptor.
    ///
    /// Exact canonical bytes are a no-op. Successful changes commit one owned
    /// canonical payload after all validation and allocation succeeds.
    pub fn replace_frames(
        &mut self,
        id: ChunkId,
        frames: &FramesAsset<'_>,
    ) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_typed_owned_with(
            id,
            ChunkType::FRAMES,
            || Ok(frames),
            |frames, existing| {
                Ok(existing
                    .bytes()
                    .is_some_and(|payload| frames.matches_payload_with_limits(payload, limits)))
            },
            |frames| {
                frames
                    .encode_payload_with_limits(limits)
                    .map_err(frames_encode_error_for_edit)
            },
            frames_edit_resolution_error,
        )
    }

    /// Transactionally edits one copy-on-write FRAMES working value.
    ///
    /// Decode, callback, validation, reserve, or encode failure leaves the
    /// document node unchanged. Panics and callback side effects are not caught.
    pub fn edit_frames(
        &mut self,
        id: ChunkId,
        edit: impl FnOnce(&mut FramesAsset<'_>),
    ) -> Result<(), EditError> {
        match self.try_edit_frames(id, |frames| {
            edit(frames);
            Ok::<(), Infallible>(())
        }) {
            Ok(()) => Ok(()),
            Err(TryEditError::Edit(error)) => Err(error),
            Err(TryEditError::Callback(never)) => match never {},
        }
    }

    /// Transactionally edits one copy-on-write FRAMES value with a fallible callback.
    ///
    /// Frame records and image planes stay borrowed until the callback requests
    /// mutable access to their partition.
    pub fn try_edit_frames<E>(
        &mut self,
        id: ChunkId,
        edit: impl FnOnce(&mut FramesAsset<'_>) -> Result<(), E>,
    ) -> Result<(), TryEditError<E>> {
        self.ensure_mutable()?;
        let limits = self.payload_limits;
        let view = self.frames(id).map_err(frames_access_error_for_edit)?;
        let mut frames = FramesAsset::from_view(view, limits);
        edit(&mut frames).map_err(TryEditError::Callback)?;

        let unchanged = self
            .get(id)
            .and_then(|chunk| chunk.payload_bytes())
            .is_some_and(|payload| frames.matches_payload_with_limits(payload, limits));
        if unchanged {
            return Ok(());
        }
        let payload = frames
            .encode_payload_with_limits(limits)
            .map_err(frames_encode_error_for_edit)?;
        self.replace_frames_payload(id, payload).map_err(Into::into)
    }

    fn replace_frames_payload(&mut self, id: ChunkId, payload: Vec<u8>) -> Result<(), EditError> {
        self.replace_typed_owned_with(
            id,
            ChunkType::FRAMES,
            || Ok(payload),
            |payload, existing| Ok(existing.bytes().is_some_and(|bytes| bytes == payload)),
            Ok,
            frames_edit_resolution_error,
        )
    }
}

fn frames_encode_error_for_edit(error: FramesEncodeError) -> EditError {
    match error {
        FramesEncodeError::AllocationFailed => EditError::AllocationFailed,
        error => EditError::InvalidFrames(error),
    }
}

fn frames_access_resolution_error(_: ImagePayloadError) -> FramesAccessError {
    FramesAccessError::InvalidPayload(FramesDecodeError::SizeOverflow)
}

fn frames_edit_resolution_error(_: ImagePayloadError) -> EditError {
    EditError::InvalidFrames(FramesEncodeError::InvalidAsset(
        FramesDecodeError::SizeOverflow,
    ))
}

fn frames_access_error_for_edit(error: FramesAccessError) -> EditError {
    match error {
        FramesAccessError::FutureSemanticsUnsupported => EditError::FutureSemanticsReadOnly,
        FramesAccessError::ChunkLayoutRequired => EditError::ChunkLayoutRequired,
        FramesAccessError::InvalidChunkId => EditError::InvalidChunkId,
        FramesAccessError::UnexpectedChunkType { .. } => EditError::InvalidChunkType,
        FramesAccessError::NonContiguousPayload => EditError::NonContiguousPayload {
            chunk_type: ChunkType::FRAMES,
        },
        FramesAccessError::InvalidPayload(error) => {
            EditError::InvalidFrames(FramesEncodeError::InvalidAsset(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AnimationFrames, AnimationSettings, AtlasFrames, ColorFormat, CriticalAssumption,
        EncodeOptions, Frame, ImageAsset, OpenOptions, PayloadInput, PayloadLimits, PayloadOrigin,
        RawChunkInput, RawChunkPolicy, RelocationAssumption, ReservedBitsPolicy, encode_chunks,
    };
    use alloc::{borrow::Cow, vec, vec::Vec};

    fn id(counter: u32) -> ChunkId {
        ChunkId::new(counter)
    }

    fn frame(source_x: u32, duration_ticks: u32) -> Frame {
        Frame {
            source_x,
            source_y: 0,
            width: 1,
            height: 1,
            target_x: source_x,
            target_y: 0,
            duration_ticks,
        }
    }

    fn atlas() -> FramesAsset<'static> {
        let mut first = frame(0, 0);
        first.target_x = 0;
        FramesAsset::Atlas(AtlasFrames::new(
            ImageAsset::new(
                2,
                1,
                ColorFormat::A8,
                ColorFormat::A8.minimum_stride(2).unwrap(),
                Cow::Owned(vec![0x10, 0x20]),
            ),
            vec![first],
        ))
    }

    fn animation() -> FramesAsset<'static> {
        FramesAsset::Animation(AnimationFrames::new(
            ImageAsset::new(
                2,
                1,
                ColorFormat::A8,
                ColorFormat::A8.minimum_stride(2).unwrap(),
                Cow::Owned(vec![0x10, 0x20]),
            ),
            vec![frame(0, 20), frame(1, 30)],
            AnimationSettings::new(2, 1, 1_000, 50).with_play_count(3),
        ))
    }

    fn frames_file(payload: &[u8], flags: ChunkFlags) -> Vec<u8> {
        encode_chunks(&[(ChunkType::FRAMES.raw(), flags.bits(), payload)])
    }

    fn explicit_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
    }

    #[test]
    fn typed_push_query_and_reopen_cover_both_modes() {
        let mut document = Document::new();
        let atlas_id = document
            .push_frames(ChunkFlags::CRITICAL, &atlas())
            .unwrap();
        let animation_id = document
            .push_frames(ChunkFlags::NONE, &animation())
            .unwrap();

        assert_eq!(atlas_id, id(0));
        assert_eq!(document.frames(atlas_id).unwrap().len(), 1);
        assert_eq!(document.frames(animation_id).unwrap().play_count(), 3);
        assert_eq!(
            document.get(animation_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );

        let encoded = document.encode(&EncodeOptions::new()).unwrap();
        let reopened = Document::open(&encoded).unwrap();
        let ids = reopened
            .chunks()
            .map(|chunk| chunk.id())
            .collect::<Vec<_>>();
        assert_eq!(reopened.frames(ids[0]).unwrap().frame(0), atlas().frame(0));
        assert_eq!(reopened.frames(ids[1]).unwrap().timescale_hz(), 1_000);
    }

    #[test]
    fn exact_replacement_is_a_storage_noop_and_changes_keep_identity() {
        let expected = animation();
        let payload = expected.encode_payload().unwrap();
        let source = frames_file(&payload, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let frames_id = document.chunks().next().unwrap().id();
        let pointer = document
            .get(frames_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr();

        document.replace_frames(frames_id, &expected).unwrap();
        assert!(!document.is_dirty());
        assert_eq!(
            document
                .get(frames_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            pointer
        );

        let mut changed = animation();
        let FramesAsset::Animation(animation) = &mut changed else {
            unreachable!()
        };
        animation.set_play_count(7);
        document.replace_frames(frames_id, &changed).unwrap();
        assert_eq!(document.chunks().next().unwrap().id(), frames_id);
        assert_eq!(document.frames(frames_id).unwrap().play_count(), 7);
        assert_eq!(
            document.get(frames_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
    }

    #[test]
    fn edits_materialize_only_requested_partitions_and_commit_atomically() {
        let payload = animation().encode_payload().unwrap();
        let source = frames_file(&payload, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let frames_id = document.chunks().next().unwrap().id();
        let main_pointer = document.frames(frames_id).unwrap().atlas().main().as_ptr();

        document
            .edit_frames(frames_id, |asset| {
                assert_eq!(asset.main().as_ptr(), main_pointer);
                {
                    let FramesAsset::Animation(animation) = asset else {
                        unreachable!()
                    };
                    animation.set_timing(2_000, 80);
                }
                assert_eq!(asset.main().as_ptr(), main_pointer);
            })
            .unwrap();
        let view = document.frames(frames_id).unwrap();
        assert_eq!(view.timescale_hz(), 2_000);
        assert_eq!(view.default_duration_ticks(), 80);

        document
            .edit_frames(frames_id, |asset| {
                let mut replacement = asset.frame(1).unwrap();
                replacement.duration_ticks = 90;
                asset.replace_frame(1, replacement).unwrap();
            })
            .unwrap();
        assert_eq!(
            document
                .frames(frames_id)
                .unwrap()
                .frame(1)
                .unwrap()
                .duration_ticks,
            90
        );
        assert!(document.encode(&EncodeOptions::new()).is_ok());

        let before = document
            .get(frames_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .to_vec();
        assert_eq!(
            document.try_edit_frames(frames_id, |_| Err::<(), _>("stop")),
            Err(TryEditError::Callback("stop"))
        );
        assert_eq!(
            document.get(frames_id).unwrap().payload_bytes().unwrap(),
            before
        );
    }

    #[test]
    fn retained_limits_gate_frame_edits_and_authored_payloads() {
        let expected = animation();
        let payload = expected.encode_payload().unwrap();
        let source = frames_file(&payload, ChunkFlags::NONE);
        let limits = PayloadLimits::HOST
            .with_max_frame_records(2)
            .with_max_decoded_bytes(0);
        let options = OpenOptions::new().with_payload_limits(limits);
        let mut document = Document::open_with(&source, &options).unwrap();
        let frames_id = document.chunks().next().unwrap().id();

        assert_eq!(document.frames(frames_id).unwrap().len(), 2);
        assert!(matches!(
            document.try_edit_frames(frames_id, |asset| { asset.try_frames_mut().map(|_| ()) }),
            Err(TryEditError::Callback(
                crate::FramesMutationError::DecodedBytesLimitExceeded { .. }
            ))
        ));

        let mut authored = Document::new_with_limits(PayloadLimits::HOST.with_max_frame_records(1));
        assert!(matches!(
            authored.push_frames(ChunkFlags::NONE, &expected),
            Err(EditError::InvalidFrames(FramesEncodeError::InvalidAsset(
                FramesDecodeError::TooManyFrames { .. }
            )))
        ));
        assert_eq!(authored.chunks().len(), 0);
    }

    #[test]
    fn raw_inference_and_access_failures_are_explicit() {
        let payload = animation().encode_payload().unwrap();
        let mut document = Document::new();
        let valid_id = document
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::FRAMES,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&payload),
                policy: RawChunkPolicy::infer(),
            })
            .unwrap();
        assert_eq!(document.frames(valid_id).unwrap().len(), 2);

        let mut invalid = payload.clone();
        let last = invalid.len() - 1;
        invalid[last] ^= 0x80;
        assert!(matches!(
            document.push_raw(RawChunkInput {
                chunk_type: ChunkType::FRAMES,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&invalid),
                policy: RawChunkPolicy::infer(),
            }),
            Err(EditError::InvalidFrames(FramesEncodeError::InvalidAsset(
                FramesDecodeError::CrcMismatch { .. }
            )))
        ));

        let opaque_id = document
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::FRAMES,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Borrowed(&invalid),
                policy: explicit_policy(),
            })
            .unwrap();
        assert!(matches!(
            document.frames(opaque_id),
            Err(FramesAccessError::InvalidPayload(
                FramesDecodeError::CrcMismatch { .. }
            ))
        ));

        assert_eq!(
            document.frames(id(99)),
            Err(FramesAccessError::InvalidChunkId)
        );
        assert_eq!(document.frames(valid_id).unwrap().frame(9), None);

        let pixels = [0u8; 1];
        let flat = Document::new_flat(ImageAsset::new(
            1,
            1,
            ColorFormat::A8,
            1,
            Cow::Borrowed(&pixels),
        ))
        .unwrap();
        assert_eq!(
            flat.frames(id(0)),
            Err(FramesAccessError::ChunkLayoutRequired)
        );

        let reserved = frames_file(&payload, ChunkFlags::from_bits_retain(0x0002));
        let mut reserved = Document::open(&reserved).unwrap();
        let frames_id = reserved.chunks().next().unwrap().id();
        let mut changed = animation();
        let FramesAsset::Animation(animation) = &mut changed else {
            unreachable!()
        };
        animation.set_play_count(8);
        assert_eq!(
            reserved.replace_frames(frames_id, &changed),
            Err(EditError::ReservedFlagBits { bits: 0x0002 })
        );
    }
}
