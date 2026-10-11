//! Bounded UTF-8 document data, independent of editor selection and appearance.

use core::ops::Range;

pub(crate) mod storage;
use storage::Storage;

pub(crate) const INLINE_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextEditError {
    InvalidBoundary { offset: usize },
    InvalidRange { start: usize, end: usize },
    ControlCharacter,
    Capacity { required: usize, capacity: usize },
    CapacityLimit { capacity: usize, maximum: usize },
    ReserveFailed { capacity: usize },
    RevisionExhausted,
}

/// UTF-8 document with prepared byte capacity and observable content revisions.
///
/// Register with `App::add_model`, or declare an owned field with `model!(owned TextDocument)`. Selection and composition state belong to individual editors, not the document.
#[crate::model]
#[derive(Clone)]
pub struct TextDocument {
    buffer: Storage,
    len: u32,
    capacity: u32,
    revision: u64,
}

#[crate::model]
impl TextDocument {
    pub fn new() -> Self {
        Self {
            buffer: Storage::Inline([0; INLINE_CAPACITY]),
            len: 0,
            capacity: INLINE_CAPACITY as u32,
            revision: 0,
        }
    }

    /// Read text while its document borrow is alive, without copying it.
    pub fn read<R>(&self, read: impl FnOnce(&str) -> R) -> R {
        read(self.buffer.text(self.len as usize))
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[observe]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[observe]
    pub fn capacity(&self) -> usize {
        self.capacity as usize
    }

    /// Prepare a byte limit without changing content. Failure preserves the previous limit and storage.
    pub fn set_capacity(&mut self, capacity: usize) -> Result<(), TextEditError> {
        if capacity < self.len as usize {
            return Err(TextEditError::Capacity {
                required: self.len as usize,
                capacity,
            });
        }
        self.buffer.prepare(capacity)?;
        self.capacity = capacity as u32;
        Ok(())
    }

    /// Atomically replace a UTF-8 byte range using prepared storage. An unchanged replacement does not advance the revision.
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> Result<(), TextEditError> {
        self.buffer
            .replacement_len(self.len as usize, self.capacity as usize, &range, text)?;
        if &self.buffer.text(self.len as usize)[range.clone()] == text {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(TextEditError::RevisionExhausted)?;
        self.buffer
            .replace(&mut self.len, self.capacity as usize, range, text)?;
        self.revision = revision;
        Ok(())
    }

    pub fn set_text(&mut self, text: &str) -> Result<(), TextEditError> {
        self.replace(0..self.len as usize, text)
    }
}

impl Default for TextDocument {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
