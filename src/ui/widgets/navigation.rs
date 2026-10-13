#![doc = include_str!("../../../docs/navigation.md")]

pub(crate) mod mount;
mod path;

pub use path::{NavigationError, NavigationPath};
