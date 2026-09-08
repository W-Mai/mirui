use super::raw::RawChunkPolicy;
use super::{ChunkEdit, ChunkNode, Document, DocumentState, EditError};
use crate::extension::Extension;
use crate::font::Font;
use crate::frames::EncodedFrames;
use crate::meta::Meta;
use crate::palette::Palette;
use crate::scene::Scene;
use crate::{ChunkFlags, ChunkId, ChunkType};

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

    /// Changes the chunk flags after validating its existing encoded payload.
    pub fn set_flags(&mut self, flags: ChunkFlags) -> Result<(), EditError> {
        self.document
            .set_flags_at(self.index, flags, RawChunkPolicy::infer())
    }

    /// Replaces type, flags, payload, and rewrite policy atomically.
    pub fn replace_extension(&mut self, extension: Extension<'source>) -> Result<(), EditError> {
        self.document
            .replace_extension_at(self.index, extension.into_raw())
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

    /// Opens an owned FONT working value for explicit commit.
    pub fn edit_font(self) -> Result<ChunkEdit<'document, 'source, Font>, EditError> {
        let id = self.id();
        let value = self.document.begin_font_edit(id)?;
        Ok(ChunkEdit::new(
            self.document,
            id,
            value,
            Document::replace_font,
        ))
    }

    /// Replaces this chunk with a checked VECTOR payload.
    pub fn replace_vector(&mut self, scene: &Scene) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_vector(id, scene)
    }

    /// Opens an owned VECTOR working value for explicit commit.
    pub fn edit_vector(self) -> Result<ChunkEdit<'document, 'source, Scene>, EditError> {
        let id = self.id();
        let value = self.document.begin_vector_edit(id)?;
        Ok(ChunkEdit::new(
            self.document,
            id,
            value,
            Document::replace_vector,
        ))
    }

    /// Replaces this chunk with a checked META payload.
    pub fn replace_meta(&mut self, meta: &Meta) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_meta(id, meta)
    }

    /// Opens an owned META working value for explicit commit.
    pub fn edit_meta(self) -> Result<ChunkEdit<'document, 'source, Meta>, EditError> {
        let id = self.id();
        let value = self.document.begin_meta_edit(id)?;
        Ok(ChunkEdit::new(
            self.document,
            id,
            value,
            Document::replace_meta,
        ))
    }

    /// Replaces this chunk with a checked PALETTE payload.
    pub fn replace_palette(&mut self, palette: &Palette) -> Result<(), EditError> {
        let id = self.id();
        self.document.replace_palette(id, palette)
    }

    /// Opens an owned PALETTE working value for explicit commit.
    pub fn edit_palette(self) -> Result<ChunkEdit<'document, 'source, Palette>, EditError> {
        let id = self.id();
        let value = self.document.begin_palette_edit(id)?;
        Ok(ChunkEdit::new(
            self.document,
            id,
            value,
            Document::replace_palette,
        ))
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
    fn typed_replacement_and_explicit_edits_stay_scoped_to_the_handle() {
        let mut original = Meta::new();
        original.push(MetaEntry::text("name", "before")).unwrap();
        let mut document = Document::new();
        let id = document.push_meta(&original).unwrap();

        let mut replacement = Meta::new();
        replacement.push(MetaEntry::text("name", "after")).unwrap();
        document
            .get_mut(id)
            .unwrap()
            .replace_meta(&replacement)
            .unwrap();
        let mut edit = document.get_mut(id).unwrap().edit_meta().unwrap();
        edit.push(MetaEntry::text("kind", "asset")).unwrap();
        edit.commit().unwrap();

        let mut discarded = document.get_mut(id).unwrap().edit_meta().unwrap();
        discarded
            .push(MetaEntry::text("discarded", "true"))
            .unwrap();
        drop(discarded);

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
        assert!(view.get_first("discarded").is_none());
    }
}
