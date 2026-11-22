use super::{Members, Membership, StackStageHandle, members};
use crate::{StackVisual, StackVisualInput, visual::OnModelRetirement};
use alloc::rc::Rc;
use mirui::{
    __macro_support::DeferredCommands,
    core::{model::BindingOwner, reactive::Effect},
    ecs::{CommandCommitError, DeferredOperation, Entity, IntoBundle, World},
};

/// StackStage operations on Mirui's deferred command batch.
pub trait StackStageCommands<'queue> {
    fn stage<'a>(&'a mut self, stage: &'a StackStageHandle) -> StageCommands<'a, 'queue>;
}

impl<'queue> StackStageCommands<'queue> for DeferredCommands<'queue> {
    fn stage<'a>(&'a mut self, stage: &'a StackStageHandle) -> StageCommands<'a, 'queue> {
        StageCommands {
            commands: self,
            stage,
        }
    }
}

/// Target stage for a group of ECS operations.
pub struct StageCommands<'a, 'queue> {
    commands: &'a mut DeferredCommands<'queue>,
    stage: &'a StackStageHandle,
}

impl StageCommands<'_, '_> {
    /// Create an entity and attach its visual during command commit.
    pub fn spawn<B: IntoBundle + 'static>(
        &mut self,
        bundle: B,
        visual: impl Into<StackVisualInput>,
    ) {
        self.commands.enqueue(Spawn {
            stage: self.stage.clone(),
            bundle,
            visual: visual.into(),
        });
    }

    /// Attach a visual to an existing entity from the current World.
    pub fn attach(&mut self, entity: Entity, visual: impl Into<StackVisualInput>) {
        self.commands.enqueue(Attach {
            stage: self.stage.clone(),
            entity,
            visual: visual.into(),
        });
    }

    /// Move an existing stage member and its visual to this stage.
    pub fn transfer(&mut self, entity: Entity) {
        self.commands.enqueue(Transfer {
            stage: self.stage.clone(),
            entity,
        });
    }
}

fn finish_mount(
    world: &mut World,
    entity: Entity,
    stage: Rc<Members>,
    owner: BindingOwner,
    effect: Option<Effect>,
) {
    let visual = world
        .get_mut::<StackVisual>(entity)
        .expect("installed visual");
    visual.membership = Some(Membership {
        members: stage.clone(),
        entity,
        _effect: effect,
    });
    stage.insert(entity, owner);
    stage.publish();
}

struct Spawn<B> {
    stage: StackStageHandle,
    bundle: B,
    visual: StackVisualInput,
}

impl<B: IntoBundle + 'static> DeferredOperation for Spawn<B> {
    fn apply(self, world: &mut World) -> Result<(), CommandCommitError> {
        let (stage, owner) = members(&self.stage, world)?;
        stage.check_capacity()?;
        self.visual.check(world)?;
        let entity = world.spawn_empty();
        self.bundle.spawn_into(world, entity);
        if !world.is_alive(entity) {
            return Err(CommandCommitError::MissingTarget { entity });
        }
        // The visual argument owns the live stage attachment.
        world.remove::<StackVisual>(entity);
        let effect = match self
            .visual
            .install(world, entity, OnModelRetirement::Despawn)
        {
            Ok(effect) => effect,
            Err(error) => {
                world.despawn(entity);
                return Err(error);
            }
        };
        finish_mount(world, entity, stage, owner, effect);
        Ok(())
    }
}

struct Attach {
    stage: StackStageHandle,
    entity: Entity,
    visual: StackVisualInput,
}

impl DeferredOperation for Attach {
    fn apply(self, world: &mut World) -> Result<(), CommandCommitError> {
        let entity = self.entity;
        if !world.is_alive(entity) {
            return Err(CommandCommitError::MissingTarget { entity });
        }
        if world.has::<StackVisual>(entity) {
            return Err(CommandCommitError::OperationRejected {
                operation: "occupied stack visual",
            });
        }
        let (stage, owner) = members(&self.stage, world)?;
        stage.check_capacity()?;
        self.visual.check(world)?;
        let effect = self
            .visual
            .install(world, entity, OnModelRetirement::Detach)?;
        finish_mount(world, entity, stage, owner, effect);
        Ok(())
    }
}

struct Transfer {
    stage: StackStageHandle,
    entity: Entity,
}

impl DeferredOperation for Transfer {
    fn apply(self, world: &mut World) -> Result<(), CommandCommitError> {
        let entity = self.entity;
        if !world.is_alive(entity) {
            return Err(CommandCommitError::MissingTarget { entity });
        }
        let (stage, owner) = members(&self.stage, world)?;
        let source = world
            .get::<StackVisual>(entity)
            .and_then(|visual| visual.membership.as_ref())
            .ok_or(CommandCommitError::MissingSource {
                entity,
                source: "stack stage member",
            })?;
        if Rc::ptr_eq(&stage, &source.members) {
            return Ok(());
        }
        stage.check_capacity()?;
        let source = world
            .get_mut::<StackVisual>(entity)
            .and_then(|visual| visual.membership.as_mut())
            .expect("checked stage member");
        let previous = core::mem::replace(&mut source.members, stage.clone());
        previous.remove(entity);
        stage.insert(entity, owner);
        previous.publish();
        stage.publish();
        Ok(())
    }
}
