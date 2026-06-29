//! Lifecycle hooks, app pause/resume, and persistence machinery.
//!
//! [`Storage`] is the byte-level KV trait every backend implements;
//! [`MemoryStorage`] is the always-available in-process implementation.
//! [`PersistencePlugin`] (gated by the `persistence` feature) layers
//! typed save/restore on top of `Storage` through serde + postcard.

pub mod storage;

#[cfg(feature = "persistence")]
pub mod persistence;

#[cfg(feature = "persistence-fs")]
pub mod file_storage;

#[cfg(all(feature = "persistence-web", target_arch = "wasm32"))]
pub mod local_storage;

pub use storage::{MemoryStorage, Storage};

#[cfg(feature = "persistence")]
pub use persistence::{PersistencePlugin, PersistenceRegistry};

#[cfg(feature = "persistence-fs")]
pub use file_storage::FileStorage;

#[cfg(all(feature = "persistence-web", target_arch = "wasm32"))]
pub use local_storage::LocalStorageStorage;
