use core::convert::Infallible;

use super::payload::resolve_node_payload;
use super::{Compatibility, Document, DocumentChunkRef, DocumentState};
use crate::payload::image::ImagePayloadError;
use crate::{
    ChunkFlags, ChunkId, ChunkType, EditError, Font, FontAccessError, FontError, TryEditError,
};

impl<'a> DocumentChunkRef<'a> {
    /// Returns this chunk as a borrowed FONT view.
    pub fn font(&self) -> Result<crate::FontView<'a>, FontAccessError> {
        if self.chunk_type() != ChunkType::FONT {
            return Err(FontAccessError::UnexpectedChunkType {
                actual: self.chunk_type(),
            });
        }
        if matches!(self.document().compatibility, Compatibility::FutureReadOnly) {
            return Err(FontAccessError::FutureSemanticsUnsupported);
        }
        resolve_node_payload(self.document(), self.node())
            .map_err(font_access_resolution_error)?
            .font_view(&self.document().payload_limits)
            .map_err(Into::into)
    }

    /// Decodes this chunk into an owned FONT value.
    pub fn decode_font(&self) -> Result<Font, FontAccessError> {
        if self.chunk_type() != ChunkType::FONT {
            return Err(FontAccessError::UnexpectedChunkType {
                actual: self.chunk_type(),
            });
        }
        self.font()?;
        let payload = resolve_node_payload(self.document(), self.node())
            .map_err(font_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(FontAccessError::NonContiguousPayload)?;
        Font::decode_with_limits(bytes, &self.document().payload_limits).map_err(Into::into)
    }
}

impl Document<'_> {
    /// Resolves and bounded-decodes one FONT node by its stable identity.
    ///
    /// The document's retained [`PayloadLimits`] profile bounds both owned
    /// FONT metadata and stored bytes. Preserved trailing bytes do not block typed reads.
    pub(super) fn decode_font_at(&self, id: ChunkId) -> Result<Font, FontAccessError> {
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
        payload
            .font_view(&self.payload_limits)
            .map_err(FontAccessError::from)?;
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
            font.preflight(&limits)
                .map_err(font_encode_error_for_edit)?;
            font.encode().map_err(font_encode_error_for_edit)
        })
    }

    /// Replaces one FONT payload without changing its identity or descriptor.
    ///
    /// Exact canonical bytes are a no-op. A semantically equivalent payload
    /// with offset gaps is rewritten into canonical contiguous form.
    pub(super) fn replace_font(&mut self, id: ChunkId, font: &Font) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_typed_owned_with(
            id,
            ChunkType::FONT,
            || {
                font.preflight(&limits)
                    .map_err(font_encode_error_for_edit)?;
                Ok(font)
            },
            |plan, existing| {
                let Some(bytes) = existing.bytes() else {
                    return Ok(false);
                };
                if !plan
                    .matches_payload(bytes)
                    .map_err(font_encode_error_for_edit)?
                {
                    return Ok(false);
                }
                Ok(existing.font_view(&limits).is_ok())
            },
            |plan| plan.encode().map_err(font_encode_error_for_edit),
            font_edit_resolution_error,
        )
    }

    /// Transactionally edits one owned FONT working value.
    ///
    /// Decode, callback, validation, reserve, or encode failure leaves the
    /// document node unchanged. Panics and callback side effects are not caught.
    pub(super) fn edit_font(
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
    pub(super) fn try_edit_font<E>(
        &mut self,
        id: ChunkId,
        edit: impl FnOnce(&mut Font) -> Result<(), E>,
    ) -> Result<(), TryEditError<E>> {
        self.ensure_mutable()?;
        let mut font = self
            .decode_font_at(id)
            .map_err(font_access_error_for_edit)?;
        edit(&mut font).map_err(TryEditError::Callback)?;
        self.replace_font(id, &font).map_err(Into::into)
    }
}

fn font_encode_error_for_edit(error: FontError) -> EditError {
    match error {
        FontError::AllocationFailed => EditError::AllocationFailed,
        error => EditError::InvalidFont(error),
    }
}

fn font_access_resolution_error(_: ImagePayloadError) -> FontAccessError {
    FontAccessError::InvalidPayload(FontError::SizeOverflow)
}

fn font_edit_resolution_error(_: ImagePayloadError) -> EditError {
    font_encode_error_for_edit(FontError::SizeOverflow)
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
        FontAccessError::InvalidPayload(error) => font_encode_error_for_edit(error),
        FontAccessError::AllocationFailed => EditError::AllocationFailed,
    }
}

#[cfg(test)]
mod tests;
