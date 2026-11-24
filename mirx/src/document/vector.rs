use super::payload::resolve_node_payload;
use super::{Document, DocumentChunkRef, DocumentState, EditError};
use crate::payload::image::ImagePayloadError;
use crate::scene::{Scene, VectorAccessError, VectorEncodeError, VectorReadError};
use crate::{ChunkFlags, ChunkId, ChunkType};

#[cfg(test)]
use crate::scene::{CodecError, SceneOp, VectorChunkHeader};

impl<'a> DocumentChunkRef<'a> {
    /// Decodes this chunk into an owned VECTOR scene.
    pub fn decode_vector(&self) -> Result<Scene, VectorAccessError> {
        if self.chunk_type() != ChunkType::VECTOR {
            return Err(VectorAccessError::UnexpectedChunkType {
                actual: self.chunk_type(),
            });
        }
        let payload = resolve_node_payload(self.document(), self.node())
            .map_err(vector_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(VectorAccessError::NonContiguousPayload)?;
        Scene::decode_with_limits(bytes, &self.document().payload_limits).map_err(Into::into)
    }
}

impl Document<'_> {
    /// Resolves and bounded-decodes one VECTOR node by its stable identity.
    ///
    /// The document's retained [`crate::PayloadLimits`] profile bounds every owned
    /// scene component. Preserved trailing bytes do not block typed reads.
    pub(super) fn decode_vector_at(&self, id: ChunkId) -> Result<Scene, VectorAccessError> {
        let DocumentState::Chunk(chunks) = &self.state else {
            return Err(VectorAccessError::ChunkLayoutRequired);
        };
        let node = chunks
            .chunks
            .iter()
            .find(|node| node.id == id)
            .ok_or(VectorAccessError::InvalidChunkId)?;
        if node.chunk_type != ChunkType::VECTOR {
            return Err(VectorAccessError::UnexpectedChunkType {
                actual: node.chunk_type,
            });
        }

        let payload = resolve_node_payload(self, node).map_err(vector_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(VectorAccessError::NonContiguousPayload)?;
        Scene::decode_with_limits(bytes, &self.payload_limits).map_err(Into::into)
    }

    /// Appends one checked VECTOR payload with no chunk flags.
    pub fn push_vector(&mut self, scene: &Scene) -> Result<ChunkId, EditError> {
        self.push_vector_with_flags(scene, ChunkFlags::NONE)
    }

    /// Appends one checked VECTOR payload with explicit chunk flags.
    ///
    /// Structural gates and the document's retained resource profile are
    /// checked before one canonical payload allocation is committed.
    pub fn push_vector_with_flags(
        &mut self,
        scene: &Scene,
        flags: ChunkFlags,
    ) -> Result<ChunkId, EditError> {
        let limits = self.payload_limits;
        self.push_typed_owned_with(ChunkType::VECTOR, flags, || {
            let plan = scene.payload_plan().map_err(EditError::InvalidVector)?;
            scene
                .validate_limits(&limits)
                .map_err(invalid_vector_read_error)?;
            plan.payload_to_vec().map_err(vector_encode_error_for_edit)
        })
    }

    /// Replaces one VECTOR payload without changing its identity or descriptor.
    ///
    /// Typed replacement is an explicit canonical rewrite. The existing payload
    /// is not decoded, so malformed or segmented VECTOR nodes can be repaired.
    pub(super) fn replace_vector(&mut self, id: ChunkId, scene: &Scene) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_typed_owned_with(
            id,
            ChunkType::VECTOR,
            || {
                let plan = scene.payload_plan().map_err(EditError::InvalidVector)?;
                scene
                    .validate_limits(&limits)
                    .map_err(invalid_vector_read_error)?;
                Ok(plan)
            },
            |_, _| Ok(false),
            |plan| plan.payload_to_vec().map_err(vector_encode_error_for_edit),
            vector_edit_resolution_error,
        )
    }

    pub(super) fn begin_vector_edit(&mut self, id: ChunkId) -> Result<Scene, EditError> {
        self.ensure_mutable()?;
        self.decode_vector_at(id)
            .map_err(vector_access_error_for_edit)
    }
}

fn invalid_vector_read_error(error: VectorReadError) -> EditError {
    match error {
        VectorReadError::AllocationFailed => EditError::AllocationFailed,
        error => EditError::InvalidVector(VectorEncodeError::InvalidPayload(error)),
    }
}

fn vector_encode_error_for_edit(error: VectorEncodeError) -> EditError {
    match error {
        VectorEncodeError::AllocationFailed
        | VectorEncodeError::InvalidPayload(VectorReadError::AllocationFailed) => {
            EditError::AllocationFailed
        }
        error => EditError::InvalidVector(error),
    }
}

fn vector_access_resolution_error(_: ImagePayloadError) -> VectorAccessError {
    VectorAccessError::InvalidPayload(VectorReadError::SizeOverflow)
}

fn vector_edit_resolution_error(_: ImagePayloadError) -> EditError {
    invalid_vector_read_error(VectorReadError::SizeOverflow)
}

fn vector_access_error_for_edit(error: VectorAccessError) -> EditError {
    match error {
        VectorAccessError::ChunkLayoutRequired => EditError::ChunkLayoutRequired,
        VectorAccessError::InvalidChunkId => EditError::InvalidChunkId,
        VectorAccessError::UnexpectedChunkType { .. } => EditError::InvalidChunkType,
        VectorAccessError::NonContiguousPayload => EditError::NonContiguousPayload {
            chunk_type: ChunkType::VECTOR,
        },
        VectorAccessError::InvalidPayload(error) => invalid_vector_read_error(error),
        VectorAccessError::AllocationFailed => EditError::AllocationFailed,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/document/vector/tests.rs"]
mod tests;
