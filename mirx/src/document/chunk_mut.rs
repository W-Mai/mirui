use super::{ChunkNode, Document, DocumentState, PayloadInput, RawChunkPolicy};
use crate::{ChunkFlags, ChunkId, ChunkType, EditError};

/// Mutable handle to one chunk in an editable document.
///
/// The handle keeps the chunk identity resolved while holding an exclusive
/// document borrow, so descriptor and payload edits cannot invalidate its
/// position through a concurrent collection change.
pub struct DocumentChunkMut<'document, 'source> {
    document: &'document mut Document<'source>,
    index: usize,
}

impl<'document, 'source> DocumentChunkMut<'document, 'source> {
    pub(super) fn new(document: &'document mut Document<'source>, index: usize) -> Self {
        Self { document, index }
    }

    pub fn id(&self) -> ChunkId {
        self.node().id
    }

    pub fn chunk_type(&self) -> ChunkType {
        self.node().chunk_type
    }

    pub fn flags(&self) -> ChunkFlags {
        self.node().flags
    }

    /// Changes the chunk type after validating its existing encoded payload.
    pub fn set_type(
        &mut self,
        chunk_type: ChunkType,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.document.set_type_at(self.index, chunk_type, policy)
    }

    /// Changes the chunk flags after validating its existing encoded payload.
    pub fn set_flags(
        &mut self,
        flags: ChunkFlags,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.document.set_flags_at(self.index, flags, policy)
    }

    /// Re-evaluates the rewrite capability of this raw chunk.
    ///
    /// Capability-only changes do not alter encoded output and therefore do
    /// not mark the document dirty. Normalizing reserved flag bits does.
    pub fn set_raw_policy(&mut self, policy: RawChunkPolicy) -> Result<(), EditError> {
        self.document.set_raw_policy_at(self.index, policy)
    }

    /// Replaces the encoded payload without changing the chunk descriptor.
    ///
    /// An exact byte match is a no-op and retains the existing storage and
    /// rewrite capability. Otherwise the replacement is checked with the same
    /// raw-payload policy used by insertion. Because this operation preserves
    /// the chunk descriptor, reserved-bit normalization is rejected when the
    /// existing flags contain reserved bits; use [`Self::set_flags`] to clear
    /// them.
    pub fn replace_raw(
        &mut self,
        payload: PayloadInput<'source>,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.document.replace_raw_at(self.index, payload, policy)
    }

    fn node(&self) -> &ChunkNode<'source> {
        let DocumentState::Chunk(chunks) = &self.document.state else {
            unreachable!("mutable chunk handles are only created for CHUNK documents");
        };
        &chunks.chunks[self.index]
    }
}
