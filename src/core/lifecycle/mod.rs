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

/// Plugin → App bridge for triggering suspend / resume from inside an
/// event handler. Plugins write this resource into the World; `App::tick`
/// drains it at the end of each frame and calls `App::suspend`/`resume`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendRequest {
    Suspend,
    Resume,
}
