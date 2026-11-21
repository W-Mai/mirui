use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::cell::RefCell;
use mirui::{
    __macro_support::{DeferredCommands, with_world, with_world_scope},
    core::{
        model::{BindingOwner, ModelHandle},
        reactive::{Effect, Signal},
    },
    ecs::{CommandCommitError, Component, DeferredOperation, Entity, IntoBundle, World},
};

use crate::{Geometry, Pose};

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
                revision: Signal::new(0),
            }),
        }
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

struct Members {
    entities: RefCell<Vec<Entity>>,
    capacity: usize,
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

    fn insert(&self, entity: Entity) {
        self.entities.borrow_mut().push(entity);
    }

    fn remove(&self, entity: Entity) {
        let mut entities = self.entities.borrow_mut();
        if let Some(index) = entities.iter().position(|&member| member == entity) {
            entities.remove(index);
        }
    }

    fn publish(&self) {
        self.revision
            .update(|revision| *revision = revision.wrapping_add(1));
    }
}

fn members(stage: &StackStageHandle, world: &World) -> Result<Rc<Members>, CommandCommitError> {
    let service = "stack stage";
    BindingOwner::registered(stage)
        .check_world(world)
        .map_err(|_| CommandCommitError::ForeignOwner { service })?;
    if !stage.callback_ready() {
        return Err(CommandCommitError::MissingService { service });
    }
    Ok(stage.read(|stage| stage.members.clone()))
}

struct Membership {
    members: Rc<Members>,
    entity: Entity,
    _effect: Option<Effect>,
}

impl Drop for Membership {
    fn drop(&mut self) {
        self.members.remove(self.entity);
        self.members.publish();
    }
}

/// Geometry and pose attached to a stage member.
pub struct StackVisual {
    geometry: Geometry,
    pose: Pose,
    membership: Option<Membership>,
}

impl Component for StackVisual {}

impl StackVisual {
    pub fn new(geometry: impl Into<Geometry>) -> Self {
        Self {
            geometry: geometry.into(),
            pose: Pose::default(),
            membership: None,
        }
    }

    pub fn at(mut self, pose: Pose) -> Self {
        self.pose = pose;
        self
    }

    pub const fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    pub const fn pose(&self) -> Pose {
        self.pose
    }

    /// Bind a visual description to the observed fields read from a registered model.
    pub fn from_model<H: ModelHandle + 'static>(
        model: &H,
        visual: impl Fn(&H) -> Self + 'static,
    ) -> StackVisualInput {
        let owner = BindingOwner::registered(model);
        let model = model.clone();
        StackVisualInput(VisualInput::Bound {
            owner,
            read: Box::new(move || model.callback_ready().then(|| visual(&model))),
        })
    }

    fn check(&self, world: &World) -> Result<(), CommandCommitError> {
        if self.membership.is_some() {
            return Err(CommandCommitError::OperationRejected {
                operation: "mounted stack visual",
            });
        }
        if let Geometry::LiveField(field) = &self.geometry {
            BindingOwner::registered(field)
                .check_world(world)
                .map_err(|_| CommandCommitError::ForeignOwner {
                    service: "stack visual geometry",
                })?;
        }
        Ok(())
    }
}

/// A static description or a model binding supplied to a stage command.
pub struct StackVisualInput(VisualInput);

enum VisualInput {
    Static(StackVisual),
    Bound {
        owner: BindingOwner,
        read: Box<dyn Fn() -> Option<StackVisual>>,
    },
}

impl From<StackVisual> for StackVisualInput {
    fn from(visual: StackVisual) -> Self {
        Self(VisualInput::Static(visual))
    }
}

impl StackVisualInput {
    fn check(&self, world: &World) -> Result<(), CommandCommitError> {
        match &self.0 {
            VisualInput::Static(visual) => visual.check(world),
            VisualInput::Bound { owner, .. } => {
                owner
                    .check_world(world)
                    .map_err(|_| CommandCommitError::ForeignOwner {
                        service: "stack visual model",
                    })
            }
        }
    }

    fn install(
        self,
        world: &mut World,
        entity: Entity,
    ) -> Result<Option<Effect>, CommandCommitError> {
        match self.0 {
            VisualInput::Static(visual) => {
                world.insert(entity, visual);
                Ok(None)
            }
            VisualInput::Bound { read, .. } => {
                let effect = with_world_scope(world, || {
                    Effect::new(move || {
                        let Some(next) = read() else {
                            return;
                        };
                        with_world(|world| {
                            if next.check(world).is_err() || !world.is_alive(entity) {
                                return;
                            }
                            let Some(current) = world.get::<StackVisual>(entity) else {
                                world.insert(entity, next);
                                return;
                            };
                            if current.pose == next.pose
                                && same_geometry(&current.geometry, &next.geometry)
                            {
                                return;
                            }
                            if current.membership.is_none() {
                                return;
                            }
                            let visual = world
                                .get_mut::<StackVisual>(entity)
                                .expect("mounted visual");
                            visual.pose = next.pose;
                            visual.geometry = next.geometry;
                            visual
                                .membership
                                .as_ref()
                                .expect("mounted visual")
                                .members
                                .publish();
                        });
                    })
                });
                if !world.has::<StackVisual>(entity) {
                    return Err(CommandCommitError::OperationRejected {
                        operation: "initial stack visual",
                    });
                }
                Ok(Some(effect))
            }
        }
    }
}

fn same_geometry(left: &Geometry, right: &Geometry) -> bool {
    match (left, right) {
        (Geometry::Sprite(a), Geometry::Sprite(b)) => a.ptr_eq(b),
        (Geometry::Prepared(a), Geometry::Prepared(b))
        | (Geometry::Sampled(a), Geometry::Sampled(b)) => core::ptr::eq(*a, *b),
        (Geometry::Field(a), Geometry::Field(b)) => a.ptr_eq(b),
        _ => false,
    }
}

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

fn finish_mount(world: &mut World, entity: Entity, stage: Rc<Members>, effect: Option<Effect>) {
    let visual = world
        .get_mut::<StackVisual>(entity)
        .expect("installed visual");
    visual.membership = Some(Membership {
        members: stage.clone(),
        entity,
        _effect: effect,
    });
    stage.insert(entity);
    stage.publish();
}

struct Spawn<B> {
    stage: StackStageHandle,
    bundle: B,
    visual: StackVisualInput,
}

impl<B: IntoBundle + 'static> DeferredOperation for Spawn<B> {
    fn apply(self, world: &mut World) -> Result<(), CommandCommitError> {
        let stage = members(&self.stage, world)?;
        stage.check_capacity()?;
        self.visual.check(world)?;
        let entity = world.spawn_empty();
        self.bundle.spawn_into(world, entity);
        if !world.is_alive(entity) {
            return Err(CommandCommitError::MissingTarget { entity });
        }
        // The visual argument owns the live stage attachment.
        world.remove::<StackVisual>(entity);
        let effect = match self.visual.install(world, entity) {
            Ok(effect) => effect,
            Err(error) => {
                world.despawn(entity);
                return Err(error);
            }
        };
        finish_mount(world, entity, stage, effect);
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
        let stage = members(&self.stage, world)?;
        stage.check_capacity()?;
        self.visual.check(world)?;
        let effect = self.visual.install(world, entity)?;
        finish_mount(world, entity, stage, effect);
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
        let stage = members(&self.stage, world)?;
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
        stage.insert(entity);
        previous.publish();
        stage.publish();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
