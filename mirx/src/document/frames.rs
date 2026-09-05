use super::payload::resolve_node_payload;
use super::{Compatibility, Document, DocumentState};
use crate::payload::image::ImagePayloadError;
use crate::{
    ChunkFlags, ChunkId, ChunkType, EditError, EncodedFrames, FramesAccessError, FramesView,
};

impl Document<'_> {
    /// Resolves one validated, zero-allocation FRAMES view by stable identity.
    ///
    /// The view retains the source file position so declared input alignment
    /// is checked against the actual chunk placement.
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
        payload
            .frames_view(&self.payload_limits)
            .map_err(Into::into)
    }

    /// Appends one finished FRAMES sequence with no chunk flags.
    pub fn push_frames(&mut self, frames: EncodedFrames) -> Result<ChunkId, EditError> {
        self.push_frames_with_flags(frames, ChunkFlags::NONE)
    }

    /// Appends one finished FRAMES sequence with explicit chunk flags.
    pub fn push_frames_with_flags(
        &mut self,
        frames: EncodedFrames,
        flags: ChunkFlags,
    ) -> Result<ChunkId, EditError> {
        let limits = self.payload_limits;
        self.push_typed_owned_with(ChunkType::FRAMES, flags, || {
            let payload = frames.into_payload();
            validate_frames(&payload, &limits)?;
            Ok(payload)
        })
    }

    /// Replaces one FRAMES sequence without changing chunk identity or flags.
    pub(super) fn replace_frames(
        &mut self,
        id: ChunkId,
        frames: EncodedFrames,
    ) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_typed_owned_with(
            id,
            ChunkType::FRAMES,
            || {
                let payload = frames.into_payload();
                validate_frames(&payload, &limits)?;
                Ok(payload)
            },
            |payload, existing| Ok(existing.bytes().is_some_and(|bytes| bytes == payload)),
            Ok,
            frames_edit_resolution_error,
        )
    }
}

fn validate_frames(payload: &[u8], limits: &crate::PayloadLimits) -> Result<(), EditError> {
    FramesView::open(payload, limits)
        .and_then(FramesView::validate_data)
        .map_err(EditError::InvalidFrames)
}

fn frames_access_resolution_error(_: ImagePayloadError) -> FramesAccessError {
    FramesAccessError::NonContiguousPayload
}

fn frames_edit_resolution_error(_: ImagePayloadError) -> EditError {
    EditError::NonContiguousPayload {
        chunk_type: ChunkType::FRAMES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{ColorDescription, SampleLayout, SurfaceDescriptor};
    use crate::{
        EncodeOptions, FrameEncodingSet, FrameSequence, FramesEncoder, PayloadLimits, Reader,
    };

    fn encoded(value: u8, input_alignment: u32) -> EncodedFrames {
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let profiles = FrameEncodingSet::lossless()
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_reversible_frequency(false)
            .with_delta(false);
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(profiles)
            .unwrap()
            .with_input_alignment(input_alignment)
            .unwrap();
        encoder.push(&[value, value + 1]).unwrap();
        encoder.push(&[value + 2, value + 3]).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn typed_insert_access_and_replacement_share_the_sectioned_contract() {
        let mut document = Document::new();
        let id = document.push_frames(encoded(1, 1)).unwrap();
        let view = document.frames(id).unwrap();
        assert_eq!(view.sequence().frame_count(), 2);
        assert_eq!(view.surface().sample_layout(), SampleLayout::A8);

        document
            .get_mut(id)
            .unwrap()
            .replace_frames(encoded(10, 1))
            .unwrap();
        assert_eq!(document.frames(id).unwrap().sequence().frame_count(), 2);
        assert_eq!(document.get(id).unwrap().id(), id);
    }

    #[test]
    fn writer_places_aligned_frame_data_at_an_absolute_address() {
        let mut document = Document::new();
        document.push_frames(encoded(1, 64)).unwrap();
        let bytes = document.encode(&EncodeOptions::new()).unwrap();
        let reader = Reader::open(&bytes).unwrap();
        reader
            .validate_known_payloads(&PayloadLimits::HOST)
            .unwrap();
        let chunk = reader.chunks().next().unwrap();
        let frames = chunk.frames(&PayloadLimits::HOST).unwrap().unwrap();
        assert_eq!(frames.input_alignment(), Ok(64));
        assert_eq!(
            frames
                .media()
                .validate_file_alignment(chunk.payload_offset(), 64),
            Ok(())
        );
    }

    #[test]
    fn typed_access_rejects_other_chunk_types_and_unknown_ids() {
        let mut document = Document::new();
        let meta = crate::Meta::new();
        let meta_id = document.push_meta(&meta).unwrap();
        assert!(matches!(
            document.frames(meta_id),
            Err(FramesAccessError::UnexpectedChunkType {
                actual: ChunkType::META
            })
        ));
        assert_eq!(
            document.frames(ChunkId::new(99)),
            Err(FramesAccessError::InvalidChunkId)
        );
    }
}
