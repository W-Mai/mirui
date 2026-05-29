//! mirui — a `no_std`, ECS-driven UI framework for embedded, desktop,
//! and (planned) WebAssembly. Renders with 24.8 fixed-point subpixel
//! precision on a software rasterizer designed for MCUs without an FPU;
//! optionally runs on top of SDL2 (CPU or hardware-accelerated) on
//! desktop.
//!
//! # Quick Start
//!
//! ```toml
//! [dependencies]
//! mirui = { version = "0.25", features = ["sdl"] }
//! ```
//!
//! The snippet below builds against mirui's default features and is
//! verified by `cargo test --doc`. Swap `FramebufSurface` for
//! `mirui::surface::sdl::SdlSurface::new("hello", 480, 320)` (with the
//! `sdl` feature) to run on a desktop window instead of into a
//! user-supplied flush callback.
//!
//! ```no_run
//! use mirui::prelude::*;
//! use mirui::surface::framebuf::FramebufSurface;
//! use mirui::draw::texture::ColorFormat;
//! use mirui::types::Rect;
//!
//! let backend = FramebufSurface::with_format(
//!     480, 320, ColorFormat::RGBA8888,
//!     |_bytes: &[u8], _area: &Rect| { /* push to your display */ },
//! );
//! let mut app = App::new(backend);
//! app.with_default_widgets().with_default_systems();
//!
//! let root = WidgetBuilder::new(&mut app.world)
//!     .bg_color(ColorToken::Surface)
//!     .id();
//!
//! ui! {
//!     :(
//!         parent: root
//!         world: &mut app.world
//!     :)
//!
//!     header (
//!         bg_color: ColorToken::Primary,
//!         text_color: ColorToken::OnPrimary,
//!         text: "Hello mirui!",
//!         border_radius: 8
//!     ) {}
//! };
//!
//! app.set_root(root);
//! app.run();
//! ```
//!
//! # Other targets
//!
//! mirui also runs bare-metal on RISC-V and ARM Cortex-M MCUs through
//! [`surface::framebuf::FramebufSurface`], and a Cargo workspace
//! template builds the same UI code on both desktop and embedded
//! targets unchanged. The full walkthrough — including ESP32-C3
//! wiring, the workspace layout, and a recipe for adding new target
//! crates — lives in [`docs/quickstart.md`][quickstart].
//!
//! [quickstart]: https://github.com/W-Mai/mirui/blob/main/docs/quickstart.md
//!
//! # Module map
//!
//! - [`app`]: the [`App`][app::App] entry point and [`Plugin`][plugin::Plugin] trait.
//! - [`ecs`]: World, Entity, Component, Resource, Query, System, SystemScheduler.
//! - [`widget`]: widget tree primitives (Style, ComputedRect, View, Theme, Dirty).
//! - [`components`]: built-in widgets — buttons, sliders, lazy lists, effects.
//! - [`draw`]: software rasterizer, paths, textures, draw commands.
//! - [`surface`]: backend trait + bundled SDL2 / framebuffer / SDL_GPU implementations.
//! - [`event`]: input dispatch, gestures, hit-testing, focus.
//! - [`anim`]: easing, springs, motion components.
//! - [`layout`]: flexbox + absolute positioning + dimension types.
//! - [`plugins`]: bundled `App` plugins (clock, perf, FPS, input feedback).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
// Lets `trace_span!` / `trace_fn` expand to `mirui::perf::enter(...)`
// uniformly — the crate has to be able to refer to itself by name
// for the absolute path to resolve in our own modules.
extern crate self as mirui;

pub mod anim;
pub mod app;
pub mod cache;
pub mod components;
pub mod draw;
pub mod ecs;
pub mod event;
pub mod feedback;
pub mod layout;
pub mod perf;
pub mod plugin;
pub mod plugins;
pub mod surface;
pub mod timer;
pub mod types;
pub mod widget;

pub use mirui_macros::{system, trace_fn, trace_span, ui};

/// `use mirui::prelude::*;` brings in the types and macros that nearly
/// every application file needs: `App`, the layout module, `Color` /
/// `Dimension` / `Fixed`, `Entity` / `World`, the widget builder, theme
/// tokens, and the `ui!` macro. Surface backends, individual widget
/// kinds, and plugins stay on their canonical paths so the prelude
/// doesn't pin a platform choice.
pub mod prelude {
    pub use crate::app::App;
    pub use crate::ecs::{Entity, World};
    pub use crate::layout::*;
    pub use crate::types::{Color, Dimension, Fixed, Point, Rect};
    pub use crate::widget::builder::WidgetBuilder;
    pub use crate::widget::theme::{ColorToken, ThemedColor};
    pub use crate::widget::{Style, Widget};

    pub use mirui_macros::ui;
}
