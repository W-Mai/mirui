//! mirui — a `no_std`, ECS-driven UI framework for embedded, mobile,
//! desktop, Linux, and WebAssembly targets. Layout, hit testing, and draw geometry
//! use 24.8 fixed point before submission to the software framebuffer,
//! SDL GPU, WGPU, Web Canvas, Linux framebuffer/DRM, and NuttX backends.
//!
//! # Quick Start
//!
//! ```toml
//! [dependencies]
//! mirui = { version = "0.51", features = ["sdl"] }
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
//! use mirui::types::PhysicalRect;
//! use mirui::ui::widgets::Text;
//!
//! #[compose]
//! fn build_ui() {
//!     ui! {
//!         View (
//!             bg_color: ColorToken::Primary,
//!             text_color: ColorToken::OnPrimary,
//!             border_radius: 8,
//!             padding: Padding::all(10)
//!         ) {
//!             Text ("Hello mirui!")
//!         }
//!     };
//! }
//!
//! let backend = FramebufSurface::with_format(
//!     480, 320, ColorFormat::RGBA8888,
//!     |_bytes: &[u8], _area: PhysicalRect| { /* push to the display */ },
//! );
//! let mut app = App::new(backend);
//! app.with_default_widgets().with_default_systems();
//!
//! let root = app.spawn_root().id();
//! app.compose(root, build_ui());
//!
//! app.run();
//! ```
//!
//! # Other targets
//!
//! mirui also runs in browsers through Web Canvas, on Android and iOS
//! through direct WGPU or bounded software rasterization, on Linux
//! framebuffer/DRM devices, on NuttX, and on bare-metal RISC-V and ARM
//! Cortex-M MCUs through [`surface::framebuf::FramebufSurface`]. The
//! mobile, ESP32-C3, and multi-target walkthrough lives in
//! [`docs/quickstart.md`][quickstart].
//!
//! [quickstart]: https://github.com/W-Mai/mirui/blob/main/docs/quickstart.md
//!
//! # Module map
//!
//! - [`app`]: the [`App`][app::App] entry point, the [`Plugin`][app::plugin::Plugin]
//!   trait, and bundled plugins.
//! - [`core`]: cross-cutting infrastructure — cache, resource, registered
//!   [`model`][core::model] instances, the [`reactive`][core::reactive] runtime (Signal / Computed / Effect),
//!   perf tracing, timer.
//! - [`ecs`]: World, Entity, Component, Resource, Query, System, SystemScheduler.
//! - [`mod@ui`]: widget tree primitives (Style, View, Theme, Dirty),
//!   [`ui::layout`] (flexbox + absolute positioning), and [`ui::widgets`]
//!   (built-in widget instances — buttons, sliders, lazy lists, effects).
//! - [`render`]: software rasterizer, paths, textures, draw commands, font,
//!   and concrete [`render::backends`] (sw / sdl_gpu / wgpu / web_canvas).
//! - [`input`]: input dispatch, gestures, hit-testing, focus,
//!   and visual [`input::feedback`] overlays.
//! - [`surface`]: backend trait + bundled SDL2 / framebuffer / SDL_GPU /
//!   wgpu / web_canvas / Linux / NuttX implementations.
//! - [`text`]: bounded Unicode layout, shaping contracts, and font-format adapters.
//! - [`anim`]: easing, springs, motion components.
//! - `audio` (feature `audio`): fixed-capacity commands, static scores,
//!   parameterized tones, sinks, and integer PCM mixing.
//! - [`types`]: Color / Dimension / Fixed / Point / Rect / Transform / Viewport.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(test)]
mod test_support;
// Lets `trace_span!` / `trace_fn` expand to `mirui::core::perf::enter(...)`
// uniformly — the crate has to be able to refer to itself by name
// for the absolute path to resolve inside this crate.
extern crate self as mirui;

pub mod anim;
pub mod app;
#[cfg(feature = "audio")]
pub mod audio;
pub mod core;
pub mod ecs;
pub mod input;
pub mod render;
pub mod surface;
pub mod text;
pub mod types;
pub mod ui;

