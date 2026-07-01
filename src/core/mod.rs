//! Cross-cutting infrastructure shared by render, ui, app, and platform.
//!
//! `cache` is the generic LRU framework. `resource` builds on it for
//! token-keyed asset management. `reactive` is the Signal / Computed
//! / Effect runtime. `perf` records trace spans. `timer` declares
//! time-driven components.

pub mod cache;
pub mod i18n;
pub mod log;
pub mod perf;
#[cfg(feature = "persistence")]
pub mod persistence;
pub mod reactive;
pub mod resource;
pub mod storage;
pub mod time;
pub mod timer;
