extern crate alloc;

mod composition;
mod orbit;
mod simulation;
mod state;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use composition::setup_app;
pub use orbit::OrbitRing;
pub use simulation::{kick_system, physics_tick_system, sync_layout_system};
pub use state::{
    KickPhase, PhysicsBody, PhysicsScratch, PhysicsTime, SpringLength, Velocity, WorldBounds,
};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);
