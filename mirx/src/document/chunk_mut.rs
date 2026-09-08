use super::{ChunkNode, Document, DocumentState, PayloadInput, RawChunkPolicy};
use crate::frames::EncodedFrames;
use crate::meta::Meta;
use crate::{ChunkFlags, ChunkId, ChunkType, EditError, Font, Palette, Scene, TryEditError};

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

    /// Replaces this chunk with a checked IMAGE payload.
    pub fn replace_image(
        &mut self,
        image: &(impl crate::image::ImageSource + ?Sized),
    ) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_image(id, image)
    }

    /// Replaces IMAGE storage with already encoded samples after bounded preflight.
    /// Exact canonical bytes are a no-op; changes own one final payload.
    pub fn replace_encoded_image(
        &mut self,
        image: &crate::image::EncodedImageAsset<'_>,
    ) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_encoded_image(id, image)
    }

    /// Replaces this chunk with a checked FONT payload.
    pub fn replace_font(&mut self, font: &Font) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_font(id, font)
    }

    /// Transactionally edits this chunk as an owned FONT value.
    pub fn edit_font(&mut self, edit: impl FnOnce(&mut Font)) -> Result<(), EditError> {
        let id = self.id();
        self.document.edit_font(id, edit)
    }

    /// Transactionally edits this chunk as a FONT with a fallible callback.
    pub fn try_edit_font<E>(
        &mut self,
        edit: impl FnOnce(&mut Font) -> Result<(), E>,
    ) -> Result<(), TryEditError<E>> {
        let id = self.id();
        self.document.try_edit_font(id, edit)
    }

    /// Replaces this chunk with a checked VECTOR payload.
    pub fn replace_vector(&mut self, scene: &Scene) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_vector(id, scene)
    }

    /// Transactionally edits this chunk as an owned VECTOR scene.
    pub fn edit_vector(&mut self, edit: impl FnOnce(&mut Scene)) -> Result<(), EditError> {
        let id = self.id();
        self.document.edit_vector(id, edit)
    }

    /// Transactionally edits this chunk as a VECTOR with a fallible callback.
    pub fn try_edit_vector<E>(
        &mut self,
        edit: impl FnOnce(&mut Scene) -> Result<(), E>,
    ) -> Result<(), TryEditError<E>> {
        let id = self.id();
        self.document.try_edit_vector(id, edit)
    }

    /// Replaces this chunk with a checked META payload.
    pub fn replace_meta(&mut self, meta: &Meta) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_meta(id, meta)
    }

    /// Transactionally edits this chunk as an owned META value.
    pub fn edit_meta(&mut self, edit: impl FnOnce(&mut Meta)) -> Result<(), EditError> {
        let id = self.id();
        self.document.edit_meta(id, edit)
    }

    /// Transactionally edits this chunk as META with a fallible callback.
    pub fn try_edit_meta<E>(
        &mut self,
        edit: impl FnOnce(&mut Meta) -> Result<(), E>,
    ) -> Result<(), TryEditError<E>> {
        let id = self.id();
        self.document.try_edit_meta(id, edit)
    }

    /// Replaces this chunk with a checked PALETTE payload.
    pub fn replace_palette(&mut self, palette: &Palette) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_palette(id, palette)
    }

    /// Transactionally edits this chunk as an owned PALETTE value.
    pub fn edit_palette(&mut self, edit: impl FnOnce(&mut Palette)) -> Result<(), EditError> {
        let id = self.id();
        self.document.edit_palette(id, edit)
    }

    /// Transactionally edits this chunk as PALETTE with a fallible callback.
    pub fn try_edit_palette<E>(
        &mut self,
        edit: impl FnOnce(&mut Palette) -> Result<(), E>,
    ) -> Result<(), TryEditError<E>> {
        let id = self.id();
        self.document.try_edit_palette(id, edit)
    }

    /// Replaces this chunk with a checked FRAMES payload.
    pub fn replace_frames(&mut self, frames: EncodedFrames) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_frames(id, frames)
    }

    fn node(&self) -> &ChunkNode<'source> {
        let DocumentState::Chunk(chunks) = &self.document.state else {
            unreachable!("mutable chunk handles are only created for CHUNK documents");
        };
        &chunks.chunks[self.index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{MetaEntry, MetaValueRef};

    #[test]
    fn typed_replacement_and_callbacks_stay_scoped_to_the_handle() {
        let mut original = Meta::new();
        original.push(MetaEntry::text("name", "before")).unwrap();
        let mut document = Document::new();
        let id = document.push_meta(&original).unwrap();

        let mut replacement = Meta::new();
        replacement.push(MetaEntry::text("name", "after")).unwrap();
        {
            let mut chunk = document.get_mut(id).unwrap();
            chunk.replace_meta(&replacement).unwrap();
            chunk
                .edit_meta(|meta| meta.push(MetaEntry::text("kind", "asset")).unwrap())
                .unwrap();
            assert_eq!(
                chunk.try_edit_meta(|_| Err::<(), _>("stop")),
                Err(TryEditError::Callback("stop"))
            );
        }

        let view = document.meta_at(id).unwrap();
        assert_eq!(view.len(), 2);
        assert_eq!(
            view.get_first("name").unwrap().value,
            MetaValueRef::Text("after")
        );
        assert_eq!(
            view.get_first("kind").unwrap().value,
            MetaValueRef::Text("asset")
        );
    }
}
