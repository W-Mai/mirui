use alloc::vec::Vec;

use crate::prelude::{Entity, Fixed};

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
    pub velocities: Vec<(Fixed, Fixed)>,
    pub ax: Vec<Fixed>,
    pub ay: Vec<Fixed>,
}

pub struct KickPhase(pub u32);
