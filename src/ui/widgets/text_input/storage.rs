use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};

use super::{TEXT_INPUT_CAP, TextEditError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Storage {
    Inline([u8; TEXT_INPUT_CAP]),
    Owned(Vec<u8>),
}

impl Storage {
    pub(super) fn prepare(&mut self, capacity: usize) -> Result<(), TextEditError> {
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
