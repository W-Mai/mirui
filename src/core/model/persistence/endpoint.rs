use alloc::{
    rc::{Rc, Weak},
    vec::Vec,
};

use crate::{
    core::{
        model::{Model, ModelAccess, ModelChildren, ModelRoot, PersistenceTarget},
        persistence::{ModelCodec, PersistenceError},
        storage::{Storage, WriteOutcome},
    },
    ecs::world::WorldId,
};

use super::{Candidate, PersistenceState};

/// A weak root endpoint using one codec chosen at registration.
pub(crate) struct PersistenceEndpoint<C: ModelCodec> {
    root: Weak<ModelRoot<C::Model>>,
    generation: Option<u64>,
    codec: C,
}

impl<C: ModelCodec> PersistenceEndpoint<C>
where
    C::Model: Model<Persistence = Candidate>,
{
    pub(crate) fn matches(&self, reference: &PersistenceTarget) -> bool {
        let Some(root) = self.root.upgrade() else {
            return false;
        };
        self.generation.is_some()
            && self.generation == root.generation()
            && reference.matches(&root)
    }

    pub(crate) fn restore(
        &self,
        world: &crate::ecs::World,
        key: &str,
        storage: &dyn Storage,
    ) -> Result<bool, PersistenceError> {
        let root = self.root.upgrade().ok_or(PersistenceError::Unregistered)?;
        self.validate(&root, world.id())?;
        if !crate::core::reactive::model_writes_allowed()
            || !root.metadata.children.is_writable()
            || root.value.try_borrow_mut().is_err()
        {
            return Err(PersistenceError::ModelBusy);
        }
        let bytes = storage.read(key)?;
        self.validate(&root, world.id())?;
        let Some(bytes) = bytes else {
            return Ok(false);
        };
        if !root.metadata.children.is_writable() {
            return Err(PersistenceError::ModelBusy);
        }
        let mut value = root
            .value
            .try_borrow_mut()
            .map_err(|_| PersistenceError::ModelBusy)?;
        let prepared = {
            let _read_only = crate::core::reactive::ModelReadOnlyGuard::enter();
            self.codec.prepare(&value, &bytes)?
        };
        self.validate(&root, world.id())?;
        if !root.metadata.children.is_writable() {
            return Err(PersistenceError::ModelBusy);
        }
        let mut guard = root.begin_transaction(&value);
        self.codec.apply(&mut value, prepared);
        self.validate_identity(&root, world.id())?;
        root.persistence.changed();
        root.metadata
            .finish(&mut value, <C::Model as Model>::pending_watches(), true);
        root.metadata.capture_after(&value);
        drop(value);
        root.persistence.confirm(root.persistence.stamp());
        root.metadata.publish();
        guard.committed = true;
        Ok(true)
    }

    fn validate_identity(
        &self,
        root: &ModelRoot<C::Model>,
        owner: WorldId,
    ) -> Result<(), PersistenceError> {
        if self.generation.is_none() || root.generation() != self.generation {
            return Err(PersistenceError::Unregistered);
        }
        if root.metadata.owner != owner {
            return Err(PersistenceError::WrongOwner);
        }
        Ok(())
    }

    pub(crate) fn new(root: &Rc<ModelRoot<C::Model>>, codec: C) -> Self {
        Self {
            root: Rc::downgrade(root),
            generation: root.generation(),
            codec,
        }
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.root
            .upgrade()
            .is_some_and(|root| self.generation.is_some() && root.generation() == self.generation)
    }

    pub(crate) fn pending(&self, world: &crate::ecs::World) -> Result<bool, PersistenceError> {
        let root = self.root.upgrade().ok_or(PersistenceError::Unregistered)?;
        self.validate(&root, world.id())?;
        Ok(root.persistence.pending())
    }

    pub(crate) fn mark_pending(&self) {
        if let Some(root) = self.root.upgrade()
            && self.generation.is_some()
            && root.generation() == self.generation
        {
            root.persistence.changed();
        }
    }

    fn validate(&self, root: &ModelRoot<C::Model>, owner: WorldId) -> Result<(), PersistenceError> {
        self.validate_identity(root, owner)?;
        if root.poisoned.get() {
            return Err(PersistenceError::Poisoned);
        }
        if root.updating.get() {
            return Err(PersistenceError::ModelBusy);
        }
        Ok(())
    }

    /// Save a pending root, or encode unconditionally for an explicit save. Only a successful write confirms the captured candidate.
    pub(crate) fn save(
        &self,
        world: &crate::ecs::World,
        key: &str,
        storage: &mut dyn Storage,
        output: &mut Vec<u8>,
        force: bool,
    ) -> Result<Option<WriteOutcome>, PersistenceError> {
        output.clear();
        let owner = world.id();
        let root = self.root.upgrade().ok_or(PersistenceError::Unregistered)?;
        self.validate(&root, owner)?;
        if force {
            root.persistence.changed();
        } else if !root.persistence.pending() {
            return Ok(None);
        }
        let stamp = root.persistence.stamp();
        {
            let value = root
                .value
                .try_borrow()
                .map_err(|_| PersistenceError::ModelBusy)?;
            let _read_only = crate::core::reactive::ModelReadOnlyGuard::enter();
            if let Err(error) = self.codec.encode(&value, output) {
                output.clear();
                return Err(error);
            }
        }
        if let Err(error) = self.validate(&root, owner) {
            output.clear();
            return Err(error);
        }
        let outcome = storage.write(key, output)?;
        if self.validate(&root, owner).is_ok() {
            root.persistence.confirm(stamp);
        }
        Ok(Some(outcome))
    }
}

impl<M: Model<Persistence = Candidate>> crate::core::model::RegisteredModel<M> {
    pub(crate) fn save_endpoint<C: ModelCodec<Model = M>>(
        &self,
        codec: C,
        restored: bool,
    ) -> PersistenceEndpoint<C> {
        if restored {
            self.root.persistence.confirm(self.root.persistence.stamp());
        }
        PersistenceEndpoint::new(&self.root, codec)
    }

    pub(crate) fn reject_persistence(&self, error: PersistenceError) {
        self.root.persistence.reject_registration(error);
    }
}

#[cfg(all(test, feature = "std"))]
mod tests;
