use core::ops::Range;

use crate::ecs::{Entity, World};
use crate::render::path::{Path, PathCmd, PathId, PathRevision, PathStore, PathStoreError};
use crate::types::Fixed;
#[cfg(test)]
use crate::types::fixed::{from_textflow, to_textflow};
#[cfg(test)]
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
    offset: Fixed,
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
            offset: Fixed::ZERO,
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

    pub const fn offset(&self) -> Fixed {
        self.offset
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

    pub const fn with_offset(mut self, offset: Fixed) -> Self {
        self.offset = offset;
        self
    }

    /// Selects a fixed-width window whose origin advances along the path.
    pub fn with_window(mut self, offset: Fixed, extent: Fixed) -> Self {
        self.start = Fixed::ZERO;
        self.offset = offset;
        self.end = Some(offset + extent);
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

#[cfg(test)]
pub(crate) fn layout_width(
    world: &World,
    text_path: TextPath,
) -> Result<Fixed, crate::text::baseline::PathBaselineError> {
    use crate::text::baseline::{DEFAULT_TOLERANCE, PathBaselineError, PathBaselineResource};

    let store = world
        .resource::<PathStore>()
        .ok_or(PathBaselineError::Unavailable)?;
    let resource = world
        .resource::<PathBaselineResource>()
        .ok_or(PathBaselineError::Unavailable)?;
    resource.with(store, text_path, DEFAULT_TOLERANCE, |baseline| {
        let length = baseline.length();
        let end = text_path.end().unwrap_or(length);
        let start = to_textflow(text_path.start())
            .checked_add(to_textflow(text_path.offset()))
            .map(from_textflow)
            .ok_or(PathBaselineError::InvalidRange)?;
        if text_path.start() < Fixed::ZERO
            || text_path.offset() < Fixed::ZERO
            || end <= start
            || end > length
        {
            Err(PathBaselineError::InvalidRange)
        } else {
            Ok(end - start)
        }
    })?
}

pub(crate) struct TextPathSubscription {
    _inner: crate::render::path::PathSubscription,
}

impl World {
    pub fn paths(&mut self) -> PathAccess<'_> {
        PathAccess::new(self)
    }

    pub fn set_text_path(&mut self, entity: Entity, path: impl Into<TextPath>) {
        let path = path.into();
        if let Some(current) = self.get::<TextPath>(entity).copied() {
            if current == path {
                return;
            }
            if current.path == path.path
                && current.subpath == path.subpath
                && current.direction == path.direction
                && current.seam == path.seam
                && current.end.is_some()
                && path.end.is_some()
                && current.end.unwrap() - current.start - current.offset
                    == path.end.unwrap() - path.start - path.offset
            {
                self.insert(entity, path);
                self.invalidate_visual(entity);
                return;
            }
        }
        let subscription = self
            .resource_mut::<PathStore>()
            .and_then(|store| {
                store
                    .subscribe(path.path(), entity, path.end().is_some())
                    .ok()
                    .flatten()
            })
            .map(|inner| TextPathSubscription { _inner: inner });

        self.insert(entity, path);
        if let Some(subscription) = subscription {
            self.insert(entity, subscription);
        } else {
            self.remove::<TextPathSubscription>(entity);
        }
        self.invalidate(entity);
    }
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
        assert_eq!(path.offset(), Fixed::ZERO);
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

        world.set_text_path(widget, first);
        world.set_text_path(widget, second);
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

    #[test]
    fn layout_width_uses_the_selected_baseline_range() {
        let mut world = World::new();
        world.insert_resource(PathStore::new(1).unwrap());
        world.insert_resource(crate::text::baseline::PathBaselineResource::default());
        let id = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::from_owned(alloc::vec![
                PathCmd::MoveTo(crate::types::Point::new(0, 0)),
                PathCmd::LineTo(crate::types::Point::new(100, 0)),
            ]))
            .unwrap();
        let path = TextPath::new(id)
            .with_range(Fixed::from_int(20)..Fixed::from_int(80))
            .with_offset(Fixed::from_int(12));

        assert_eq!(layout_width(&world, path), Ok(Fixed::from_int(48)));
    }

    #[test]
    fn path_window_keeps_extent_constant_while_offset_advances() {
        let mut store = PathStore::new(1).unwrap();
        let id = store.insert(Path::new()).unwrap();
        let path = TextPath::new(id).with_window(Fixed::from_int(12), Fixed::from_int(80));

        assert_eq!(path.start(), Fixed::ZERO);
        assert_eq!(path.offset(), Fixed::from_int(12));
        assert_eq!(path.end(), Some(Fixed::from_int(92)));
    }

    #[test]
    fn equal_width_offset_changes_preserve_layout() {
        let mut world = World::new();
        world.insert_resource(PathStore::new(1).unwrap());
        let widget = world.spawn_empty();
        let id = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::new())
            .unwrap();
        world.set_text_path(
            widget,
            TextPath::new(id).with_range(Fixed::ZERO..Fixed::from_int(80)),
        );
        world.remove::<Dirty>(widget);

        world.set_text_path(
            widget,
            TextPath::new(id)
                .with_range(Fixed::ZERO..Fixed::from_int(92))
                .with_offset(Fixed::from_int(12)),
        );

        assert!(world.get::<Dirty>(widget).is_none());
        assert!(world.get::<crate::ui::dirty::VisualDirty>(widget).is_some());
    }

    #[test]
    fn width_changes_still_invalidate_layout() {
        let mut world = World::new();
        world.insert_resource(PathStore::new(1).unwrap());
        let widget = world.spawn_empty();
        let id = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::new())
            .unwrap();
        world.set_text_path(
            widget,
            TextPath::new(id).with_range(Fixed::ZERO..Fixed::from_int(80)),
        );
        world.remove::<Dirty>(widget);

        world.set_text_path(
            widget,
            TextPath::new(id)
                .with_range(Fixed::ZERO..Fixed::from_int(80))
                .with_offset(Fixed::from_int(12)),
        );

        assert!(world.get::<Dirty>(widget).is_some());
        assert!(world.get::<crate::ui::dirty::VisualDirty>(widget).is_none());
    }

    #[test]
    fn fixed_range_path_edits_only_invalidate_visual_geometry() {
        let mut world = World::new();
        world.insert_resource(PathStore::new(1).unwrap());
        let widget = world.spawn_empty();
        let id = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::from_owned(alloc::vec![
                PathCmd::MoveTo(crate::types::Point::new(0, 0)),
                PathCmd::LineTo(crate::types::Point::new(100, 0)),
            ]))
            .unwrap();
        world.set_text_path(
            widget,
            TextPath::new(id).with_range(Fixed::ZERO..Fixed::from_int(80)),
        );
        world.remove::<Dirty>(widget);

        world
            .resource_mut::<PathStore>()
            .unwrap()
            .edit(id, |_| {})
            .unwrap();
        crate::core::reactive::flush_signal_dirty(&mut world);

        assert!(world.get::<Dirty>(widget).is_none());
        assert!(world.get::<crate::ui::dirty::VisualDirty>(widget).is_some());
    }

    #[test]
    fn layout_width_rejects_ranges_outside_the_path() {
        let mut world = World::new();
        world.insert_resource(PathStore::new(1).unwrap());
        world.insert_resource(crate::text::baseline::PathBaselineResource::default());
        let id = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::from_owned(alloc::vec![
                PathCmd::MoveTo(crate::types::Point::new(0, 0)),
                PathCmd::LineTo(crate::types::Point::new(10, 0)),
            ]))
            .unwrap();
        let path = TextPath::new(id).with_range(Fixed::ZERO..Fixed::from_int(11));

        assert_eq!(
            layout_width(&world, path),
            Err(crate::text::baseline::PathBaselineError::InvalidRange)
        );
    }

    #[test]
    fn layout_width_rejects_invalid_offsets() {
        let mut world = World::new();
        world.insert_resource(PathStore::new(1).unwrap());
        world.insert_resource(crate::text::baseline::PathBaselineResource::default());
        let id = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::from_owned(alloc::vec![
                PathCmd::MoveTo(crate::types::Point::new(0, 0)),
                PathCmd::LineTo(crate::types::Point::new(10, 0)),
            ]))
            .unwrap();

        for offset in [Fixed::from_int(-1), Fixed::from_int(10)] {
            assert_eq!(
                layout_width(&world, TextPath::new(id).with_offset(offset)),
                Err(crate::text::baseline::PathBaselineError::InvalidRange)
            );
        }
    }
}
