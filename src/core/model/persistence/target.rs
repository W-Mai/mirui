use alloc::rc::{Rc, Weak};
use core::any::Any;

use crate::ecs::world::WorldId;

/// Type-erased weak identity of one registered Model incarnation.
pub(crate) struct PersistenceTarget {
    reference: Weak<dyn Any>,
    generation: u64,
    pub(crate) owner: WorldId,
}

impl PersistenceTarget {
    pub(crate) fn new(reference: Rc<dyn Any>, generation: u64, owner: WorldId) -> Self {
        Self {
            reference: Rc::downgrade(&reference),
            generation,
            owner,
        }
    }

    pub(super) fn matches<M: super::Model>(&self, root: &Rc<super::super::ModelRoot<M>>) -> bool {
        use super::super::ModelAccess;
        let reference: Rc<dyn Any> = root.clone();
        root.generation() == Some(self.generation)
            && self.owner == root.metadata.owner
            && Weak::ptr_eq(&self.reference, &Rc::downgrade(&reference))
    }
}
