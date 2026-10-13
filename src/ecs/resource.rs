//! Resource borrows carrying their application's identity.

use core::fmt;
use core::ops::{Deref, DerefMut};

use super::world::WorldId;

/// A shared resource borrow supplied by `res!(T)`.
pub struct ResourceRef<'a, T> {
    value: &'a T,
    pub(crate) owner: WorldId,
}

impl<'a, T> ResourceRef<'a, T> {
    pub(crate) fn new(value: &'a T, owner: WorldId) -> Self {
        Self { value, owner }
    }
}

impl<T> Deref for ResourceRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
}

/// An exclusive resource borrow supplied by `res!(mut T)`.
pub struct ResourceMut<'a, T> {
    value: &'a mut T,
    pub(crate) owner: WorldId,
}

impl<'a, T> ResourceMut<'a, T> {
    pub(crate) fn new(value: &'a mut T, owner: WorldId) -> Self {
        Self { value, owner }
    }
}

impl<T> Deref for ResourceMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
}

impl<T> DerefMut for ResourceMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<T: fmt::Debug> fmt::Debug for ResourceRef<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceRef")
            .field("owner", &self.owner)
            .field("value", self.value)
            .finish()
    }
}

impl<T: fmt::Debug> fmt::Debug for ResourceMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceMut")
            .field("owner", &self.owner)
            .field("value", self.value)
            .finish()
    }
}
