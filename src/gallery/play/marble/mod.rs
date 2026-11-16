mod model;
mod physics;
mod presets;
#[cfg(any(feature = "persistence", test))]
mod save;
mod transport;
mod types;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub(crate) use model::MarbleModel;
#[cfg(test)]
pub(crate) use model::MarbleModelHandle;
pub(crate) use physics::RAILS;
#[cfg(feature = "persistence")]
pub(crate) use save::{MarbleLayout, SAVE_LEN};
#[allow(unused_imports)]
pub(crate) use types::{
    Ball, MAX_BALLS, MAX_PADS, MAX_PARTICLES, MAX_RINGS, MarbleSound, PAD_PITCHES, PALETTE, Pad,
    PadTimbre, Page, Particle, Ring, THEMES, TRAIL_LEN, Theme, Vec2, WORLD_BOTTOM, WORLD_LEFT,
    WORLD_RIGHT, WORLD_TOP,
};
