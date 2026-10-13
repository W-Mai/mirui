#![doc = include_str!("../../../docs/navigation.md")]

pub(crate) mod mount;
mod path;
mod stack;

pub use path::{NavigationError, NavigationPath};
pub use stack::{CreatePage, NavigationStack, view};
