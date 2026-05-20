#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
// Lets `trace_span!` / `trace_fn` expand to `mirui::perf::enter(...)`
// uniformly — the crate has to be able to refer to itself by name
// for the absolute path to resolve in our own modules.
extern crate self as mirui;

pub mod anim;
pub mod app;
pub mod components;
pub mod draw;
pub mod ecs;
pub mod event;
pub mod input_feedback;
pub mod layout;
pub mod perf;
pub mod plugin;
pub mod plugins;
pub mod surface;
pub mod timer;
pub mod types;
pub mod widget;

pub use mirui_macros::{system, trace_fn, trace_span};
