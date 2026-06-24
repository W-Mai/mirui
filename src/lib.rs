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
//! mirui = { version = "0.33", features = ["sdl"] }
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
//! use mirui::render::texture::ColorFormat;
//! use mirui::types::Rect;
//!
//! let backend = FramebufSurface::with_format(
//!     480, 320, ColorFormat::RGBA8888,
//!     |_bytes: &[u8], _area: &Rect| { /* push to your display */ },
//! );
//! let mut app = App::new(backend);
//! app.with_default_widgets().with_default_systems();
//!
//! let root = app.spawn_root().id();
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
//! - [`app`]: the [`App`][app::App] entry point, the [`Plugin`][app::plugin::Plugin]
//!   trait, and bundled plugins.
//! - [`core`]: cross-cutting infrastructure — cache, resource, the
//!   [`reactive`][core::reactive] runtime (Signal / Computed / Effect),
//!   perf tracing, timer.
//! - [`ecs`]: World, Entity, Component, Resource, Query, System, SystemScheduler.
//! - [`ui`]: widget tree primitives (Style, View, Theme, Dirty),
//!   [`ui::layout`] (flexbox + absolute positioning), and [`ui::widgets`]
//!   (built-in widget instances — buttons, sliders, lazy lists, effects).
//! - [`render`]: software rasterizer, paths, textures, draw commands, font,
//!   and concrete [`render::backends`] (sw / sdl_gpu / wgpu / web_canvas).
//! - [`input`]: input dispatch, gestures, hit-testing, focus,
//!   and visual [`input::feedback`] overlays.
//! - [`surface`]: backend trait + bundled SDL2 / framebuffer / SDL_GPU /
//!   wgpu / web_canvas / Linux / NuttX implementations.
//! - [`anim`]: easing, springs, motion components.
//! - [`types`]: Color / Dimension / Fixed / Point / Rect / Transform / Viewport.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
// Lets `trace_span!` / `trace_fn` expand to `mirui::core::perf::enter(...)`
// uniformly — the crate has to be able to refer to itself by name
// for the absolute path to resolve in our own modules.
extern crate self as mirui;

pub mod anim;
pub mod app;
pub mod core;
pub mod ecs;
pub mod input;
pub mod render;
pub mod surface;
pub mod types;
pub mod ui;

#[cfg(feature = "gallery")]
pub mod gallery;

pub use mirui_macros::{Component, path, scene, system, trace_fn, trace_span, ui};

// Re-export so `ui!`-generated code references `Rc` through `mirui`, working in
// both std and no_std user crates without an `extern crate alloc` of their own.
#[doc(hidden)]
pub use ::core::cell::Cell as __Cell;
#[doc(hidden)]
pub use ::core::cell::RefCell as __RefCell;
#[doc(hidden)]
pub use alloc::borrow::Cow as __Cow;
#[doc(hidden)]
pub use alloc::rc::Rc as __Rc;
#[doc(hidden)]
pub use alloc::vec::Vec as __Vec;

/// Glob-imports the types and macros nearly every application file
/// needs. Default prelude — `use mirui::prelude::*;`.
///
/// Sub-preludes for less common surfaces:
///
/// - [`prelude::plugin`] for writing custom [`Plugin`][app::plugin::Plugin]s
/// - [`prelude::backend`] for picking / wiring a [`Surface`][surface::Surface]
/// - [`prelude::draw`] for emitting [`DrawCommand`][render::DrawCommand]s
///   from a custom widget view
pub mod prelude {
    pub use crate::app::{App, RendererFactory};
    pub use crate::core::reactive::{Computed, Effect, Signal};
    pub use crate::ecs::{Component, Entity, IntoBundle, MonoClock, World};
    pub use crate::render::font::FontToken;
    pub use crate::surface::Surface;
    pub use crate::types::{Color, Dimension, Fixed, Point, Rect};
    pub use crate::ui::builder::WidgetBuilder;
    pub use crate::ui::layout::{
        AlignItems, FlexDirection, JustifyContent, LayoutStyle, Padding, Position,
    };
    pub use crate::ui::theme::{ColorToken, ThemedColor};
    pub use crate::ui::{Style, Widget};

    pub use mirui_macros::{animate, path, scene, system, timer, trace_fn, trace_span, ui};

    /// Surface integration — picking and wiring a backend.
    pub mod backend {
        pub use crate::app::RendererFactory;
        pub use crate::render::SwRendererFactory;
        pub use crate::render::texture::{ColorFormat, Texture};
        pub use crate::surface::{FramebufferAccess, InputEvent, Surface};
    }

    /// Plugin authorship — writing a custom `Plugin` impl.
    ///
    /// The `std`-only built-in clock plugin is reachable on its own
    /// path: `use mirui::app::plugins::StdInstantClockPlugin;`. It
    /// stays out of this prelude so the prelude itself is unconditional.
    pub mod plugin {
        pub use crate::app::plugin::Plugin;
        pub use crate::app::plugins::{FpsSummaryPlugin, InputFeedbackPlugin};
        pub use crate::core::perf;
        pub use crate::ecs::{FrameStats, FrameTimings, System, SystemScheduler, SystemSlot};
    }

    /// Custom widget / view authorship — emitting `DrawCommand`s.
    pub mod draw {
        pub use crate::render::path::Path;
        pub use crate::render::{Canvas, DrawCommand, Renderer};
        pub use crate::ui::view::{View, ViewCtx, ViewRegistry};
    }
}
