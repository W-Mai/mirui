use super::{PersistenceError, PersistenceRegistry, PersistenceStatus};
use crate::{
    core::model::{ModelHandle, PersistenceTarget, try_registered_owner},
    ecs::World,
};

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

    /// Save a registered root from its current data. A successful write clears failed-restore protection.
    pub fn save<H: ModelHandle>(&mut self, model: &H) -> Result<(), PersistenceError> {
        self.save_target(model.access().persistence_target())
    }

    pub(super) fn save_target(
        &mut self,
        target: Option<PersistenceTarget>,
    ) -> Result<(), PersistenceError> {
        self.with_target(target, |registry, world, index| {
            registry.models[index].save(world, &mut *registry.storage, &mut registry.output, true)
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
