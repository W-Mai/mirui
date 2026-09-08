use alloc::vec::Vec;

use crate::document::raw::{PayloadInput, RawChunkInput};
use crate::document::{Document, EditError};
use crate::{ChunkFlags, ChunkId, ChunkType};

pub use crate::document::raw::{
    CriticalAssumption as Critical, RawChunkPolicy as Policy, RelocationAssumption as Relocation,
    ReservedBitsPolicy as ReservedFlags,
};

/// One opaque chunk value admitted as a complete descriptor and payload.
#[derive(Debug, Eq, PartialEq)]
pub struct Extension<'a> {
    input: RawChunkInput<'a>,
}

impl<'a> Extension<'a> {
    /// Creates an extension that borrows its payload bytes.
    pub fn borrowed(chunk_type: ChunkType, payload: &'a [u8]) -> Self {
        Self {
            input: RawChunkInput::new(chunk_type, PayloadInput::Borrowed(payload)),
        }
    }

    /// Creates an extension that owns its payload bytes.
    pub fn owned(chunk_type: ChunkType, payload: Vec<u8>) -> Self {
        Self {
            input: RawChunkInput::new(chunk_type, PayloadInput::Owned(payload)),
        }
    }

    /// Sets the complete chunk flag set.
    pub const fn with_flags(mut self, flags: ChunkFlags) -> Self {
        self.input.flags = flags;
        self
    }

    /// Sets the assumptions used to admit and later relocate the payload.
    pub const fn with_policy(mut self, policy: Policy) -> Self {
        self.input.policy = policy;
        self
    }

    /// Returns the open chunk type identifier.
    pub const fn chunk_type(&self) -> ChunkType {
        self.input.chunk_type
    }

    /// Returns the complete chunk flag set.
    pub const fn flags(&self) -> ChunkFlags {
        self.input.flags
    }

    /// Borrows the encoded payload bytes.
    pub fn payload(&self) -> &[u8] {
        self.input.payload.as_bytes()
    }

    pub(crate) fn into_raw(self) -> RawChunkInput<'a> {
        self.input
    }
}

impl<'a> Document<'a> {
    /// Appends one opaque extension after validating the complete value.
    pub fn push_extension(&mut self, extension: Extension<'a>) -> Result<ChunkId, EditError> {
        self.push_raw(extension.into_raw())
    }

    /// Inserts one opaque extension immediately before `anchor`.
    pub fn insert_extension_before(
        &mut self,
        anchor: ChunkId,
        extension: Extension<'a>,
    ) -> Result<ChunkId, EditError> {
        self.insert_raw_before(anchor, extension.into_raw())
    }

    /// Inserts one opaque extension immediately after `anchor`.
    pub fn insert_extension_after(
        &mut self,
        anchor: ChunkId,
        extension: Extension<'a>,
    ) -> Result<ChunkId, EditError> {
        self.insert_raw_after(anchor, extension.into_raw())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    const fn understood_policy() -> Policy {
        Policy::infer()
            .with_relocation(Relocation::AssumeRelocatable)
            .with_critical_semantics(Critical::AssumeCriticalUnderstood)
    }

    #[test]
    fn insertion_and_replacement_commit_one_complete_extension() {
        let first = ChunkType::new(0x8001).unwrap();
        let second = ChunkType::new(0x8002).unwrap();
        let mut document = Document::new();
        let id = document
            .push_extension(Extension::borrowed(first, b"old").with_policy(understood_policy()))
            .unwrap();

        document
            .get_mut(id)
            .unwrap()
            .replace_extension(
                Extension::owned(second, vec![1, 2, 3])
                    .with_flags(ChunkFlags::CRITICAL)
                    .with_policy(understood_policy()),
            )
            .unwrap();

        let chunk = document.get(id).unwrap();
        assert_eq!(chunk.chunk_type(), second);
        assert_eq!(chunk.flags(), ChunkFlags::CRITICAL);
        assert_eq!(chunk.payload_bytes(), Some([1, 2, 3].as_slice()));
    }

    #[test]
    fn rejected_replacement_preserves_the_complete_chunk() {
        let chunk_type = ChunkType::new(0x8001).unwrap();
        let mut document = Document::new();
        let id = document
            .push_extension(
                Extension::borrowed(chunk_type, b"old").with_policy(understood_policy()),
            )
            .unwrap();

        assert!(
            document
                .get_mut(id)
                .unwrap()
                .replace_extension(Extension::borrowed(ChunkType::IMAGE, b"invalid"))
                .is_err()
        );
        let chunk = document.get(id).unwrap();
        assert_eq!(chunk.chunk_type(), chunk_type);
        assert_eq!(chunk.flags(), ChunkFlags::NONE);
        assert_eq!(chunk.payload_bytes(), Some(b"old".as_slice()));
    }
}