/// Runtime types used only by generated authoring macros.
#[cfg(feature = "persistence")]
#[doc(hidden)]
#[macro_export]
macro_rules! __mirui_persistence {
    (state;) => { type Persistence = $crate::core::model::Candidate; };
    ($($item:item)*) => { $($item)* };
}

#[cfg(not(feature = "persistence"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __mirui_persistence {
    (state;) => {
        type Persistence = ();
    };
    ($($item:item)*) => {};
}

/// Runtime types used only by generated authoring macros.
#[doc(hidden)]
pub mod __macro_support {
    pub use crate::ui::construction::{apply_component_field, component_event, component_type};
    pub fn despawn_subtree(world: &mut crate::ecs::World, entity: crate::ecs::Entity) {
        crate::ui::despawn_subtree(world, entity);
    }

    pub fn with_world<R>(f: impl FnOnce(&mut crate::ecs::World) -> R) -> Option<R> {
        crate::core::reactive::with_world(f)
    }

    pub fn with_world_scope<R>(world: &mut crate::ecs::World, f: impl FnOnce() -> R) -> R {
        crate::core::reactive::with_world_scope(world, f)
    }

    pub fn new_scope(
        world: &mut crate::ecs::World,
        parent: crate::ecs::Entity,
    ) -> crate::ui::UiScope<'_> {
        crate::ui::UiScope::new(world, parent)
    }

    pub trait ScopeWorldAccess {
        fn __mirui_world(&mut self) -> &mut crate::ecs::World;
    }

    impl ScopeWorldAccess for crate::ui::UiScope<'_> {
        fn __mirui_world(&mut self) -> &mut crate::ecs::World {
            self.world_mut()
        }
    }

    pub use crate::anim::MotionComponent;
    pub use crate::anim::generated::{MotionFrame, advance_motion};
    pub use crate::core::model::{BindingOwner, ModelOwnerError};
    pub use crate::core::reactive::widget_binding::{PreparedWidgetEffect, prepare_widget_effect};
    pub use crate::core::timer::generated::{
        TimerSchedule, bound_timer, require_timer_model, static_timer,
    };
    pub use crate::ecs::commands::{CommandCommitError, CommandError, DeferredCommands};
    pub use crate::ecs::query::system::QueryReadScope;
    pub use crate::ecs::query::system::{
        ComponentDriver, EntityItem, OptionalRead, Read, With, Without, Write,
    };
    pub use crate::ecs::system::access::{AccessError, SystemAccess};
    pub use crate::ecs::system::generated::{
        bound_deferred_system, bound_system, deferred_system, require_same_owner, static_system,
        with_metadata,
    };
    pub use crate::ecs::system::param::{
        OptionalRes, QueryParam, Res, ResMut, SystemParam, fetch_system_params,
    };
    pub use crate::input::event::binding::{
        EntityEventBindings, EntityInstallRoute, EventBinding, install_entity_event,
    };
    pub use crate::input::event::callback::{
        AppContext, CallbackContext, CallbackInvocation, EntityContext, EventCallback, NodeContext,
        PreparationError, PreparedEvent, deferred_event_callback, event_callback,
    };
    pub use crate::input::event::emission::{flush_notifications, prepare_notifications};
    pub use crate::input::event::listeners::{
        EventListeners, ListenerCapacity, ListenerDecision, ListenerError, ListenerId,
        ListenerSelector, ListenerSnapshot, ListenerTarget,
    };
    pub use crate::input::event::notification::{
        Notification, NotificationCapacity, NotificationError, NotificationFlushError,
        NotificationQueue,
    };
    pub use crate::input::event::runtime::{RuntimeInstallError, RuntimeRunError};
    pub use crate::input::event::view_attach::{AttachDefinition, PreparedAttach, prepare_attach};
    pub use crate::input::event::view_input::{PreparedInputs, prepare_inputs};
    pub use crate::ui::builder::WidgetBuilder;
    pub use crate::ui::construction::{ReservedWidget, finish_widget_static};
    pub use crate::ui::delivery::{
        dispatch_gesture as dispatch_entity_gesture, flush as flush_entity_events,
    };
    pub use crate::ui::layout_binding::LayoutBinding;
    pub use crate::ui::recipe::{PropertyBindings, WidgetRecipe};
    pub use crate::ui::view::generated::{
        ViewDescriptor, ViewObservationBindings, assemble_view, report_query_reads,
    };
    pub use ::core::cell::{Cell as __Cell, RefCell as __RefCell};
    pub use alloc::borrow::Cow as __Cow;
    pub use alloc::rc::{Rc as __Rc, Weak as __Weak};
    pub use alloc::vec::Vec as __Vec;
}

