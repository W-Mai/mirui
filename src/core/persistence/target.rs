use alloc::{borrow::Cow, string::String};
use core::any::TypeId;

use crate::{
    core::{
        model::{ModelHandle, PersistenceTarget},
        reactive::{Signal, SourceId},
    },
    ecs::{
        resource::{ResourceMut, ResourceRef},
        world::WorldId,
    },
};

/// A registered Model, Signal, resource borrow, or non-Model storage key.
///
/// Borrow targets with `&value`. Keys accept static strings, owned `String`, or `Cow<'static, str>`. Conversion retains identity rather than borrowing or copying the value to save.
pub trait SaveTarget: sealed::Sealed {}

mod sealed {
    pub trait Sealed {
        fn into_request(self) -> super::SaveRequest;
    }
}

pub struct SaveRequest(pub(super) SaveKind);

pub(super) enum SaveKind {
    Model(Option<PersistenceTarget>),
    Resource { owner: WorldId, ty: TypeId },
    Signal(SourceId),
    Key(Cow<'static, str>),
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Source {
    Resource(TypeId),
    Signal(SourceId),
    Bytes,
}

impl<H: ModelHandle> SaveTarget for &H {}
impl<H: ModelHandle> sealed::Sealed for &H {
    fn into_request(self) -> SaveRequest {
        SaveRequest(SaveKind::Model(self.access().persistence_target()))
    }
}

impl<T: 'static> SaveTarget for &Signal<T> {}
impl<T: 'static> sealed::Sealed for &Signal<T> {
    fn into_request(self) -> SaveRequest {
        SaveRequest(SaveKind::Signal(self.source_id()))
    }
}

impl<T: 'static> SaveTarget for &ResourceRef<'_, T> {}
impl<T: 'static> sealed::Sealed for &ResourceRef<'_, T> {
    fn into_request(self) -> SaveRequest {
        SaveRequest(SaveKind::Resource {
            owner: self.owner,
            ty: TypeId::of::<T>(),
        })
    }
}

impl<T: 'static> SaveTarget for &ResourceMut<'_, T> {}
impl<T: 'static> sealed::Sealed for &ResourceMut<'_, T> {
    fn into_request(self) -> SaveRequest {
        SaveRequest(SaveKind::Resource {
            owner: self.owner,
            ty: TypeId::of::<T>(),
        })
    }
}

impl SaveTarget for &'static str {}
impl sealed::Sealed for &'static str {
    fn into_request(self) -> SaveRequest {
        SaveRequest(SaveKind::Key(Cow::Borrowed(self)))
    }
}

impl SaveTarget for String {}
impl sealed::Sealed for String {
    fn into_request(self) -> SaveRequest {
        SaveRequest(SaveKind::Key(Cow::Owned(self)))
    }
}

impl SaveTarget for Cow<'static, str> {}
impl sealed::Sealed for Cow<'static, str> {
    fn into_request(self) -> SaveRequest {
        SaveRequest(SaveKind::Key(self))
    }
}
