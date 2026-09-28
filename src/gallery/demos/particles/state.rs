use crate::prelude::Fixed;

#[crate::component]
pub struct Particle {
    pub x: Fixed,
    pub y: Fixed,
    pub vx: Fixed,
    pub vy: Fixed,
    pub phase: Fixed,
}

#[crate::component]
pub struct PulseRing {
    pub radius: Fixed,
    pub grow_speed: Fixed,
    pub max_radius: Fixed,
}

#[crate::component]
pub struct BouncingBar {
    pub pos: Fixed,
    pub speed: Fixed,
    pub vertical: bool,
}

pub struct ParticleBounds {
    pub w: i32,
    pub h: i32,
}

#[crate::component]
pub struct ParticleArena;