#[cfg(feature = "gallery")]
pub mod gallery;

pub use mirui_macros::{
    animate, com, component, compose, event, model, path, scene, system, timer, trace_fn,
    trace_span, ui, view,
};

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
    pub use crate::app::{App, EventCapacity, RendererFactory};
    pub use crate::core::reactive::{Computed, Effect, Signal};
    pub use crate::ecs::{CommandCapacity, Component, Entity, IntoBundle, MonoClock};
    pub use crate::input::event::contract::{Event, EventKey};
    pub use crate::input::event::focus::{FocusChanged, FocusSelectors, Focusable, PointerFocus};
    pub use crate::input::event::gesture::{
        DragCancel, DragEnd, DragMove, DragStart, GestureSelectors, LongPress, Pinch, Rotate, Tap,
    };
    pub use crate::input::event::hover::{HoverChanged, HoverSelectors};
    pub use crate::input::event::input::{
        InputSelectors, Key, KeyModifiers, PhysicalKey, Rotary, TextCommitted, Wheel,
    };
    pub use crate::input::event::layout::{LayoutSelectors, LayoutSize, SizeChanged};
    pub use crate::input::event::lifecycle::{
        HostFocusChanged, HostInteraction, HostVisibilityChanged, MemoryWarning, Quit, Resumed,
        SafeAreaChanged, Started, Suspended, ViewportChanged,
    };
    pub use crate::input::event::press::{
        PressCancelReason, PressCancelled, PressSelectors, Pressed, Released,
    };
    pub use crate::input::event::scroll::{
        ScrollChanged, ScrollEndReason, ScrollEnded, ScrollSelectors, ScrollStarted,
    };
    pub use crate::render::font::FontToken;
    pub use crate::render::path::{Path, PathId};
    pub use crate::surface::{SafeAreaInsets, Surface};
    pub use crate::text::{
        PathCaretHit, PathDirection, PathSelectionRibbon, PathTextGeometryError, TextGeometry,
        TextGeometryRead, TextPath,
    };
    pub use crate::types::{Color, Dimension, Fixed, PhysicalRect, Point, Rect};
    pub use crate::ui::layout::{
        AlignItems, FlexDirection, FlexWrap, JustifyContent, LayoutStyle, Padding, Position,
    };
    pub use crate::ui::text_input::{
        CompositionEndReason, CompositionEnded, CompositionStarted, CompositionUpdated,
        InputMethod, TextInputChanged, TextInputMode, TextInputSession, TextInputState,
    };
    pub use crate::ui::theme::{
        ColorToken, Theme, ThemeCatalog, ThemeError, ThemeId, ThemeInfo, ThemeSource, ThemedColor,
    };
    pub use crate::ui::{HitTarget, InteractionFeedback, RespectSafeArea, Style, Widget};
    pub use crate::ui::{WidgetMut, property};

    pub use mirui_macros::{
        animate, com, component, compose, event, model, path, scene, system, timer, trace_fn,
        trace_span, ui, view,
    };

    /// Surface integration — picking and wiring a backend.
    pub mod backend {
        pub use crate::app::RendererFactory;
        pub use crate::render::SwRendererFactory;
        pub use crate::render::texture::{ColorFormat, Texture, TexturePixels, TexturePixelsMut};
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
        pub use crate::render::{CompositeMode, DrawCommand, FillRule};
        pub use crate::ui::view::{PaintCtx, ViewInvalidation};
    }
}
