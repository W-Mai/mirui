//! App lifecycle state types.
//!
//! `App` owns the suspend/resume state machine and exposes
//! `App::suspend` / `App::resume` directly. Plugins driving those
//! transitions from inside an event handler write the [`SuspendRequest`] resource into `World`; `App::tick` applies it at input and frame boundaries. Typed lifecycle notifications follow the completed transition and plugin hooks.

/// Plugin → App bridge for triggering suspend / resume from inside an
/// event handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendRequest {
    Suspend,
    Resume,
}
