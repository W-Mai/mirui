//! Layered sprite images with a shared atlas and bottom-to-top slice placement.

mod asset;
pub mod grid;
pub use asset::{SpriteStackAsset, SpriteStackError};
