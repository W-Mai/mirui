//! App lifecycle hooks and persistence plugin.
//!
//! The byte-level [`Storage`][crate::core::storage::Storage] trait
//! and its backends live in `core::storage`. [`PersistencePlugin`]
//! (gated by the `persistence` feature) layers typed save/restore
//! over that trait via serde + postcard.

#[cfg(feature = "persistence")]
pub mod persistence;

#[cfg(feature = "persistence")]
pub use persistence::{PersistencePlugin, PersistenceRegistry};
