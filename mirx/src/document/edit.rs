use core::ops::{Deref, DerefMut};

use super::{Document, EditError};
use crate::ChunkId;

type Commit<'source, T> = fn(&mut Document<'source>, ChunkId, &T) -> Result<(), EditError>;

/// Owned working value for one transactional chunk edit.
#[must_use = "call commit to apply the edit"]
pub struct ChunkEdit<'document, 'source, T> {
    document: &'document mut Document<'source>,
    id: ChunkId,
    value: T,
    commit: Commit<'source, T>,
}

impl<'document, 'source, T> ChunkEdit<'document, 'source, T> {
    pub(crate) fn new(
        document: &'document mut Document<'source>,
        id: ChunkId,
        value: T,
        commit: Commit<'source, T>,
    ) -> Self {
        Self {
            document,
            id,
            value,
            commit,
        }
    }

    /// Validates and atomically applies the working value.
    pub fn commit(self) -> Result<(), EditError> {
        (self.commit)(self.document, self.id, &self.value)
    }
}

impl<T> Deref for ChunkEdit<'_, '_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for ChunkEdit<'_, '_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
