use alloc::vec::Vec;

#[cfg(test)]
use super::DocumentState;
use super::payload::resolve_node_payload;
use super::{Document, DocumentChunkRef, EditError};
use crate::payload::image::ImageAssetEncodeError;
use crate::{ChunkFlags, ChunkId, ChunkType, image::ImageAccessError};

#[cfg(test)]
use crate::ImageAsset;
use crate::image::{EncodedImageAsset, ImageRef, ImageSource};

impl<'a> DocumentChunkRef<'a> {
    /// Returns this chunk as a borrowed IMAGE view.
    pub fn image(&self) -> Result<ImageRef<'a>, ImageAccessError> {
        if self.chunk_type() != ChunkType::IMAGE {
            return Err(ImageAccessError::UnexpectedChunkType {
                actual: self.chunk_type(),
            });
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
    /// allocating decoded samples.
    #[cfg(test)]
    pub(super) fn image(&self, id: ChunkId) -> Result<ImageRef<'_>, ImageAccessError> {
        let DocumentState::Chunk(chunks) = &self.state else {
            return Err(ImageAccessError::ChunkLayoutRequired);
        };
        let node = chunks
            .chunks
            .iter()
            .find(|node| node.id == id)
            .ok_or(ImageAccessError::InvalidChunkId)?;
        if node.chunk_type != ChunkType::IMAGE {
            return Err(ImageAccessError::UnexpectedChunkType {
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

fn image_encode_error_for_edit(error: ImageAssetEncodeError) -> EditError {
    match error {
        ImageAssetEncodeError::InvalidPayload(error) => EditError::InvalidPayload(error),
        ImageAssetEncodeError::AllocationFailed => EditError::AllocationFailed,
        ImageAssetEncodeError::BufferTooSmall { .. } => {
            unreachable!("allocating IMAGE encoder has no caller-provided buffer")
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/document/image/tests.rs"]
mod tests;
