use alloc::{rc::Rc, vec::Vec};
use core::cell::{Cell, RefCell};
use mirui::{
    core::{
        model::{BindingOwner, ModelHandle, ModelOwnerError},
        reactive::{Effect, Signal},
    },
    ecs::{CommandCommitError, Entity, World},
};

mod commands;
pub use commands::{StackStageCommands, StageCommands};

/// The entities and terrain displayed together by a set of stack viewports.
#[mirui::model]
pub struct StackStage {
    members: Rc<Members>,
}

impl StackStage {
    /// Reserve a fixed number of members for this stage.
    pub fn bounded(capacity: usize) -> Self {
        Self {
            members: Rc::new(Members {
                entities: RefCell::new(Vec::with_capacity(capacity)),
                capacity,
                owner: Cell::new(BindingOwner::unbound()),
                revision: Signal::new(0),
            }),
        }
    }

    pub(crate) fn with_entities<R>(&self, read: impl FnOnce(&[Entity]) -> R) -> R {
        read(&self.members.entities.borrow())
    }

    pub(crate) fn check_member_owner(&self, world: &World) -> Result<(), ModelOwnerError> {
        self.members.owner.get().check_world(world)
    }
}

#[mirui::model]
impl StackStage {
    /// Number of attached entities.
    pub fn len(&self) -> usize {
        self.members.entities.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.entities.borrow().is_empty()
    }

    /// Observe the current drawing revision.
    pub fn revision(&self) -> u64 {
        self.members.revision.get()
    }
}

pub(crate) struct Members {
    entities: RefCell<Vec<Entity>>,
    capacity: usize,
    owner: Cell<BindingOwner>,
    revision: Signal<u64>,
}

impl Members {
    fn check_capacity(&self) -> Result<(), CommandCommitError> {
        if self.entities.borrow().len() == self.capacity {
            return Err(CommandCommitError::OperationRejected {
                operation: "stack stage capacity",
            });
        }
        Ok(())
    }

    fn insert(&self, entity: Entity, owner: BindingOwner) {
        self.owner.set(owner);
        self.entities.borrow_mut().push(entity);
    }

    fn remove(&self, entity: Entity) {
        let mut entities = self.entities.borrow_mut();
        if let Some(index) = entities.iter().position(|&member| member == entity) {
            entities.remove(index);
            if entities.is_empty() {
                self.owner.set(BindingOwner::unbound());
            }
        }
    }

    pub(crate) fn publish(&self) {
        self.revision
            .update(|revision| *revision = revision.wrapping_add(1));
    }
}

fn members(
    stage: &StackStageHandle,
    world: &World,
) -> Result<(Rc<Members>, BindingOwner), CommandCommitError> {
    let service = "stack stage";
    let foreign_owner = |_| CommandCommitError::ForeignOwner { service };
    let owner = BindingOwner::registered(stage);
    owner.check_world(world).map_err(foreign_owner)?;
    if !stage.callback_ready() {
        return Err(CommandCommitError::MissingService { service });
    }
    stage.read(|stage| {
        stage.check_member_owner(world).map_err(foreign_owner)?;
        Ok((stage.members.clone(), owner))
    })
}

pub(crate) struct Membership {
    pub(crate) members: Rc<Members>,
    entity: Entity,
    _effect: Option<Effect>,
}

impl Drop for Membership {
    fn drop(&mut self) {
        self.members.remove(self.entity);
        self.members.publish();
    }
}

#[cfg(test)]
#[path = "../tests/unit/stage.rs"]
mod tests;
