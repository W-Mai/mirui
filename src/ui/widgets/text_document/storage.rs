use alloc::vec::Vec;
use core::ops::Range;
use core::ops::{Deref, DerefMut};

use super::{INLINE_CAPACITY, TextEditError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Storage {
    Inline([u8; INLINE_CAPACITY]),
    Owned(Vec<u8>),
}

impl Storage {
    pub(crate) fn prepare(&mut self, capacity: usize) -> Result<(), TextEditError> {
        let maximum = (u32::MAX as usize).min(isize::MAX as usize);
        if capacity > maximum {
            return Err(TextEditError::CapacityLimit { capacity, maximum });
        }
        if capacity <= self.len() {
            return Ok(());
        }
        match self {
            Self::Inline(bytes) => {
                let mut owned = Vec::new();
                owned
                    .try_reserve_exact(capacity)
                    .map_err(|_| TextEditError::ReserveFailed { capacity })?;
                owned.extend_from_slice(bytes);
                owned.resize(capacity, 0);
                *self = Self::Owned(owned);
            }
            Self::Owned(bytes) => {
                bytes
                    .try_reserve_exact(capacity - bytes.len())
                    .map_err(|_| TextEditError::ReserveFailed { capacity })?;
                bytes.resize(capacity, 0);
            }
        }
        Ok(())
    }

    pub(crate) fn text(&self, len: usize) -> &str {
        core::str::from_utf8(&self[..len]).expect("validated text edits preserve UTF-8")
    }

    pub(crate) fn replacement_len(
        &self,
        len: usize,
        capacity: usize,
        range: &Range<usize>,
        text: &str,
    ) -> Result<usize, TextEditError> {
        if range.start > range.end {
            return Err(TextEditError::InvalidRange {
                start: range.start,
                end: range.end,
            });
        }
        let current = self.text(len);
        for offset in [range.start, range.end] {
            if !current.is_char_boundary(offset) {
                return Err(TextEditError::InvalidBoundary { offset });
            }
        }
        let required = (len - range.len()).saturating_add(text.len());
        if required > capacity {
            return Err(TextEditError::Capacity { required, capacity });
        }
        Ok(required)
    }

    pub(crate) fn replace(
        &mut self,
        len: &mut u32,
        capacity: usize,
        range: Range<usize>,
        text: &str,
    ) -> Result<bool, TextEditError> {
        let required = self.replacement_len(*len as usize, capacity, &range, text)?;
        if &self.text(*len as usize)[range.clone()] == text {
            return Ok(false);
        }
        let cursor = range.start + text.len();
        self.copy_within(range.end..*len as usize, cursor);
        self[range.start..cursor].copy_from_slice(text.as_bytes());
        *len = required as u32;
        Ok(true)
    }
}

impl Deref for Storage {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Self::Inline(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

impl DerefMut for Storage {
    fn deref_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Inline(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}
