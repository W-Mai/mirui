use super::{
    IntoDuration, SaveTarget,
    target::{SaveKind, SaveRequest, Source},
};
use super::{PersistenceError, PersistenceRegistry, PersistenceStatus};
use crate::core::reactive::Signal;
use crate::{
    core::model::{ModelHandle, PersistenceTarget, try_registered_owner},
    ecs::World,
};
use alloc::vec::Vec;
use serde::{Serialize, de::DeserializeOwned};

/// Synchronous persistence operations for one application.
pub struct Persistence<'a> {
    world: &'a mut World,
}

impl<'a> Persistence<'a> {
    pub(crate) fn new(world: &'a mut World) -> Self {
        Self { world }
    }

    /// Read registration, pending-save, and failure state without accessing storage. Disabled registrations retain their errors until IO resumes; retired handles are unregistered.
    pub fn status<H: ModelHandle>(&self, model: &H) -> PersistenceStatus {
        let Some(registry) = self.world.resource::<PersistenceRegistry>() else {
            return PersistenceStatus::Unavailable;
        };
        let Some(owner) = try_registered_owner(model) else {
            return PersistenceStatus::Unregistered;
        };
        let error = if owner != self.world.id() {
            Some(PersistenceError::WrongOwner)
        } else {
            model
                .access()
                .upgrade()
                .and_then(|access| access.persistence_registration_error())
        };
        if let Some(error) = error {
            return PersistenceStatus::Failed {
                error,
                protected: false,
            };
        }
        let Some(target) = model.access().persistence_target() else {
            return PersistenceStatus::Unregistered;
        };
        let Some(entry) = registry
            .models
            .iter()
            .find(|entry| entry.endpoint.matches(&target))
        else {
            return PersistenceStatus::Unregistered;
        };
        if !registry.enabled {
            return PersistenceStatus::Disabled;
        }
        entry.status(self.world)
    }

    /// Save a registered Model, Signal, resource, or non-Model key from its current data. A source registered under multiple keys requires an explicit key. Saving a Model clears failed-restore protection only after a successful write.
    pub fn save(&mut self, target: impl SaveTarget) -> Result<(), PersistenceError> {
        self.save_request(target.into_request())
    }

    pub(super) fn save_request(&mut self, request: SaveRequest) -> Result<(), PersistenceError> {
        if let SaveKind::Model(target) = request.0 {
            return self.with_target(target, |registry, world, index| {
                registry.models[index].save(
                    world,
                    &mut *registry.storage,
                    &mut registry.output,
                    true,
                )
            });
        }
        self.with_registry(|registry, world| {
            let source = match request.0 {
                SaveKind::Resource { owner, ty } => {
                    if owner != world.id() {
                        return Err(PersistenceError::WrongOwner);
                    }
                    Source::Resource(ty)
                }
                SaveKind::Signal(id) => Source::Signal(id),
                SaveKind::Key(key) => return registry.save(world, &key),
                SaveKind::Model(_) => unreachable!(),
            };
            if !registry.enabled {
                return Err(PersistenceError::Disabled);
            }
            let mut matches = registry
                .items
                .iter_mut()
                .filter(|item| item.source == source);
            let item = matches.next().ok_or(PersistenceError::Unregistered)?;
            if matches.next().is_some() {
                return Err(PersistenceError::AmbiguousTarget);
            }
            item.save(world, &mut *registry.storage)
        })
    }

    /// Register a resource type and immediately restore stored data, inserting the resource if absent. Missing data leaves the current resource unchanged; saving reads its current value or skips an absent resource.
    pub fn register_resource<T>(&mut self, key: &'static str) -> Result<(), PersistenceError>
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        self.with_registry(|registry, world| registry.resource::<T>(world, key))
    }

    /// Register a Signal instance and immediately restore stored data when persistence is enabled.
    pub fn register_signal<T>(
        &mut self,
        key: &'static str,
        signal: Signal<T>,
    ) -> Result<(), PersistenceError>
    where
        T: Serialize + DeserializeOwned + Clone + 'static,
    {
        self.with_registry(|registry, _| registry.signal(key, signal))
    }

    /// Register custom bytes and immediately restore existing data when enabled. A restore error prevents registration; callback mutations are not rolled back. Returning `None` from save leaves stored data unchanged.
    pub fn register_bytes(
        &mut self,
        key: &'static str,
        save: impl FnMut() -> Option<Vec<u8>> + 'static,
        restore: impl FnMut(&[u8]) -> Result<(), PersistenceError> + 'static,
    ) -> Result<(), PersistenceError> {
        self.with_registry(|registry, _| registry.bytes(key, save, restore))
    }

    /// Remove stored bytes without unregistering their source or resetting current values. Subsequent saves may recreate the record.
    pub fn remove(&mut self, key: &str) -> Result<(), PersistenceError> {
        self.with_registry(|registry, _| registry.remove(key))
    }

    /// Configure periodic saving. Integers specify milliseconds; `core::time::Duration` is also accepted. Zero disables periodic saving; invalid integers warn and leave the interval unchanged.
    pub fn autosave(&mut self, interval: impl IntoDuration) -> Result<(), PersistenceError> {
        self.with_registry(|registry, _| {
            registry.autosave(interval);
            Ok(())
        })
    }

    /// Restore a registered root and publish observations without delivering business effects. Missing data leaves the current value unchanged; invalid data is rejected before application.
    pub fn restore<H: ModelHandle>(&mut self, model: &H) -> Result<(), PersistenceError> {
        self.restore_target(model.access().persistence_target())
    }

    pub(super) fn restore_target(
        &mut self,
        target: Option<PersistenceTarget>,
    ) -> Result<(), PersistenceError> {
        self.with_target(target, |registry, world, index| {
            registry.models[index].restore(world, &*registry.storage)
        })
    }

    /// Save pending registered values without overriding failed-restore protection. All entries are attempted; the first failure is returned.
    pub fn save_all(&mut self) -> Result<(), PersistenceError> {
        self.with_registry(|registry, world| registry.save_all(world))
    }

    /// Enable or suspend persistence IO. Enabling keeps current data rather than restoring storage.
    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), PersistenceError> {
        self.with_registry(|registry, _| {
            registry.set_enabled(enabled);
            Ok(())
        })
    }

    fn with_registry(
        &mut self,
        operation: impl FnOnce(&mut PersistenceRegistry, &mut World) -> Result<(), PersistenceError>,
    ) -> Result<(), PersistenceError> {
        self.world
            .with_resource_box::<PersistenceRegistry, _>(operation)
            .ok_or(PersistenceError::Unavailable)?
    }

    fn with_target(
        &mut self,
        target: Option<PersistenceTarget>,
        operation: impl FnOnce(
            &mut PersistenceRegistry,
            &mut World,
            usize,
        ) -> Result<(), PersistenceError>,
    ) -> Result<(), PersistenceError> {
        self.with_registry(|registry, world| {
            let target = target.ok_or(PersistenceError::Unregistered)?;
            if target.owner != world.id() {
                return Err(PersistenceError::WrongOwner);
            }
            if !registry.enabled {
                return Err(PersistenceError::Disabled);
            }
            let index = registry
                .models
                .iter()
                .position(|entry| entry.endpoint.matches(&target))
                .ok_or(PersistenceError::Unregistered)?;
            operation(registry, world, index)
        })
    }
}
