//! Layered sprites stored in a MIRX image atlas.
//!
//! Register `view()` once and share the loaded `SpriteStackAsset` between widgets.
//! `yaw` and `pitch` are degrees. A positive pitch looks down onto the layers.
#![no_std]

extern crate alloc;

pub use mirui_sprite_stack_format as format;

#[cfg(feature = "widget")]
mod asset;
#[cfg(feature = "widget")]
mod widget;

#[cfg(feature = "widget")]
pub use asset::{SpriteStackAsset, SpriteStackError};
#[cfg(feature = "widget")]
pub use widget::props as prop;
#[cfg(feature = "widget")]
pub use widget::{LayerSpacing, SpriteStack, view};
