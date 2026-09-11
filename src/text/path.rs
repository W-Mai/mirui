use core::ops::Range;

use crate::ecs::{Entity, World};
use crate::render::path::{Path, PathCmd, PathId, PathRevision, PathStore, PathStoreError};
use crate::types::Fixed;
use crate::ui::dirty::Dirty;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PathDirection {
    #[default]
    Forward,
    Reverse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, crate::Component)]
pub struct TextPath {
    path: PathId,
    subpath: u16,
    start: Fixed,
    end: Option<Fixed>,
    direction: PathDirection,
    seam: Option<Fixed>,
}

impl TextPath {
    pub const fn new(path: PathId) -> Self {
        Self {
            path,
            subpath: 0,
            start: Fixed::ZERO,
            end: None,
            direction: PathDirection::Forward,
            seam: None,
        }
    }

    pub const fn path(&self) -> PathId {
        self.path
    }

    pub const fn subpath(&self) -> u16 {
        self.subpath
    }

    pub const fn start(&self) -> Fixed {
        self.start
    }

    pub const fn end(&self) -> Option<Fixed> {
        self.end
    }

    pub const fn direction(&self) -> PathDirection {
        self.direction
    }

    pub const fn seam(&self) -> Option<Fixed> {
        self.seam
    }

    pub const fn with_subpath(mut self, subpath: u16) -> Self {
        self.subpath = subpath;
        self
    }

    pub const fn with_range(mut self, range: Range<Fixed>) -> Self {
        self.start = range.start;
        self.end = Some(range.end);
        self
    }

    pub const fn with_direction(mut self, direction: PathDirection) -> Self {
        self.direction = direction;
        self
    }

    pub const fn with_seam(mut self, seam: Fixed) -> Self {
        self.seam = Some(seam);
        self
    }
}

impl From<PathId> for TextPath {
    fn from(path: PathId) -> Self {
        Self::new(path)
    }
}

pub(crate) struct TextPathSubscription {
    _inner: crate::render::path::PathSubscription,
}

pub(crate) fn set_text_path(world: &mut World, entity: Entity, path: impl Into<TextPath>) {
    let path = path.into();
    let subscription = world
        .resource_mut::<PathStore>()
        .and_then(|store| store.subscribe(path.path(), entity).ok().flatten())
        .map(|inner| TextPathSubscription { _inner: inner });

    world.insert(entity, path);
    if let Some(subscription) = subscription {
        world.insert(entity, subscription);
    } else {
        world.remove::<TextPathSubscription>(entity);
    }
    world.insert(entity, Dirty);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathAccessError {
    StoreUnavailable,
    Store(PathStoreError),
}

impl From<PathStoreError> for PathAccessError {
    fn from(error: PathStoreError) -> Self {
        Self::Store(error)
    }
}

pub struct PathAccess<'a> {
    world: &'a mut World,
}

impl<'a> PathAccess<'a> {
    pub(crate) const fn new(world: &'a mut World) -> Self {
        Self { world }
    }

    pub fn insert_static(
        &mut self,
        commands: &'static [PathCmd],
    ) -> Result<PathId, PathAccessError> {
        self.store()?.insert_static(commands).map_err(Into::into)
    }

    pub fn insert(&mut self, path: Path) -> Result<PathId, PathAccessError> {
        self.store()?.insert(path).map_err(Into::into)
    }

    pub fn with<R>(
        &self,
        id: PathId,
        inspect: impl FnOnce(&Path) -> R,
    ) -> Result<R, PathAccessError> {
        self.world
            .resource::<PathStore>()
            .ok_or(PathAccessError::StoreUnavailable)?
            .with(id, inspect)
            .map_err(Into::into)
    }

    pub fn edit<R>(
        &mut self,
        id: PathId,
        edit: impl FnOnce(&mut Path) -> R,
    ) -> Result<R, PathAccessError> {
        self.store()?.edit(id, edit).map_err(Into::into)
    }

    pub fn revision(&self, id: PathId) -> Result<Option<PathRevision>, PathAccessError> {
        self.world
            .resource::<PathStore>()
            .ok_or(PathAccessError::StoreUnavailable)?
            .revision(id)
            .map_err(Into::into)
    }

    pub fn remove(&mut self, id: PathId) -> Result<Path, PathAccessError> {
        self.store()?.remove(id).map_err(Into::into)
    }

    fn store(&mut self) -> Result<&mut PathStore, PathAccessError> {
        self.world
            .resource_mut::<PathStore>()
            .ok_or(PathAccessError::StoreUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_id_short_form_uses_documented_defaults() {
        let mut store = PathStore::new(1).unwrap();
        let id = store.insert(Path::new()).unwrap();
        let path = TextPath::from(id);

        assert_eq!(path.path(), id);
        assert_eq!(path.subpath(), 0);
        assert_eq!(path.start(), Fixed::ZERO);
        assert_eq!(path.end(), None);
        assert_eq!(path.direction(), PathDirection::Forward);
        assert_eq!(path.seam(), None);
    }

    #[test]
    fn replacing_text_path_releases_the_old_subscription() {
        let mut world = World::new();
        world.insert_resource(PathStore::new(2).unwrap());
        let widget = world.spawn_empty();
        let first = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::new())
            .unwrap();
        let second = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::new())
            .unwrap();

        set_text_path(&mut world, widget, first);
        set_text_path(&mut world, widget, second);
        world.remove::<Dirty>(widget);

        world
            .resource_mut::<PathStore>()
            .unwrap()
            .edit(first, |_| {})
            .unwrap();
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert!(world.get::<Dirty>(widget).is_none());

        world
            .resource_mut::<PathStore>()
            .unwrap()
            .edit(second, |_| {})
            .unwrap();
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert!(world.get::<Dirty>(widget).is_some());
    }
}
