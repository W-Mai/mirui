//! Echo Walker is a deterministic route-recording puzzle with fixed-capacity state.

mod input;
#[cfg(feature = "persistence")]
mod persistence;
mod render;
mod shell;
mod state;
mod style;

#[cfg(test)]
mod tests;

use crate::prelude::{App, Entity, RendererFactory, Surface};

pub const VIEWPORT: (u16, u16) = (480, 320);

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    shell::setup_app(app, parent);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);
