//! An endless magnetic-tether arcade game for SpriteStack.

#![no_std]

extern crate alloc;

mod app;
mod gameplay;
mod presentation;
mod world;

pub use app::{FluxRunHandle, setup_app, setup_compact_app};
pub use gameplay::{Controls, Game, Mode, Session};
pub use presentation::sound::audio_bank;
pub use world::Harbor;

pub const VIEWPORT: (u16, u16) = (800, 600);
pub const COMPACT_VIEWPORT: (u16, u16) = (128, 128);
