use alloc::{
    rc::{Rc, Weak},
    vec::Vec,
};

use crate::{
    core::{
        model::{Model, ModelAccess, ModelRoot},
        persistence::{ModelCodec, PersistenceError},
        storage::{Storage, WriteOutcome},
    },
    ecs::world::WorldId,
};

use super::{Candidate, PersistenceState};

/// A weak root endpoint using one codec chosen at registration.
#[doc(hidden)]
pub struct SaveEndpoint<C: ModelCodec> {
    root: Weak<ModelRoot<C::Model>>,
    generation: Option<u64>,
    codec: Rc<C>,
}

impl<C: ModelCodec> SaveEndpoint<C>
where
    C::Model: Model<Persistence = Candidate>,
{
    pub fn new(root: &Rc<ModelRoot<C::Model>>, codec: Rc<C>) -> Self {
        Self {
            root: Rc::downgrade(root),
            generation: root.generation(),
            codec,
        }
    }

    fn validate(&self, root: &ModelRoot<C::Model>, owner: WorldId) -> Result<(), PersistenceError> {
        if self.generation.is_none() || root.generation() != self.generation {
            return Err(PersistenceError::Unregistered);
        }
        if root.metadata.owner != owner {
            return Err(PersistenceError::WrongOwner);
        }
        if root.poisoned.get() {
            return Err(PersistenceError::Poisoned);
        }
        if root.updating.get() {
            return Err(PersistenceError::ModelBusy);
        }
        Ok(())
    }

    /// Save a pending root, or encode unconditionally for an explicit save. Only a successful write confirms the captured candidate.
    pub fn save(
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

#[cfg(all(test, feature = "std"))]
mod tests;
