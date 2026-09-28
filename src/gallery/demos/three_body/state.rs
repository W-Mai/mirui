use alloc::vec::Vec;

use crate::prelude::{Entity, Fixed, World};

#[crate::component]
pub struct Velocity {
    pub vx: Fixed,
    pub vy: Fixed,
}

#[crate::component]
pub struct PhysicsBody {
    pub x: Fixed,
    pub y: Fixed,
}

#[crate::component]
#[derive(Clone, Copy)]
pub(super) struct LayoutOrigin {
    pub(super) x: Fixed,
    pub(super) y: Fixed,
}

pub struct PhysicsTime {
    pub last_tick_ms: u32,
    pub accumulator_ms: u32,
}

pub struct WorldBounds {
    pub w: i32,
    pub h: i32,
}

pub struct SpringLength(pub Fixed);

#[derive(Default)]
pub struct PhysicsScratch {
    pub entities: Vec<Entity>,
    pub positions: Vec<(Fixed, Fixed)>,
    pub ax: Vec<Fixed>,
    pub ay: Vec<Fixed>,
}

impl PhysicsScratch {
    pub(super) fn with_entities(world: &mut World, f: impl FnOnce(&mut World, &[Entity])) {
        let Some(scratch) = world.resource_mut::<Self>() else {
            return;
        };
        let scratch = core::mem::take(scratch);
        f(world, &scratch.entities);
        if let Some(slot) = world.resource_mut::<Self>() {
            *slot = scratch;
        }
    }
}

pub struct KickPhase(pub u32);
