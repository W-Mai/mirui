use alloc::vec::Vec;

use super::{Path, PathCmd};
use crate::core::reactive::{Signal, SignalSubscription};
use crate::ecs::Entity;

const NO_SLOT: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PathId {
    slot: u32,
    generation: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PathRevision(u32);

impl PathRevision {
    pub const fn get(self) -> u32 {
        self.0
    }

    fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathStoreError {
    Allocation,
    CapacityOverflow,
    Full { capacity: usize },
    InUse { paths: usize },
    Missing(PathId),
    Immutable(PathId),
}

enum PathSlot {
    Vacant {
        generation: u32,
        next: u32,
    },
    Static {
        generation: u32,
        path: Path,
    },
    Mutable {
        generation: u32,
        revision: PathRevision,
        changed: Option<Signal<PathRevision>>,
        path: Path,
    },
}

impl PathSlot {
    fn generation(&self) -> u32 {
        match self {
            Self::Vacant { generation, .. }
            | Self::Static { generation, .. }
            | Self::Mutable { generation, .. } => *generation,
        }
    }

    fn path(&self) -> Option<&Path> {
        match self {
            Self::Static { path, .. } | Self::Mutable { path, .. } => Some(path),
            Self::Vacant { .. } => None,
        }
    }
}

pub struct PathStore {
    slots: Vec<PathSlot>,
    free: u32,
    capacity: usize,
    len: usize,
}

pub(crate) struct PathSubscription {
    _inner: SignalSubscription<PathRevision>,
}

impl PathStore {
    pub const EMBEDDED_CAPACITY: usize = 32;
    pub const HOST_CAPACITY: usize = 4_096;

    pub fn new(capacity: usize) -> Result<Self, PathStoreError> {
        if capacity >= NO_SLOT as usize {
            return Err(PathStoreError::CapacityOverflow);
        }
        Ok(Self {
            slots: Vec::new(),
            free: NO_SLOT,
            capacity,
            len: 0,
        })
    }

    pub fn try_with_capacity(capacity: usize) -> Result<Self, PathStoreError> {
        let mut store = Self::new(capacity)?;
        store
            .slots
            .try_reserve_exact(capacity)
            .map_err(|_| PathStoreError::Allocation)?;
        Ok(store)
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn insert_static(
        &mut self,
        commands: &'static [PathCmd],
    ) -> Result<PathId, PathStoreError> {
        self.insert_slot(|generation| PathSlot::Static {
            generation,
            path: Path::from_static(commands),
        })
    }

    pub fn insert(&mut self, path: Path) -> Result<PathId, PathStoreError> {
        self.insert_slot(|generation| PathSlot::Mutable {
            generation,
            revision: PathRevision::default(),
            changed: None,
            path,
        })
    }

    pub fn get(&self, id: PathId) -> Result<&Path, PathStoreError> {
        self.resolve(id)?.path().ok_or(PathStoreError::Missing(id))
    }

    pub fn with<R>(
        &self,
        id: PathId,
        inspect: impl FnOnce(&Path) -> R,
    ) -> Result<R, PathStoreError> {
        self.get(id).map(inspect)
    }

    pub fn revision(&self, id: PathId) -> Result<Option<PathRevision>, PathStoreError> {
        match self.resolve(id)? {
            PathSlot::Static { .. } => Ok(None),
            PathSlot::Mutable { revision, .. } => Ok(Some(*revision)),
            PathSlot::Vacant { .. } => Err(PathStoreError::Missing(id)),
        }
    }

    pub fn edit<R>(
        &mut self,
        id: PathId,
        edit: impl FnOnce(&mut Path) -> R,
    ) -> Result<R, PathStoreError> {
        let (output, revision, changed) = match self.resolve_mut(id)? {
            PathSlot::Mutable {
                revision,
                changed,
                path,
                ..
            } => {
                let output = edit(path);
                revision.advance();
                (output, *revision, changed.clone())
            }
            PathSlot::Static { .. } => return Err(PathStoreError::Immutable(id)),
            PathSlot::Vacant { .. } => return Err(PathStoreError::Missing(id)),
        };
        if let Some(changed) = changed {
            changed.set(revision);
        }
        Ok(output)
    }

    pub(crate) fn subscribe(
        &mut self,
        id: PathId,
        entity: Entity,
        visual_only: bool,
    ) -> Result<Option<PathSubscription>, PathStoreError> {
        match self.resolve_mut(id)? {
            PathSlot::Static { .. } => Ok(None),
            PathSlot::Mutable {
                revision, changed, ..
            } => {
                let signal = changed.get_or_insert_with(|| Signal::new(*revision));
                Ok(Some(PathSubscription {
                    _inner: if visual_only {
                        signal.subscribe_visual_widget(entity)
                    } else {
                        signal.subscribe_widget(entity)
                    },
                }))
            }
            PathSlot::Vacant { .. } => Err(PathStoreError::Missing(id)),
        }
    }

    pub fn remove(&mut self, id: PathId) -> Result<Path, PathStoreError> {
        self.resolve(id)?;
        let generation = id.generation.wrapping_add(1);
        let vacant = PathSlot::Vacant {
            generation,
            next: self.free,
        };
        let slot = core::mem::replace(&mut self.slots[id.slot as usize], vacant);
        self.free = id.slot;
        self.len -= 1;
        match slot {
            PathSlot::Static { path, .. } => Ok(path),
            PathSlot::Mutable {
                path,
                mut revision,
                changed,
                ..
            } => {
                revision.advance();
                if let Some(changed) = changed {
                    changed.set(revision);
                }
                Ok(path)
            }
            PathSlot::Vacant { .. } => Err(PathStoreError::Missing(id)),
        }
    }

    fn insert_slot(
        &mut self,
        make: impl FnOnce(u32) -> PathSlot,
    ) -> Result<PathId, PathStoreError> {
        if self.free != NO_SLOT {
            let slot = self.free as usize;
            let (generation, next) = match self.slots[slot] {
                PathSlot::Vacant { generation, next } => (generation, next),
                _ => unreachable!("free path slot must be vacant"),
            };
            self.slots[slot] = make(generation);
            self.free = next;
            self.len += 1;
            return Ok(PathId {
                slot: slot as u32,
                generation,
            });
        }
        if self.slots.len() == self.capacity {
            return Err(PathStoreError::Full {
                capacity: self.capacity,
            });
        }
        if self.slots.len() == self.slots.capacity() {
            self.slots
                .try_reserve_exact(1)
                .map_err(|_| PathStoreError::Allocation)?;
        }
        let slot = self.slots.len() as u32;
        self.slots.push(make(0));
        self.len += 1;
        Ok(PathId {
            slot,
            generation: 0,
        })
    }

    fn resolve(&self, id: PathId) -> Result<&PathSlot, PathStoreError> {
        let slot = self
            .slots
            .get(id.slot as usize)
            .ok_or(PathStoreError::Missing(id))?;
        if slot.generation() != id.generation || matches!(slot, PathSlot::Vacant { .. }) {
            return Err(PathStoreError::Missing(id));
        }
        Ok(slot)
    }

    fn resolve_mut(&mut self, id: PathId) -> Result<&mut PathSlot, PathStoreError> {
        let slot = self
            .slots
            .get_mut(id.slot as usize)
            .ok_or(PathStoreError::Missing(id))?;
        if slot.generation() != id.generation || matches!(slot, PathSlot::Vacant { .. }) {
            return Err(PathStoreError::Missing(id));
        }
        Ok(slot)
    }
}

impl Default for PathStore {
    fn default() -> Self {
        let capacity = if cfg!(feature = "std") {
            Self::HOST_CAPACITY
        } else {
            Self::EMBEDDED_CAPACITY
        };
        Self::new(capacity).expect("default path capacity is representable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::types::Point;
    use crate::ui::dirty::Dirty;

    const STATIC_COMMANDS: &[PathCmd] = &[
        PathCmd::MoveTo(Point::ZERO),
        PathCmd::LineTo(Point {
            x: crate::types::Fixed::from_int(10),
            y: crate::types::Fixed::ZERO,
        }),
    ];

    #[test]
    fn static_paths_remain_borrowed_and_immutable() {
        let mut store = PathStore::new(1).unwrap();
        let id = store.insert_static(STATIC_COMMANDS).unwrap();

        assert!(store.get(id).unwrap().is_borrowed());
        assert_eq!(store.revision(id), Ok(None));
        assert_eq!(
            store
                .edit(id, |path| {
                    path.clear();
                })
                .unwrap_err(),
            PathStoreError::Immutable(id)
        );
    }

    #[test]
    fn edit_advances_once_and_reuses_command_capacity() {
        let mut store = PathStore::new(1).unwrap();
        let mut path = Path::try_with_capacity(4).unwrap();
        path.move_to(Point::ZERO).line_to(Point::new(10, 0));
        let id = store.insert(path).unwrap();
        let capacity = store.get(id).unwrap().command_capacity();

        store
            .edit(id, |path| {
                path.set_command(0, PathCmd::MoveTo(Point::new(1, 2)))
                    .unwrap();
                path.set_command(1, PathCmd::LineTo(Point::new(12, 2)))
                    .unwrap();
            })
            .unwrap();

        assert_eq!(store.revision(id).unwrap().unwrap().get(), 1);
        assert_eq!(store.get(id).unwrap().command_capacity(), capacity);
    }

    #[test]
    fn stale_ids_do_not_resolve_reused_slots() {
        let mut store = PathStore::new(1).unwrap();
        let old = store.insert(Path::new()).unwrap();
        store.remove(old).unwrap();
        let current = store.insert(Path::new()).unwrap();

        assert_ne!(old, current);
        assert_eq!(store.get(old).unwrap_err(), PathStoreError::Missing(old));
        assert!(store.get(current).is_ok());
    }

    #[test]
    fn capacity_is_bounded_without_growth() {
        let mut store = PathStore::new(1).unwrap();
        store.insert(Path::new()).unwrap();

        assert_eq!(
            store.insert(Path::new()),
            Err(PathStoreError::Full { capacity: 1 })
        );
    }

    #[test]
    fn mutable_path_edits_notify_only_while_subscribed() {
        let mut world = crate::ecs::World::new();
        let widget = world.spawn_empty();
        let mut store = PathStore::new(1).unwrap();
        let id = store.insert(Path::new()).unwrap();
        let subscription = store.subscribe(id, widget, false).unwrap().unwrap();

        store
            .edit(id, |path| {
                path.move_to(Point::ZERO);
            })
            .unwrap();
        flush_signal_dirty(&mut world);
        assert!(world.remove::<Dirty>(widget).is_some());

        drop(subscription);
        store
            .edit(id, |path| {
                path.line_to(Point::ZERO);
            })
            .unwrap();
        flush_signal_dirty(&mut world);
        assert!(world.get::<Dirty>(widget).is_none());
    }

    #[test]
    fn static_paths_need_no_subscription() {
        let mut world = crate::ecs::World::new();
        let widget = world.spawn_empty();
        let mut store = PathStore::new(1).unwrap();
        let id = store.insert_static(STATIC_COMMANDS).unwrap();

        assert!(store.subscribe(id, widget, false).unwrap().is_none());
    }
}
