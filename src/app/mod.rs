pub mod lifecycle;
pub mod plugin;
pub mod plugins;

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::app::plugin::Plugin;
use crate::ecs::{Entity, System, SystemScheduler, World};
use crate::input::event::PointerCursor;
use crate::input::event::focus::FocusState;
use crate::input::event::gesture::GestureSystem;
use crate::input::event::multi_tap::MultiTapTracker;
use crate::input::event::scroll::{ScrollDragState, ScrollMotion};
use crate::render::canvas::Canvas;
use crate::render::renderer::Renderer;
use crate::surface::{FramebufferAccess, InputEvent, Surface};
use crate::text::TextGeometry;
use crate::types::{PhysicalRect, Rect};
use crate::ui::dirty::DirtyRegions;
use crate::ui::offscreen::WidgetTexture;
use crate::ui::outputs::Outputs;
use crate::ui::render_system;
use crate::ui::view::{View, ViewRegistry};
use crate::ui::{Theme, ThemeCatalog, ThemeError, ThemeSource};

pub use crate::input::event::EventCapacity;
pub use crate::render::factory::{RendererFactory, SwRendererFactory};

/// Viewport used by the frame currently entering layout and rendering.
///
/// Plugins can read this resource from `pre_render` without retaining a
/// surface reference or duplicating platform resize state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderViewport(pub crate::types::Viewport);

/// The first bounded text error attached to the current root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextContentFailure {
    pub entity: Entity,
    pub error: crate::ui::widgets::text::TextContentError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextLayoutReservation {
    Unspecified,
    Pending(crate::text::TextLayoutCapacity),
    Prepared,
    Absolute,
}

fn text_belongs_to_root(world: &World, root: Entity, mut entity: Entity) -> bool {
    let max_depth = world.query::<crate::ui::Parent>().iter().count();
    for _ in 0..=max_depth {
        if entity == root {
            return true;
        }
        let Some(parent) = world.get::<crate::ui::Parent>(entity) else {
            return false;
        };
        entity = parent.0;
    }
    false
}

fn clear_hidden_root(
    world: &World,
    transform: &crate::types::Viewport,
    renderer: &mut impl Canvas,
) {
    let (lw, lh) = transform.logical_size();
    let surface = world
        .resource::<Theme>()
        .expect("App always owns an active Theme")
        .resolve(crate::ui::theme::ColorToken::Surface);
    renderer.clear(&Rect::new(0, 0, lw, lh), &surface);
}

/// Main application entry point — ties World + Surface + Renderer factory together
pub struct App<B: Surface, F: RendererFactory<B> = SwRendererFactory> {
    pub world: World,
    pub backend: B,
    pub factory: F,
    pub root: Option<Entity>,
    pub systems: SystemScheduler,
    plugins: Vec<Box<dyn Plugin<B, F>>>,
    #[cfg(feature = "perf")]
    pub perf: Option<crate::render::PerfCtx>,
    last_layout_ns: u64,
    last_render_ns: u64,
    last_flush_ns: u64,
    last_seed_prev_ns: u64,
    dirty_plan: DirtyRegions,
    pending_frame: Option<PendingFrame>,
    needs_full_first_frame: bool,
    suspended: bool,
    started: bool,
    quitting: bool,
    text_layout_started: bool,
    dirty_reserved_slots: u32,
    dirty_reserved_live_entities: usize,
    text_layout_reservation: TextLayoutReservation,
    text_layout_preparation_error: Option<crate::text::TextLayoutError>,
    text_raster_scratch_extent: Option<(u16, u16)>,
    text_raster_scratch_preparation_error: Option<crate::render::RenderError>,
}

struct PendingFrame {
    frame_start: u64,
    input_end: u64,
    systems_end: u64,
}

impl<B: FramebufferAccess> App<B, SwRendererFactory> {
    pub fn new(backend: B) -> Self {
        Self::with_factory(backend, SwRendererFactory::new())
    }
}

type HeadlessFlush = fn(&[u8], PhysicalRect);

impl App<crate::surface::framebuf::FramebufSurface<HeadlessFlush>, SwRendererFactory> {
    /// In-memory `App` for tests and snapshots. No-op flush, no
    /// event source.
    pub fn headless(width: u16, height: u16) -> Self {
        let cb: HeadlessFlush = |_buf, _area| {};
        Self::new(crate::surface::framebuf::FramebufSurface::new(
            width, height, cb,
        ))
    }
}

/// Default-sink lookup table. First matching row wins; the three
/// `not(...)` guards below make the rows mutually exclusive at
/// compile time. `None` means the user disabled every default-sink
/// feature and is expected to call [`App::with_log`] themselves.
///
///   wasm  →  `WebConsoleSink`
///   nuttx →  `NuttxSyslogSink`  (stderr would block on /dev/console)
///   std   →  `StderrSink`
#[cfg(feature = "std")]
fn default_log_sink() -> Option<alloc::boxed::Box<dyn crate::core::log::Sink>> {
    #[cfg(all(feature = "log-web-console", target_arch = "wasm32"))]
    {
        return Some(alloc::boxed::Box::new(
            crate::core::log::sinks::WebConsoleSink::new(),
        ));
    }
    #[cfg(all(
        feature = "log-nuttx-syslog",
        target_os = "nuttx",
        not(target_arch = "wasm32"),
    ))]
    {
        return Some(alloc::boxed::Box::new(
            crate::core::log::sinks::NuttxSyslogSink::info(),
        ));
    }
    #[cfg(all(
        feature = "log-stderr",
        not(target_os = "nuttx"),
        not(target_arch = "wasm32"),
    ))]
    {
        return Some(alloc::boxed::Box::new(
            crate::core::log::sinks::StderrSink::info(),
        ));
    }
    #[allow(unreachable_code)]
    None
}

#[cfg(feature = "std")]
fn install_default_log_sink_once() {
    use std::sync::OnceLock;
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        if let Some(sink) = default_log_sink() {
            crate::core::log::install_sink(sink);
        }
    });
}

impl<B: Surface, F: RendererFactory<B>> App<B, F> {
    /// Install a resource immediately in this application's World.
    ///
    /// Replaces the previous value of the same type. Callbacks borrow resources through `res!`; structural resource changes inside callbacks use deferred `commands!`.
    pub fn add_resource<T: 'static>(&mut self, value: T) -> &mut Self {
        self.world.insert_resource(value);
        self
    }

    /// Register an independent model instance and return its shared handle.
    pub fn add_model<M: crate::core::model::Model>(&mut self, value: M) -> M::Handle {
        let (registration, handle) = crate::core::model::register(&mut self.world, value);
        let entity = self.world.spawn_empty();
        self.world.insert(entity, registration);
        handle
    }

    /// Register the consumer for one effect type produced by a model instance.
    pub fn on_effect<H, E>(
        &mut self,
        model: &H,
        handler: impl Fn(E) + 'static,
    ) -> Result<(), crate::core::model::EffectRegistrationError>
    where
        H: crate::core::model::ModelHandle,
        H::Data: crate::core::model::Produces<E>,
    {
        crate::core::model::register_effect(self.world.id(), model, handler)
    }

    /// Override the bounded UI event preset before installing callbacks. Invalid changes log a warning and retain the installed configuration.
    ///
    /// Ordinary callbacks use [`EventCapacity::default`] automatically. Supply an explicit declaration for larger payloads, operation batches, or smaller limits; dispatch never expands these capacities.
    pub fn with_event_capacity(&mut self, capacity: EventCapacity) -> &mut Self {
        if let Err(error) = crate::input::event::runtime::configure(&mut self.world, capacity) {
            crate::warn!(target: "mirui::event::install", "event capacity ignored: {:?}", error);
        }
        self
    }

    /// Install an App lifecycle or input-fallback callback in declaration order. Rejected bindings log a warning without failing application construction.
    pub fn on_event<E>(
        &mut self,
        key: crate::input::event::contract::EventKey<E>,
        callback: crate::input::event::callback::EventCallback<
            E,
            crate::input::event::callback::AppContext,
        >,
    ) -> &mut Self
    where
        E: crate::input::event::contract::Event<Options = ()>,
        E::Route: crate::input::event::contract::AppRoute,
        E::Source: crate::input::event::contract::AppEventSource,
        <E::Route as crate::input::event::contract::EventRoute>::Decision:
            crate::input::event::listeners::ListenerDecision,
    {
        if let Err(error) =
            crate::input::event::runtime::install_app(&mut self.world, key, callback)
        {
            crate::warn!(target: "mirui::event::install", "App event binding skipped: {:?}", error);
        }
        self
    }

    pub fn with_factory(backend: B, factory: F) -> Self {
        #[cfg(feature = "std")]
        install_default_log_sink_once();
        let mut world = World::new();
        world.insert_resource(ScrollDragState::default());
        world.insert_resource(ScrollMotion::default());
        world.insert_resource(PointerCursor::default());
        world.insert_resource(GestureSystem::default());
        world.insert_resource(MultiTapTracker::new());
        world.insert_resource(FocusState::default());
        let info = backend.display_info();
        world.insert_resource(info);
        world.insert_resource(backend.safe_area_insets());
        world.insert_resource(ViewRegistry::default());
        world.insert_resource(ThemeCatalog::default());
        world.insert_resource(Theme::default());
        world.insert_resource(crate::render::font::default_font_manager());
        world.insert_resource(Outputs::<TextGeometry>::new_text(
            crate::text::TextLayoutLimits::default(),
        ));
        world.insert_resource(crate::render::path::PathStore::default());
        world.insert_resource(crate::core::i18n::I18n::default());
        world.insert_resource(Outputs::<WidgetTexture>::with_budget(0));
        world.insert_resource(crate::ui::IdMap::new());
        world.insert_resource(crate::ui::dirty::ExactDirtyRegions::default());
        world.insert_resource(RenderViewport(backend.viewport()));
        world.insert_resource(crate::ui::render_system::LastDirtyRegions(
            DirtyRegions::with_rect_capacity(render_system::DIRTY_PLAN_RECT_CAPACITY),
        ));
        crate::input::event::runtime::initialize(&mut world);
        Self {
            world,
            backend,
            factory,
            root: None,
            systems: SystemScheduler::new(),
            plugins: Vec::new(),
            #[cfg(feature = "perf")]
            perf: None,
            last_layout_ns: 0,
            last_render_ns: 0,
            last_flush_ns: 0,
            last_seed_prev_ns: 0,
            dirty_plan: DirtyRegions::with_rect_capacity(render_system::DIRTY_PLAN_RECT_CAPACITY),
            pending_frame: None,
            needs_full_first_frame: true,
            suspended: false,
            started: false,
            quitting: false,
            text_layout_started: false,
            dirty_reserved_slots: 0,
            dirty_reserved_live_entities: 0,
            text_layout_reservation: TextLayoutReservation::Unspecified,
            text_layout_preparation_error: None,
            text_raster_scratch_extent: None,
            text_raster_scratch_preparation_error: None,
        }
    }

    /// Replace the active [`Theme`]. Defaults to [`Theme::dark`].
    pub fn with_theme(&mut self, theme: Theme) -> &mut Self {
        crate::ui::theme::set_theme(&mut self.world, theme)
            .expect("an owned theme is always accepted");
        self
    }

    /// The default manager falls back to the bundled 8x8 bitmap for any
    /// unregistered token; passing a custom manager replaces it.
    ///
    /// Replacement invalidates Text and TextInput layouts. Editing an installed manager directly does not automatically schedule layout or drawing.
    pub fn with_fonts(&mut self, fonts: crate::render::font::FontManager) -> &mut Self {
        self.world.insert_resource(fonts);
        self.world
            .invalidate_query_dependency::<crate::text::TextLayoutHandle>();
        self.world
            .invalidate_query_dependency::<crate::ui::widgets::text_input::TextInputLayoutHandle>();
        self
    }

    /// Configure text limits while preserving any bounded text reservation.
    ///
    /// Panics if the new limits cannot hold a bounded reservation or if that
    /// cache has already been used. Use [`Self::try_with_text_layout_limits`]
    /// when either case should be handled as an error.
    pub fn with_text_layout_limits(&mut self, limits: crate::text::TextLayoutLimits) -> &mut Self {
        self.try_with_text_layout_limits(limits)
            .expect("text layout limits cannot replace the bounded cache")
    }

    /// Configure text limits without discarding an existing bounded reservation.
    /// A failed replacement leaves the active cache unchanged.
    pub fn try_with_text_layout_limits(
        &mut self,
        limits: crate::text::TextLayoutLimits,
    ) -> Result<&mut Self, crate::text::TextLayoutError> {
        let bounded = {
            let resource = self
                .world
                .resource::<Outputs<TextGeometry>>()
                .expect("the app owns a text-layout cache");
            let storage = resource.storage();
            let cache = &storage.layouts;
            if cache.is_in_use()
                && (cache.bounded_capacity().is_some()
                    || matches!(
                        self.text_layout_reservation,
                        TextLayoutReservation::Pending(_)
                    ))
            {
                return Err(crate::text::TextLayoutError::InUse);
            }
            cache.bounded_capacity()
        };
        let cache = if let Some(capacity) = bounded {
            crate::text::layout::TextLayoutCache::try_new_bounded(limits, capacity)?
        } else {
            crate::text::layout::TextLayoutCache::new(limits)
        };
        self.world
            .resource::<Outputs<TextGeometry>>()
            .expect("the app owns a text-layout cache")
            .replace_text_layouts(cache);
        self.world
            .invalidate_query_dependency::<crate::text::TextLayoutHandle>();
        self.world
            .invalidate_query_dependency::<crate::ui::widgets::text_input::TextInputLayoutHandle>();
        Ok(self)
    }

    /// Reserve bounded text-layout storage before the first layout pass.
    /// Existing text layouts are left intact if reservation fails.
    pub fn with_text_layout_capacity(
        &mut self,
        capacity: crate::text::TextLayoutCapacity,
    ) -> Result<&mut Self, crate::text::TextLayoutError> {
        if self.text_layout_started {
            return Err(crate::text::TextLayoutError::InUse);
        }
        let limits = {
            let resource = self
                .world
                .resource::<Outputs<TextGeometry>>()
                .expect("the app owns a text-layout cache");
            let storage = resource.storage();
            let cache = &storage.layouts;
            if cache.is_in_use() {
                return Err(crate::text::TextLayoutError::InUse);
            }
            if matches!(
                self.text_layout_reservation,
                TextLayoutReservation::Pending(_) | TextLayoutReservation::Prepared
            ) {
                return Err(crate::text::TextLayoutError::ConfigurationConflict);
            }
            cache.limits()
        };
        let cache = crate::text::layout::TextLayoutCache::try_new_bounded(limits, capacity)?;
        self.world
            .resource::<Outputs<TextGeometry>>()
            .expect("the app owns a text-layout cache")
            .replace_text_layouts(cache);
        self.text_layout_reservation = TextLayoutReservation::Absolute;
        Ok(self)
    }

    /// Add one tree's text storage requirement before the first layout pass.
    /// The cache is reserved once by [`Self::prepare_text_layout`].
    pub fn require_text_layout_capacity(
        &mut self,
        capacity: crate::text::TextLayoutCapacity,
    ) -> Result<&mut Self, crate::text::TextLayoutError> {
        if self.text_layout_started {
            return Err(crate::text::TextLayoutError::InUse);
        }
        let resource = self
            .world
            .resource::<Outputs<TextGeometry>>()
            .expect("the app owns a text-layout cache");
        if resource.storage().layouts.is_in_use() {
            return Err(crate::text::TextLayoutError::InUse);
        }
        let combined = match self.text_layout_reservation {
            TextLayoutReservation::Unspecified => capacity,
            TextLayoutReservation::Pending(current) => current
                .checked_add(capacity)
                .ok_or(crate::text::TextLayoutError::DimensionOverflow)?,
            TextLayoutReservation::Prepared => return Err(crate::text::TextLayoutError::InUse),
            TextLayoutReservation::Absolute => {
                return Err(crate::text::TextLayoutError::ConfigurationConflict);
            }
        };
        self.text_layout_reservation = TextLayoutReservation::Pending(combined);
        Ok(self)
    }

    /// Request reusable raster storage for bounded text runs.
    /// Multiple requests retain the largest width and height independently.
    /// This is a performance hint; runs that exceed it use the normal cache.
    pub fn prefer_text_raster_scratch(
        &mut self,
        max_logical_width: u16,
        max_logical_height: u16,
    ) -> Result<&mut Self, crate::render::RenderError> {
        if max_logical_width == 0 || max_logical_height == 0 {
            return Err(crate::render::RenderError::InvalidGeometry);
        }
        let (width, height) = self.text_raster_scratch_extent.unwrap_or((0, 0));
        self.text_raster_scratch_extent =
            Some((width.max(max_logical_width), height.max(max_logical_height)));
        Ok(self)
    }

    fn prepare_text_raster_scratch(&mut self, viewport: &crate::types::Viewport) {
        let Some((width, height)) = self.text_raster_scratch_extent else {
            return;
        };
        let retained_runs = self
            .world
            .query::<crate::ui::widgets::Text>()
            .iter()
            .filter(|(_, text)| text.text_capacity().is_some())
            .count()
            .max(1);
        self.text_raster_scratch_preparation_error = self
            .factory
            .prepare_text_raster_scratch(viewport, width, height, retained_runs)
            .err();
    }

    /// Most recent optional raster-scratch preparation failure.
    /// Rendering continues through the existing glyph cache when preparation fails.
    pub const fn last_text_raster_scratch_preparation_error(
        &self,
    ) -> Option<crate::render::RenderError> {
        self.text_raster_scratch_preparation_error
    }

    /// Reserve all declared text storage atomically before entering layout.
    /// Failed reservations leave both the current cache and declarations intact.
    pub fn prepare_text_layout(&mut self) -> Result<&mut Self, crate::text::TextLayoutError> {
        let TextLayoutReservation::Pending(capacity) = self.text_layout_reservation else {
            return Ok(self);
        };
        let limits = {
            let resource = self
                .world
                .resource::<Outputs<TextGeometry>>()
                .expect("the app owns a text-layout cache");
            let storage = resource.storage();
            let cache = &storage.layouts;
            if cache.is_in_use() {
                let error = crate::text::TextLayoutError::InUse;
                self.text_layout_preparation_error = Some(error);
                return Err(error);
            }
            cache.limits()
        };
        let cache = match crate::text::layout::TextLayoutCache::try_new_bounded(limits, capacity) {
            Ok(cache) => cache,
            Err(error) => {
                self.text_layout_preparation_error = Some(error);
                return Err(error);
            }
        };
        self.world
            .resource::<Outputs<TextGeometry>>()
            .expect("the app owns a text-layout cache")
            .replace_text_layouts(cache);
        self.text_layout_reservation = TextLayoutReservation::Prepared;
        self.text_layout_preparation_error = None;
        Ok(self)
    }

    /// Most recent App-wide text preparation failure, independent of text entities.
    pub const fn last_text_layout_preparation_error(&self) -> Option<crate::text::TextLayoutError> {
        self.text_layout_preparation_error
    }

    /// Last text preparation failure, if a layout pass has not since succeeded.
    pub fn last_text_layout_failure(&self) -> Option<crate::ui::render_system::TextLayoutFailure> {
        self.world
            .resource::<crate::ui::render_system::TextLayoutFailure>()
            .copied()
    }

    fn text_content_failure(&self, pending_initial_only: bool) -> Option<TextContentFailure> {
        let root = self.root?;
        self.world
            .query::<crate::ui::widgets::Text>()
            .iter()
            .find_map(|(entity, text)| {
                if pending_initial_only && text.has_valid_content() {
                    return None;
                }
                let error = text.last_content_error()?;
                text_belongs_to_root(&self.world, root, entity)
                    .then_some(TextContentFailure { entity, error })
            })
    }

    /// Inspect bounded text errors in the current UI tree without allocating.
    pub fn last_text_content_failure(&self) -> Option<TextContentFailure> {
        self.text_content_failure(true)
            .or_else(|| self.text_content_failure(false))
    }

    /// Set the maximum number of live paths while the registry is empty. Retired slot generations remain allocated so removed IDs cannot resolve new paths.
    pub fn with_path_capacity(
        &mut self,
        capacity: usize,
    ) -> Result<&mut Self, crate::render::path::PathStoreError> {
        if let Some(store) = self.world.resource_mut::<crate::render::path::PathStore>() {
            store.set_capacity_if_empty(capacity)?;
        } else {
            self.world
                .insert_resource(crate::render::path::PathStore::new(capacity)?);
        }
        Ok(self)
    }

    pub fn paths(&mut self) -> crate::text::PathAccess<'_> {
        self.world.paths()
    }

    pub fn viewport(&self) -> crate::types::Viewport {
        self.backend.viewport()
    }

    pub fn with_i18n(&mut self, i18n: crate::core::i18n::I18n) -> &mut Self {
        self.world.insert_resource(i18n);
        self
    }

    /// Set the widget output storage's byte budget. The budget must cover concurrently needed current outputs, retained history, writable slots, and the shared Mirror/Shadow/Glow/TemporalMix workspace. A 128×64 RGBA texture occupies 32 KiB. Pass `0` to disable output storage; explicit `OffscreenRender` entities then fall through to inline rendering.
    ///
    /// Replaces the existing output storage, so configure it before constructing widgets that request retained history or rendering an [`crate::ui::OffscreenRender`] entity.
    pub fn with_offscreen_pool_budget(&mut self, budget_bytes: usize) -> &mut Self {
        self.world
            .insert_resource(Outputs::<WidgetTexture>::with_budget(budget_bytes));
        self
    }

    /// Appends a sink to the global log registry — multiple calls
    /// fan out, never replace.
    pub fn with_log(&mut self, sink: impl crate::core::log::Sink + 'static) -> &mut Self {
        crate::core::log::install_sink(alloc::boxed::Box::new(sink));
        self
    }

    /// Runtime counterpart to `with_theme`: also forces a full-tree repaint.
    pub fn set_theme(&mut self, source: impl Into<ThemeSource>) -> Result<(), ThemeError> {
        crate::ui::theme::set_theme(&mut self.world, source)
    }

    pub fn register_theme(&mut self, theme: Theme) -> &mut Self {
        crate::ui::theme::register(&mut self.world, theme);
        self
    }

    pub fn themes(&self) -> &ThemeCatalog {
        self.world
            .resource::<ThemeCatalog>()
            .expect("App always owns a ThemeCatalog")
    }

    pub fn theme(&self) -> &Theme {
        self.world
            .resource::<Theme>()
            .expect("App always owns an active Theme")
    }

    pub fn edit_theme(&mut self, update: impl FnOnce(&mut Theme)) {
        let edited = crate::ui::theme::edit(&mut self.world, update);
        debug_assert!(edited, "App always owns an active Theme");
    }

    pub fn edit_theme_id(
        &mut self,
        id: impl Into<crate::ui::theme::ThemeId>,
        update: impl FnOnce(&mut Theme),
    ) -> Result<(), ThemeError> {
        crate::ui::theme::edit_registered(&mut self.world, id, update)
    }

    /// Copy the entity's completed output into an owned texture. Triggers a full-tree render if no completed output is available. Shared reads use [`Outputs<WidgetTexture>`].
    ///
    /// Returns `None` when the renderer cannot produce an offscreen texture or the output storage cannot fit it. Render failures are returned without exposing a partial snapshot.
    pub fn snapshot_widget(
        &mut self,
        entity: crate::ecs::Entity,
    ) -> Result<Option<crate::render::texture::Texture<'static>>, crate::render::RenderError> {
        use crate::ui::OffscreenRender;

        if let Some(output) = self
            .world
            .resource::<Outputs<WidgetTexture>>()
            .and_then(|outputs| outputs.get(entity))
        {
            return Ok(Some(clone_texture_owned(output.texture())));
        }

        let already_requested = crate::ui::offscreen::config(&self.world, entity).is_some();
        if !already_requested {
            self.world.insert(entity, OffscreenRender::default());
        }

        let render_result = self.render();

        let result = self
            .world
            .resource::<Outputs<WidgetTexture>>()
            .and_then(|outputs| outputs.get(entity))
            .map(|output| clone_texture_owned(output.texture()));

        if !already_requested {
            self.world.remove::<OffscreenRender>(entity);
            crate::ui::offscreen::reconcile_texture_outputs(&self.world);
        }

        render_result.map(|_| result)
    }

    /// Register one widget kind (built-in or user-defined).
    pub fn with_widget(&mut self, view: View) -> &mut Self {
        let systems = &mut self.systems;
        view.install(&mut self.world, |s| systems.add(s));
        if let Some(reg) = self.world.resource_mut::<ViewRegistry>() {
            reg.insert(view);
        }
        self
    }

    /// Batch counterpart of `with_widget`; also accepts a `ViewRegistry`.
    pub fn with_widgets(&mut self, views: impl IntoIterator<Item = View>) -> &mut Self {
        for view in views {
            self.with_widget(view);
        }
        self
    }

    /// Register all built-in mirui widgets in render-priority order.
    pub fn with_default_widgets(&mut self) -> &mut Self {
        self.with_widgets(ViewRegistry::with_builtins())
    }

    pub fn with_default_systems(&mut self) -> &mut Self {
        self.add_system(crate::anim::sync_delta_time_ms::system());
        self.add_system(crate::core::timer::timer_system::system());
        self.add_system(crate::core::i18n::i18n_dirty_system::system());
        self.add_system(crate::input::event::scroll::system::scroll_inertia_system::system());
        self.add_system(crate::ui::state::hover_system::system());
        self.add_system(crate::ui::state::press_system::system());
        self
    }

    pub fn add_system(&mut self, system: System) -> &mut Self {
        system.assert_owner(&self.world);
        self.systems.add(system);
        self
    }

    /// Install a generated timer and return its callback entity.
    pub fn add_timer(&mut self, timer: crate::core::timer::Timer) -> Entity {
        crate::core::timer::install(&mut self.world, timer)
    }

    /// Register a plugin. Runs `plugin.build(self)` immediately, then stores
    /// the instance so later lifecycle hooks (pre/post_render, on_event,
    /// on_quit) can be dispatched to it.
    pub fn add_plugin<P: Plugin<B, F> + 'static>(&mut self, mut plugin: P) -> &mut Self {
        plugin.build(self);
        self.plugins.push(Box::new(plugin));
        self
    }

    /// Return the shared audio controls when an audio output plugin is installed.
    #[cfg(feature = "audio")]
    pub fn audio(&self) -> Option<crate::audio::AudioHandle> {
        self.world.resource::<crate::audio::AudioHandle>().cloned()
    }

    /// Notify plugins that a platform host control received an explicit user
    /// interaction. This is separate from widget input dispatch so host UI
    /// controls can unlock platform services without triggering a demo.
    pub fn notify_host_interaction(&mut self) {
        for plugin in &mut self.plugins {
            plugin.on_host_interaction(&mut self.world);
        }
        self.notify_app(crate::input::event::lifecycle::HostInteraction);
    }

    pub fn suspend(&mut self) {
        if !self.suspended {
            self.suspended = true;
            let now_ms = (self.clock_ns() / 1_000_000) as u32;
            crate::input::event::cancel_active_input(&mut self.world, now_ms);
            self.dispatch_gestures();
            for p in &mut self.plugins {
                p.on_suspend(&mut self.world);
            }
            self.notify_app(crate::input::event::lifecycle::Suspended);
        }
    }

    pub fn resume(&mut self) {
        if self.suspended {
            self.suspended = false;
            for p in &mut self.plugins {
                p.on_resume(&mut self.world);
            }
            self.notify_app(crate::input::event::lifecycle::Resumed);
        }
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// Releases reconstructible caches while preserving application state.
    pub fn handle_memory_warning(&mut self) {
        if let Some(outputs) = self.world.resource::<Outputs<WidgetTexture>>() {
            outputs.trim_memory();
        }
        if let Some(outputs) = self.world.resource::<Outputs<TextGeometry>>() {
            outputs.trim_text_memory();
            self.world
                .invalidate_query_dependency::<crate::text::TextLayoutHandle>();
            self.world.invalidate_query_dependency::<
                crate::ui::widgets::text_input::TextInputLayoutHandle,
            >();
        }
        self.factory.trim_memory();
        for plugin in &mut self.plugins {
            plugin.on_memory_warning(&mut self.world);
        }
        self.needs_full_first_frame = true;
        if let Some(root) = self.root {
            self.world.invalidate(root);
        }
        self.notify_app(crate::input::event::lifecycle::MemoryWarning);
    }

    fn notify_app<E: crate::input::event::lifecycle::AppNotice>(&mut self, notice: E) {
        if let Err(error) = crate::input::event::runtime::notify_app(&mut self.world, notice) {
            crate::warn!("App notification failed: {:?}", error);
        }
    }

    fn flush_event_notifications(&mut self) {
        if let Err(error) = crate::input::event::runtime::flush(&mut self.world) {
            crate::warn!("event notification flush failed: {:?}", error);
        }
    }

    fn ensure_started(&mut self) {
        if !self.started {
            self.started = true;
            for plugin in &mut self.plugins {
                plugin.on_start(&mut self.world);
            }
            self.notify_app(crate::input::event::lifecycle::Started);
        }
    }

    fn dispatch_gestures(&mut self) {
        let now_ms = (self.clock_ns() / 1_000_000) as u32;
        if let Err(error) = crate::input::event::gesture::system::drain_gestures(
            &mut self.world,
            |world, gesture| {
                crate::input::event::runtime::dispatch_gesture(world, gesture, now_ms)?;
                crate::input::event::runtime::flush(world).map(|_| ())
            },
        ) {
            crate::warn!("gesture batch dispatch failed: {:?}", error);
        }
    }

    fn finish_quit(&mut self) {
        if !self.quitting {
            self.quitting = true;
            let now_ms = (self.clock_ns() / 1_000_000) as u32;
            crate::input::event::cancel_active_input(&mut self.world, now_ms);
            self.dispatch_gestures();
            for plugin in &mut self.plugins {
                plugin.on_quit(&mut self.world);
            }
            self.notify_app(crate::input::event::lifecycle::Quit);
        }
    }

    fn apply_suspend_request(&mut self) {
        if let Some(request) = self
            .world
            .remove_resource::<crate::app::lifecycle::SuspendRequest>()
        {
            match request {
                crate::app::lifecycle::SuspendRequest::Suspend => self.suspend(),
                crate::app::lifecycle::SuspendRequest::Resume => self.resume(),
            }
        }
    }

    fn notify_raw_input(&mut self, event: &InputEvent) -> bool {
        let mut consumed = false;
        for plugin in &mut self.plugins {
            let accepted = plugin.on_event(&mut self.world, event);
            consumed |= accepted;
            if accepted && event.is_user_input() {
                break;
            }
        }
        consumed
    }

    fn clock_ns(&self) -> u64 {
        self.world
            .resource::<crate::ecs::MonoClock>()
            .map(|fc| (fc.clock)())
            .unwrap_or_else(crate::core::time::clock_now_ns)
    }

    /// Select a constructed root without rebuilding or reattaching its descendants.
    pub fn set_root(&mut self, root: Entity) {
        self.root = Some(root);
        self.world.insert_resource(crate::ui::WidgetRoot(root));
        crate::input::event::sim::set_sim_root(&mut self.world, root);
        self.world.reserve_dirty_traversal_stack();
    }

    /// Reclaim the backend by value, dropping the rest of the `App`.
    /// Lets a host rebuild a fresh `App` around the same surface (e.g.
    /// the web gallery swapping demos without recreating the canvas).
    pub fn into_backend(self) -> B {
        self.backend
    }

    /// Create the root widget with a fill-viewport default style and
    /// register it via [`set_root`][Self::set_root].
    ///
    /// Defaults: `grow: Fixed::ONE` (fills the viewport), background
    /// [`ColorToken::Surface`][crate::ui::theme::ColorToken::Surface],
    /// [`FlexDirection::Column`][crate::ui::layout::FlexDirection::Column],
    /// exclusion from pointer hit testing, and respect for the surface safe area.
    /// Chain [`RootBuilder::bg_color`] / [`RootBuilder::layout`] to
    /// override, then [`RootBuilder::id`] to finish:
    ///
    /// ```ignore
    /// let root = app.spawn_root().id();
    /// // or override the defaults:
    /// let root = app.spawn_root().bg_color(ColorToken::Primary).id();
    /// ```
    pub fn spawn_root(&mut self) -> RootBuilder<'_, B, F> {
        let entity = crate::ui::builder::WidgetBuilder::new(&mut self.world)
            .bg_color(crate::ui::theme::ColorToken::Surface)
            .layout(crate::ui::layout::LayoutStyle {
                direction: crate::ui::layout::FlexDirection::Column,
                grow: crate::types::Fixed::ONE,
                ..Default::default()
            })
            .id();
        self.world.insert(entity, crate::ui::IgnoreHitTest);
        self.world.insert(entity, crate::ui::RespectSafeArea);
        RootBuilder { app: self, entity }
    }

    /// Execute a composition under `parent` and return its output.
    ///
    /// The application activates its reactive owner for construction and prepares dirty storage after the composition returns.
    ///
    /// ```ignore
    /// let panel = app.compose(root, build_panel());
    /// ```
    pub fn compose<R>(
        &mut self,
        parent: crate::ecs::Entity,
        f: impl FnOnce(&mut crate::ui::UiScope<'_>) -> R,
    ) -> R {
        let output = {
            let mut cx = crate::ui::UiScope::new(&mut self.world, parent);
            f(&mut cx)
        };
        self.prepare_dirty_storage();
        output
    }

    fn prepare_dirty_storage(&mut self) {
        let slots = self.world.allocated_entity_slots();
        let live_entities = self.world.live_entity_count();
        if slots <= self.dirty_reserved_slots && live_entities <= self.dirty_reserved_live_entities
        {
            return;
        }
        let Some(max_entity_id) = slots.checked_sub(1) else {
            return;
        };
        let current = self
            .world
            .storage::<crate::ui::dirty::Dirty>()
            .map_or(0, |storage| storage.len());
        self.world
            .reserve_component_storage::<crate::ui::dirty::Dirty>(
                max_entity_id,
                live_entities.saturating_sub(current),
            );
        let visual = self
            .world
            .storage::<crate::ui::dirty::VisualDirty>()
            .map_or(0, |storage| storage.len());
        self.world
            .reserve_component_storage::<crate::ui::dirty::VisualDirty>(
                max_entity_id,
                live_entities.saturating_sub(visual),
            );
        self.dirty_reserved_slots = slots;
        self.dirty_reserved_live_entities = live_entities;
        self.world.reserve_dirty_traversal_stack();
    }

    fn prepare_render(&mut self, viewport: crate::types::Viewport) {
        self.prepare_dirty_storage();
        let viewport_changed = {
            let current = self
                .world
                .resource_mut::<RenderViewport>()
                .expect("App always owns a RenderViewport");
            let changed = current.0 != viewport;
            current.0 = viewport;
            changed
        };
        let safe_area = self.backend.safe_area_insets();
        let safe_area_changed = self
            .world
            .resource::<crate::surface::SafeAreaInsets>()
            .is_none_or(|current| *current != safe_area);
        if viewport_changed || safe_area_changed {
            // Geometry from the previous viewport is stale, but its layout
            // storage can be reused when the same root is recomputed.
            if let Some(snapshot) = self
                .world
                .resource_mut::<crate::ui::render_system::LayoutSnapshot>()
            {
                snapshot.invalidate();
            }
            if let Some(root) = self.root {
                self.world.mark_subtree_dirty(root);
            }
        }
        if safe_area_changed {
            self.world.insert_resource(safe_area);
        }
        for plugin in &mut self.plugins {
            plugin.pre_render(&mut self.world);
        }
        self.flush_event_notifications();
        self.prepare_dirty_storage();
        ViewRegistry::reconcile_observations(&mut self.world);
        crate::core::reactive::flush_signal_dirty(&mut self.world);
        crate::ui::offscreen::reconcile_texture_outputs(&self.world);
    }

    /// Render one frame
    #[mirui::trace_fn("frame.full")]
    pub fn render(&mut self) -> Result<(), crate::render::RenderError> {
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(&mut self.world);
        let Some(root) = self.root else { return Ok(()) };
        self.prepare_text_layout()
            .map_err(|_| crate::render::RenderError::TextPreparation)?;
        let transform = self.backend.viewport();
        self.prepare_text_raster_scratch(&transform);
        self.prepare_render(transform);
        if self.text_content_failure(true).is_some() {
            self.world.mark_subtree_dirty(root);
            return Err(crate::render::RenderError::TextContent);
        }
        let root_hidden = crate::ui::branch::is_effectively_hidden(&self.world, root);

        let layout_start = self.clock_ns();
        self.text_layout_started = true;

        {
            crate::trace_span!("frame.layout");
            render_system::try_collect_dirty_regions_into(
                &mut self.world,
                root,
                &transform,
                &mut self.dirty_plan,
            )
            .map_err(|_| crate::render::RenderError::TextLayout)?;
        }
        let layout_end = self.clock_ns();
        self.last_layout_ns = layout_end.saturating_sub(layout_start);

        let render_result = {
            crate::trace_span!("frame.render");
            let _outputs_batch = self
                .world
                .resource::<Outputs<WidgetTexture>>()
                .map(Outputs::begin_batch);
            let mut renderer = self.factory.make(&mut self.backend, &transform);
            if root_hidden {
                clear_hidden_root(&self.world, &transform, &mut renderer);
                Ok(())
            } else {
                render_system::render(&self.world, root, &transform, &mut renderer)
            }
        };
        if let Err(error) = render_result {
            self.world.mark_subtree_dirty(root);
            return Err(error);
        }
        self.dirty_plan.clear();
        let render_end = self.clock_ns();
        self.last_render_ns = render_end.saturating_sub(layout_end);
        let render_ns = render_end.saturating_sub(layout_start);

        let (pw, ph) = transform.physical_size();
        {
            crate::trace_span!("frame.flush");
            self.backend.begin_flush();
            self.backend.flush(PhysicalRect::from_size(pw, ph));
            self.backend.end_flush();
        }
        let flush_end = self.clock_ns();
        self.last_flush_ns = flush_end.saturating_sub(render_end);

        // Seed PrevRect so the next render_dirty frame's dirty union
        // covers pixels this full render actually wrote — otherwise
        // any widget that shrinks or moves between the full render
        // and the first dirty render leaves residue.
        {
            crate::trace_span!("frame.seed_prev");
            render_system::seed_prev_rects(&mut self.world, root, &transform);
        }
        self.last_seed_prev_ns = self.clock_ns().saturating_sub(flush_end);

        {
            crate::trace_span!("frame.finalize");
            self.finalize_frame_stats();
        }
        {
            crate::trace_span!("frame.post_render");
            for p in &mut self.plugins {
                p.post_render(&mut self.world, render_ns);
            }
        }
        Ok(())
    }

    fn finalize_frame_stats(&mut self) {
        // FrameTimings + FrameStats must land before plugin post_render
        // so reporters that read them (BudgetReportPlugin, custom sinks)
        // see the just-finished frame, not the previous one.
        let Some(pending) = self.pending_frame.take() else {
            return;
        };
        let input_nanos = pending.input_end.saturating_sub(pending.frame_start);
        let systems_nanos = pending.systems_end.saturating_sub(pending.input_end);
        let frame_nanos = input_nanos
            + systems_nanos
            + self.last_layout_ns
            + self.last_render_ns
            + self.last_flush_ns
            + self.last_seed_prev_ns;
        self.world.insert_resource(crate::ecs::FrameTimings {
            frame_nanos,
            input_nanos,
            systems_nanos,
            layout_nanos: self.last_layout_ns,
            render_nanos: self.last_render_ns,
            flush_nanos: self.last_flush_ns,
            seed_prev_nanos: self.last_seed_prev_ns,
        });
        let mut stats = self
            .world
            .resource_mut::<crate::ecs::FrameStats>()
            .map(core::mem::take)
            .unwrap_or_default();
        stats.push(frame_nanos);
        self.world.insert_resource(stats);

        // Publish after FrameTimings so a single post_render sees both.
        let snapshots = crate::core::cache::InspectCaches::inspect_caches(&self.backend)
            .map(|(_, c)| crate::core::cache::CacheStatsSnapshot::capture(c))
            .collect();
        self.world
            .insert_resource(crate::core::cache::CacheRegistry::from_snapshots(snapshots));
    }

    /// Get the dirty region in **logical pixels** after event processing,
    /// clearing dirty flags in the process. Use the backend's `viewport()`
    /// to obtain a clipped physical region.
    pub fn dirty_region(&mut self) -> Option<Rect> {
        let root = self.root?;
        let transform = self.backend.viewport();
        render_system::collect_dirty_region(&mut self.world, root, &transform)
    }

    /// Poll one event
    pub fn poll_event(&mut self) -> Option<InputEvent> {
        self.backend.poll_event()
    }

    /// Systems + render + poll until quit. Persistent backends take the
    /// `render_dirty` fast path; Transient backends render every frame.
    pub fn run(&mut self) {
        self.ensure_started();
        if let Err(error) = self.render() {
            crate::warn!("frame render failed: {:?}", error);
        }
        loop {
            if self.tick() {
                return;
            }
        }
    }

    /// One frame. Returns `true` on `Quit` (after `on_quit` hooks).
    pub fn tick(&mut self) -> bool {
        if self.quitting {
            return true;
        }
        self.ensure_started();
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(&mut self.world);
        if self.suspended {
            // Fan-out plugin on_event while suspended so AutoSuspendOnFocus-style
            // plugins can write SuspendRequest::Resume in response to lifecycle events.
            // Widget dispatch is skipped — suspended means no UI interaction.
            loop {
                match self.poll_event() {
                    Some(InputEvent::Quit) => {
                        self.finish_quit();
                        return true;
                    }
                    Some(event) => {
                        self.notify_raw_input(&event);
                        self.apply_suspend_request();
                        self.flush_event_notifications();
                        if !self.suspended {
                            break;
                        }
                    }
                    None => break,
                }
            }
            self.apply_suspend_request();
            self.flush_event_notifications();
            if self.suspended {
                #[cfg(feature = "std")]
                std::thread::sleep(core::time::Duration::from_millis(50));
                return false;
            }
            // Fell through resume() — continue into the normal frame body.
        }

        let transient =
            self.backend.persistence() == crate::surface::BackbufferPersistence::Transient;

        if let Some(gs) = self.world.resource_mut::<GestureSystem>() {
            gs.events.clear();
        }

        let frame_start = self.clock_ns();

        // Keep the DisplayInfo resource in step with the backend each frame;
        // systems that read it (not the per-event size) would otherwise see a
        // stale viewport after the surface changes size.
        let info = self.backend.display_info();
        self.world.insert_resource(info);

        let mut logical: Option<(u16, u16)> = None;
        let mut quit = false;

        {
            crate::trace_span!("frame.input");
            loop {
                match self.poll_event() {
                    Some(InputEvent::Quit) => {
                        quit = true;
                        break;
                    }
                    Some(event) => {
                        // Simulation owns interactive input, not host lifecycle requests.
                        let sim_running = self
                            .world
                            .resource::<crate::input::event::sim::SimTimeline>()
                            .is_some_and(|t| t.is_running());
                        if sim_running && event.is_user_input() {
                            continue;
                        }
                        let consumed = self.notify_raw_input(&event);
                        if !event.is_user_input()
                            && self
                                .world
                                .resource::<crate::app::lifecycle::SuspendRequest>()
                                .is_some()
                        {
                            self.apply_suspend_request();
                        } else if (!consumed || !event.is_user_input()) && !self.suspended {
                            let (lw, lh) = *logical.get_or_insert_with(|| {
                                let info = self.backend.display_info();
                                (info.width, info.height)
                            });
                            let now_ms = (self.clock_ns() / 1_000_000) as u32;
                            crate::input::event::dispatch_input(
                                &mut self.world,
                                self.root,
                                &event,
                                now_ms,
                                lw,
                                lh,
                            );
                        }
                        self.flush_event_notifications();
                        self.apply_suspend_request();
                    }
                    None => break,
                }
            }
            {
                let now_ms = (self.clock_ns() / 1_000_000) as u32;
                if let Some(gs) = self.world.resource_mut::<GestureSystem>() {
                    gs.recognizer.check_long_press(now_ms, &mut gs.events);
                }
            }

            self.dispatch_gestures();

            if quit {
                self.finish_quit();
                return true;
            }
            self.flush_event_notifications();
        }

        if self.suspended {
            return false;
        }

        let input_end = self.clock_ns();

        {
            crate::trace_span!("frame.systems");
            if let Err(error) = self.systems.run_all(&mut self.world) {
                crate::warn!("scheduled system failed: {:?}", error);
            }
        }
        self.snapshot_system_perf();
        self.flush_event_notifications();
        let systems_end = self.clock_ns();

        self.pending_frame = Some(PendingFrame {
            frame_start,
            input_end,
            systems_end,
        });
        if transient {
            if let Err(error) = self.render() {
                crate::warn!("frame render failed: {:?}", error);
            }
        } else {
            if let Err(error) = self.render_dirty() {
                crate::warn!("dirty frame render failed: {:?}", error);
            }
        }
        self.flush_event_notifications();
        self.backend.frame_end();

        self.apply_suspend_request();

        false
    }

    fn snapshot_system_perf(&mut self) {
        // PerfReportPlugin::build is the opt-in path for this resource.
        if self
            .world
            .resource::<crate::app::plugins::perf_report::SystemPerfSnapshot>()
            .is_none()
        {
            return;
        }

        let want_reset = self
            .world
            .resource::<crate::app::plugins::perf_report::PerfResetFlag>()
            .map(|f| f.0)
            .unwrap_or(false);
        if want_reset {
            self.systems.reset_perf();
            self.world
                .insert_resource(crate::app::plugins::perf_report::PerfResetFlag(false));
        }

        let snap = self
            .world
            .resource_mut::<crate::app::plugins::perf_report::SystemPerfSnapshot>()
            .expect("checked above");
        snap.entries.clear();
        for s in self.systems.iter() {
            let avg = if s.call_count == 0 {
                0
            } else {
                (s.total_us / s.call_count as u64) as u32
            };
            snap.entries
                .push(crate::app::plugins::perf_report::SystemStat {
                    name: s.name,
                    priority: s.priority(),
                    last_us: s.last_us,
                    avg_us: avg,
                    call_count: s.call_count,
                });
        }
    }

    /// Render only dirty regions. Falls back to full render if no dirty tracking.
    #[mirui::trace_fn("frame.dirty")]
    pub fn render_dirty(&mut self) -> Result<(), crate::render::RenderError> {
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(&mut self.world);
        let Some(root) = self.root else { return Ok(()) };
        self.prepare_text_layout()
            .map_err(|_| crate::render::RenderError::TextPreparation)?;
        let transform = self.backend.viewport();
        self.prepare_text_raster_scratch(&transform);
        self.prepare_render(transform);
        if self.text_content_failure(true).is_some() {
            self.world.mark_subtree_dirty(root);
            return Err(crate::render::RenderError::TextContent);
        }
        let root_hidden = crate::ui::branch::is_effectively_hidden(&self.world, root);

        let layout_start = self.clock_ns();
        self.text_layout_started = true;

        let force_full = self.needs_full_first_frame && self.backend.buffer_count() > 1;
        let mut plan = core::mem::take(&mut self.dirty_plan);
        if force_full {
            if render_system::try_update_layout(&mut self.world, root, &transform).is_err() {
                plan.clear();
                self.dirty_plan = plan;
                return Err(crate::render::RenderError::TextLayout);
            }
            plan.clear();
            let (lw, lh) = transform.logical_size();
            plan.rects.push(Rect::new(0, 0, lw, lh));
        } else {
            let collected = crate::trace_span!("frame.collect_dirty", {
                render_system::try_collect_dirty_regions_into(
                    &mut self.world,
                    root,
                    &transform,
                    &mut plan,
                )
            });
            if collected.is_err() {
                plan.clear();
                self.dirty_plan = plan;
                return Err(crate::render::RenderError::TextLayout);
            }
        }
        if root_hidden && self.needs_full_first_frame && plan.is_empty() {
            let (lw, lh) = transform.logical_size();
            plan.rects.push(Rect::new(0, 0, lw, lh));
        }
        let layout_end = self.clock_ns();
        self.last_layout_ns = layout_end.saturating_sub(layout_start);

        if !plan.is_empty() {
            crate::trace_span!("frame.render_region");
            let mut renderer = self.factory.make(&mut self.backend, &transform);

            let plan = if !renderer.supports_scroll_blit() {
                plan.flatten_shifts()
            } else {
                plan
            };
            let mut plan = plan;
            plan.coalesce_overlaps();

            if let Some(last) = self
                .world
                .resource_mut::<crate::ui::render_system::LastDirtyRegions>()
            {
                last.0.clone_from(&plan);
            } else {
                self.world
                    .insert_resource(crate::ui::render_system::LastDirtyRegions(plan.clone()));
            }
            let render_result = (|| {
                let requests = render_system::prepare_paint_requests(&self.world)?;
                let _outputs_batch = self
                    .world
                    .resource::<Outputs<WidgetTexture>>()
                    .map(Outputs::begin_batch);
                for sop in &plan.shifts {
                    renderer.scroll_target_region(&sop.area, sop.dx, sop.dy)?;
                }

                let snapshot = self
                    .world
                    .resource::<crate::ui::render_system::LayoutSnapshot>();
                let result = if root_hidden {
                    clear_hidden_root(&self.world, &transform, &mut renderer);
                    Ok(())
                } else if plan.prefers_split_redraw() {
                    let mut result = Ok(());
                    for rect in &plan.rects {
                        result = if let Some(snapshot) = snapshot {
                            render_system::render_region_cached(
                                &self.world,
                                snapshot,
                                rect,
                                &mut renderer,
                                &requests,
                            )
                        } else {
                            render_system::render_region_in_batch(
                                &self.world,
                                root,
                                &transform,
                                rect,
                                &mut renderer,
                                &requests,
                            )
                        };
                        if result.is_err() {
                            break;
                        }
                    }
                    result
                } else if let Some(union_rect) = plan
                    .rects
                    .iter()
                    .copied()
                    .reduce(|left, right| left.union(&right))
                {
                    if let Some(snapshot) = snapshot {
                        render_system::render_region_cached(
                            &self.world,
                            snapshot,
                            &union_rect,
                            &mut renderer,
                            &requests,
                        )
                    } else {
                        render_system::render_region_in_batch(
                            &self.world,
                            root,
                            &transform,
                            &union_rect,
                            &mut renderer,
                            &requests,
                        )
                    }
                } else {
                    Ok(())
                };
                result?;
                requests.commit();
                Ok(())
            })();

            drop(renderer);
            if let Err(error) = render_result {
                self.world.mark_subtree_dirty(root);
                self.dirty_plan = plan;
                return Err(error);
            }

            render_system::acknowledge_rendered_outputs(&self.world);

            let render_end = self.clock_ns();
            self.last_render_ns = render_end.saturating_sub(layout_end);
            let render_ns = render_end.saturating_sub(layout_start);
            {
                crate::trace_span!("frame.flush");
                self.backend.begin_flush();
                for rect in &plan.rects {
                    if let Some(physical) = transform.physical_rect(*rect) {
                        self.backend.flush(physical);
                    }
                }
                for sop in &plan.shifts {
                    if let Some(physical) = transform.physical_rect(sop.area) {
                        self.backend.flush(physical);
                    }
                }
                self.backend.end_flush();
            }
            self.factory
                .mirror_and_advance(&mut self.backend, &plan, &transform);
            self.needs_full_first_frame = false;
            self.last_flush_ns = self.clock_ns().saturating_sub(render_end);
            self.last_seed_prev_ns = 0;

            {
                crate::trace_span!("frame.finalize");
                self.finalize_frame_stats();
            }
            {
                crate::trace_span!("frame.post_render");
                for p in &mut self.plugins {
                    p.post_render(&mut self.world, render_ns);
                }
            }
            self.dirty_plan = plan;
            return Ok(());
        }

        // Idle frame: clear LastDirtyRegions so consumers (cursor
        // feedback, perf-plan-probe) can distinguish "this frame
        // produced no shift" from "stale plan from N frames ago".
        if let Some(last) = self
            .world
            .resource_mut::<crate::ui::render_system::LastDirtyRegions>()
        {
            last.0.clear();
        } else {
            self.world
                .insert_resource(crate::ui::render_system::LastDirtyRegions::default());
        }

        let render_end = self.clock_ns();
        self.last_render_ns = render_end.saturating_sub(layout_end);
        let render_ns = render_end.saturating_sub(layout_start);
        self.last_flush_ns = 0;
        self.last_seed_prev_ns = 0;
        {
            crate::trace_span!("frame.finalize");
            self.finalize_frame_stats();
        }
        {
            crate::trace_span!("frame.post_render");
            for p in &mut self.plugins {
                p.post_render(&mut self.world, render_ns);
            }
        }
        self.dirty_plan = plan;
        Ok(())
    }

    /// Consume into a [`Runner`].
    pub fn into_runner(self) -> Runner<B, F> {
        Runner { app: self }
    }
}

/// Chainable override for the root spawned by [`App::spawn_root`].
/// [`id`][Self::id] finalizes by registering the root via
/// [`App::set_root`].
pub struct RootBuilder<'a, B: Surface, F: RendererFactory<B>> {
    app: &'a mut App<B, F>,
    entity: Entity,
}

impl<B: Surface, F: RendererFactory<B>> RootBuilder<'_, B, F> {
    /// Override the root background, replacing the
    /// [`ColorToken::Surface`][crate::ui::theme::ColorToken::Surface] default.
    pub fn bg_color(self, color: impl Into<crate::ui::theme::ThemedColor>) -> Self {
        if let Some(style) = self.app.world.get_mut::<crate::ui::Style>(self.entity) {
            style.bg_color = Some(color.into());
        }
        self
    }

    /// Replace the root layout wholesale, dropping the
    /// fill-viewport `Column` default.
    pub fn layout(self, layout: crate::ui::layout::LayoutStyle) -> Self {
        if let Some(style) = self.app.world.get_mut::<crate::ui::Style>(self.entity) {
            style.layout = layout;
        }
        self
    }

    /// Allow content to occupy the complete surface, including system-reserved areas.
    pub fn ignore_safe_area(self) -> Self {
        self.app
            .world
            .remove::<crate::ui::RespectSafeArea>(self.entity);
        self
    }

    /// Initialize the completed root, select it via [`App::set_root`], and return its entity.
    pub fn id(self) -> Entity {
        crate::input::event::widget_input::attach_handlers_for(&mut self.app.world, self.entity);
        self.app.set_root(self.entity);
        self.entity
    }
}

/// Owns an [`App`] and drives [`tick`][App::tick] from a non-blocking
/// loop. Native: `Runner::run_blocking`. Wasm: `Runner::start_animation_frame`.
pub struct Runner<B: Surface, F: RendererFactory<B> = SwRendererFactory> {
    app: App<B, F>,
}

impl<B: Surface, F: RendererFactory<B>> Runner<B, F> {
    /// `App::run` with `-> !`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_blocking(mut self) -> ! {
        self.app.ensure_started();
        if let Err(error) = self.app.render() {
            crate::warn!("frame render failed: {:?}", error);
        }
        loop {
            if self.app.tick() {
                break;
            }
        }
        #[cfg(feature = "std")]
        {
            std::process::exit(0)
        }
        // no_std MCUs never return from main; spin.
        #[cfg(not(feature = "std"))]
        loop {
            core::hint::spin_loop()
        }
    }

    /// Drive `App::tick` from `requestAnimationFrame` and return so the
    /// wasm-bindgen `start` function can yield to the browser.
    #[cfg(all(target_arch = "wasm32", feature = "web-canvas"))]
    pub fn start_animation_frame(self)
    where
        B: 'static,
        F: 'static,
    {
        use alloc::rc::Rc;
        use core::cell::RefCell;
        Self::drive_animation_frame(Rc::new(RefCell::new(Some(self.app))));
    }

    /// Drive a shared, swappable `App` from `requestAnimationFrame`. The
    /// cell may hold `None` while a host swaps the `App` (e.g. the web
    /// gallery rebuilding for a new demo); those frames are skipped and
    /// the loop keeps rescheduling.
    #[cfg(all(target_arch = "wasm32", feature = "web-canvas"))]
    pub fn drive_animation_frame(app: alloc::rc::Rc<core::cell::RefCell<Option<App<B, F>>>>)
    where
        B: 'static,
        F: 'static,
    {
        use alloc::rc::Rc;
        use core::cell::RefCell;
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;

        let holder: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let holder_inner = holder.clone();

        *holder.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || {
            if let Some(app) = app.borrow_mut().as_mut() {
                if app.tick() {
                    return;
                }
            }
            let window = web_sys::window().expect("no global `window`");
            window
                .request_animation_frame(
                    holder_inner
                        .borrow()
                        .as_ref()
                        .expect("RAF closure")
                        .as_ref()
                        .unchecked_ref(),
                )
                .expect("requestAnimationFrame");
        }));

        let window = web_sys::window().expect("no global `window`");
        window
            .request_animation_frame(
                holder
                    .borrow()
                    .as_ref()
                    .expect("RAF closure")
                    .as_ref()
                    .unchecked_ref(),
            )
            .expect("requestAnimationFrame");
    }

    #[cfg(all(target_arch = "wasm32", not(feature = "web-canvas")))]
    pub fn start_animation_frame(self) -> ! {
        let _ = self.app;
        unimplemented!("Runner::start_animation_frame requires the `web-canvas` feature on wasm32");
    }
}

fn clone_texture_owned(
    src: &crate::render::texture::Texture<'static>,
) -> crate::render::texture::Texture<'static> {
    use crate::render::texture::{TexBuf, Texture};
    let mut owned = Texture::owned(src.width, src.height, src.format);
    if let TexBuf::Owned(ref mut dst) = owned.buf {
        dst.copy_from_slice(src.buf.as_slice());
    }
    owned
}

#[cfg(test)]
mod compose_entry_tests {
    use super::*;
    use crate::core::reactive::current_world_id;

    #[test]
    fn returns_entity_and_owned_results_after_preparing_dirty_storage() {
        let mut app = App::headless(8, 8);
        let root = app.spawn_root().id();
        let payload = alloc::vec![2_u8, 3, 5];
        let output = app.compose(root, |cx| {
            assert_eq!(current_world_id(), Some(cx.world_mut().id()));
            (cx.world_mut().spawn_empty(), payload)
        });

        assert!(app.world.is_alive(output.0));
        assert_eq!(output.1, [2, 3, 5]);
        assert_eq!(current_world_id(), None);
        let capacity = app
            .world
            .storage::<crate::ui::dirty::Dirty>()
            .unwrap()
            .reserved_entity_capacity();
        assert!(capacity.0 >= app.world.allocated_entity_slots() as usize);
        assert!(capacity.1 >= app.world.live_entity_count());
    }

    #[test]
    fn restores_enclosing_owner_after_return_and_unwind() {
        let mut outer = App::headless(8, 8);
        let outer_root = outer.spawn_root().id();
        let mut inner = App::headless(8, 8);
        let inner_root = inner.spawn_root().id();
        let outer_id = outer.world.id();
        let inner_id = inner.world.id();

        outer.compose(outer_root, |_cx| {
            assert_eq!(current_world_id(), Some(outer_id));
            assert_eq!(
                inner.compose(inner_root, |_cx| current_world_id()),
                Some(inner_id)
            );
            assert_eq!(current_world_id(), Some(outer_id));
            let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                inner.compose(inner_root, |_cx| panic!("composition unwind"));
            }));
            assert!(failure.is_err());
            assert_eq!(current_world_id(), Some(outer_id));
        });
        assert_eq!(current_world_id(), None);
    }

    #[test]
    fn repeated_invocation_adds_no_wrapper_allocations() {
        let mut app = App::headless(8, 8);
        let root = app.spawn_root().id();
        app.compose(root, |_cx| ());
        let allocations = crate::test_support::tracked_allocations(|| {
            for value in 0..64 {
                assert_eq!(app.compose(root, |_cx| value), value);
            }
        });
        assert_eq!(allocations, 0);
    }
}

#[cfg(test)]
mod multi_tap_initialization_tests {
    use super::*;
    use crate::input::event::gesture::GestureEvent;
    use crate::input::event::multi_tap::current_count;
    use crate::types::Fixed;

    #[test]
    fn with_factory_starts_with_an_empty_multi_tap_tracker() {
        let flush: HeadlessFlush = |_buf, _area| {};
        let surface = crate::surface::framebuf::FramebufSurface::new(8, 8, flush);
        let app = App::with_factory(surface, SwRendererFactory::new());

        assert!(
            app.world
                .resource::<MultiTapTracker>()
                .unwrap()
                .last
                .is_none()
        );
    }

    #[test]
    fn app_dispatch_counts_first_and_second_taps() {
        let mut app = App::headless(8, 8);
        let target = app.world.spawn_empty();
        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        };

        crate::input::event::runtime::dispatch_gesture(&mut app.world, &tap, 100).unwrap();
        assert_eq!(current_count(&app.world, target), 1);
        crate::input::event::runtime::dispatch_gesture(&mut app.world, &tap, 200).unwrap();
        assert_eq!(current_count(&app.world, target), 2);
    }
}

#[cfg(test)]
mod app_invariant_tests {
    use super::*;
    use crate::input::event::hit_test::hit_test;
    use crate::types::{Dimension, Fixed};
    use crate::ui::builder::WidgetBuilder;
    use crate::ui::dirty::Dirty;
    use crate::ui::layout::LayoutNode;
    use crate::ui::layout::LayoutStyle;
    use crate::ui::render_system::{LastDirtyRegions, LayoutSnapshot};
    use crate::ui::render_system::{TextLayoutFailureKind, TextLayoutStage};
    use crate::ui::widgets::text::Text;
    use crate::ui::{Children, HitTarget, Parent};

    #[test]
    fn text_raster_scratch_hints_share_one_maximum_extent() {
        let mut app = App::headless(8, 8);
        assert!(matches!(
            app.prefer_text_raster_scratch(0, 8),
            Err(crate::render::RenderError::InvalidGeometry)
        ));
        assert_eq!(app.text_raster_scratch_extent, None);

        app.prefer_text_raster_scratch(12, 4).unwrap();
        app.prefer_text_raster_scratch(8, 6).unwrap();
        assert_eq!(app.text_raster_scratch_extent, Some((12, 6)));
    }

    struct FailingScratchFactory {
        software: SwRendererFactory,
        fail: bool,
        requested_runs: usize,
    }

    impl<B: FramebufferAccess> RendererFactory<B> for FailingScratchFactory {
        type Renderer<'a>
            = crate::render::backends::sw::SwRenderer<'a>
        where
            Self: 'a,
            B: 'a;

        fn make<'a>(
            &'a mut self,
            backend: &'a mut B,
            viewport: &crate::types::Viewport,
        ) -> Self::Renderer<'a> {
            self.software.make(backend, viewport)
        }

        fn prepare_text_raster_scratch(
            &mut self,
            _viewport: &crate::types::Viewport,
            _width: u16,
            _height: u16,
            retained_runs: usize,
        ) -> Result<(), crate::render::RenderError> {
            self.requested_runs = retained_runs;
            if self.fail {
                Err(crate::render::RenderError::ResourceLimit(
                    crate::render::RenderResource::Target,
                ))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn optional_text_raster_scratch_failure_does_not_abort_frames() {
        let surface =
            crate::surface::framebuf::FramebufSurface::new(32, 32, (|_, _| {}) as HeadlessFlush);
        let mut app = App::with_factory(
            surface,
            FailingScratchFactory {
                software: SwRendererFactory::new(),
                fail: true,
                requested_runs: 0,
            },
        );
        let root = app.spawn_root().id();
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
            crate::ui! {
                Text ("A", id: "scratch_a", text_capacity: 1)
            }
        };
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
            crate::ui! {
                Text ("B", id: "scratch_b", text_capacity: 1)
            }
        };
        app.prefer_text_raster_scratch(8, 8).unwrap();

        app.render().unwrap();
        assert_eq!(app.factory.requested_runs, 2);
        assert_eq!(
            app.last_text_raster_scratch_preparation_error(),
            Some(crate::render::RenderError::ResourceLimit(
                crate::render::RenderResource::Target,
            )),
        );
        app.render_dirty().unwrap();
        app.factory.fail = false;
        app.render_dirty().unwrap();
        assert_eq!(app.last_text_raster_scratch_preparation_error(), None);
    }

    fn text_capacity(slots: usize, workspace_glyphs: usize) -> crate::text::TextLayoutCapacity {
        crate::text::TextLayoutCapacity {
            layout_slots: slots,
            measurements: 8,
            lines: 8,
            runs: 8,
            glyphs: 48,
            carets: 48,
            workspace: crate::text::WorkspaceCapacity {
                runs: 8,
                glyphs: workspace_glyphs,
                scratch_glyphs: 24,
                lines: 8,
            },
        }
    }

    #[test]
    fn text_requirements_merge_across_named_subtrees_before_layout() {
        let mut app = App::headless(64, 32);
        app.with_default_widgets();
        let left = text_capacity(3, 16);
        let right = text_capacity(4, 24);
        app.require_text_layout_capacity(left).unwrap();
        app.require_text_layout_capacity(right).unwrap();
        let combined = crate::text::TextLayoutCapacity {
            layout_slots: 7,
            measurements: 16,
            lines: 16,
            runs: 16,
            glyphs: 96,
            carets: 96,
            workspace: crate::text::WorkspaceCapacity {
                runs: 8,
                glyphs: 24,
                scratch_glyphs: 24,
                lines: 8,
            },
        };
        assert_eq!(
            app.text_layout_reservation,
            TextLayoutReservation::Pending(combined)
        );

        let root = app.spawn_root().id();
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
            crate::ui! {
                Column () {
                    Column (id: "left_tree") {
                        Text ("LEFT", id: "left_label")
                    }
                    Column (id: "right_tree") {
                        Text ("RIGHT", id: "right_label")
                    }
                }
            }
        };
        assert_ne!(
            app.world.find_by_id("left_tree"),
            app.world.find_by_id("right_tree")
        );
        app.prepare_text_layout().unwrap();
        assert_eq!(app.text_layout_reservation, TextLayoutReservation::Prepared);
        assert_eq!(
            app.world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage()
                .layouts
                .bounded_capacity(),
            Some(combined)
        );
        app.render().unwrap();
        assert_eq!(app.last_text_layout_preparation_error(), None);
        assert_eq!(app.last_text_layout_failure(), None);
        assert_eq!(
            app.require_text_layout_capacity(right).err(),
            Some(crate::text::TextLayoutError::InUse)
        );
    }

    #[test]
    fn text_requirement_overflow_and_failed_preparation_are_atomic() {
        let mut app = App::headless(64, 32);
        app.with_default_widgets();
        let first = text_capacity(3, 16);
        app.require_text_layout_capacity(first).unwrap();
        let overflow = crate::text::TextLayoutCapacity {
            layout_slots: usize::MAX,
            ..first
        };
        assert_eq!(
            app.require_text_layout_capacity(overflow).err(),
            Some(crate::text::TextLayoutError::DimensionOverflow)
        );
        assert_eq!(
            app.text_layout_reservation,
            TextLayoutReservation::Pending(first)
        );

        app.with_text_layout_limits(crate::text::TextLayoutLimits::HOST.with_cache_bytes(1024));
        assert_eq!(
            app.text_layout_reservation,
            TextLayoutReservation::Pending(first)
        );
        let root = app.spawn_root().id();
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
            crate::ui! {
                Text ("STORAGE", id: "storage_label")
            }
        };
        let error = app.prepare_text_layout().err().unwrap();
        assert!(matches!(
            error,
            crate::text::TextLayoutError::CacheBudget { .. }
        ));
        assert_eq!(app.last_text_layout_preparation_error(), Some(error));
        assert_eq!(app.last_text_layout_failure(), None);
        assert_eq!(
            app.world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage()
                .layouts
                .bounded_capacity(),
            None
        );
        assert_eq!(
            app.render(),
            Err(crate::render::RenderError::TextPreparation)
        );
        assert_eq!(
            app.render_dirty(),
            Err(crate::render::RenderError::TextPreparation)
        );
        assert_eq!(
            app.text_layout_reservation,
            TextLayoutReservation::Pending(first)
        );

        app.with_text_layout_limits(crate::text::TextLayoutLimits::HOST);
        app.prepare_text_layout().unwrap();
        assert_eq!(app.last_text_layout_preparation_error(), None);
        assert_eq!(
            app.world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage()
                .layouts
                .bounded_capacity(),
            Some(first)
        );
        assert!(matches!(
            app.try_with_text_layout_limits(
                crate::text::TextLayoutLimits::HOST.with_cache_bytes(1024)
            ),
            Err(crate::text::TextLayoutError::CacheBudget { .. })
        ));
        assert_eq!(
            app.world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage()
                .layouts
                .bounded_capacity(),
            Some(first)
        );
        app.render_dirty().unwrap();
    }

    #[test]
    fn text_absolute_capacity_and_requirements_cannot_be_mixed() {
        let mut app = App::headless(64, 32);
        let capacity = text_capacity(4, 24);
        app.with_text_layout_capacity(capacity).unwrap();
        app.with_text_layout_capacity(capacity).unwrap();
        assert_eq!(
            app.require_text_layout_capacity(capacity).err(),
            Some(crate::text::TextLayoutError::ConfigurationConflict)
        );
        let mut required = App::headless(64, 32);
        required.require_text_layout_capacity(capacity).unwrap();
        assert_eq!(
            required.with_text_layout_capacity(capacity).err(),
            Some(crate::text::TextLayoutError::ConfigurationConflict)
        );
        assert_eq!(
            required.text_layout_reservation,
            TextLayoutReservation::Pending(capacity)
        );
        required.prepare_text_layout().unwrap();
        assert_eq!(
            required.with_text_layout_capacity(capacity).err(),
            Some(crate::text::TextLayoutError::ConfigurationConflict)
        );
        assert_eq!(
            required.require_text_layout_capacity(capacity).err(),
            Some(crate::text::TextLayoutError::InUse)
        );
        let root = app.spawn_root().id();
        app.world.insert(root, crate::ui::dirty::Dirty);
        app.render().unwrap();
        assert_eq!(
            app.with_text_layout_capacity(capacity).err(),
            Some(crate::text::TextLayoutError::InUse)
        );
    }

    #[test]
    fn text_requirement_cannot_follow_unbounded_layout_even_after_limit_replacement() {
        let mut app = App::headless(64, 32);
        app.with_default_widgets();
        app.spawn_root().id();
        app.render().unwrap();
        app.with_text_layout_limits(crate::text::TextLayoutLimits::HOST);
        assert_eq!(
            app.require_text_layout_capacity(text_capacity(4, 24)).err(),
            Some(crate::text::TextLayoutError::InUse)
        );
        assert_eq!(
            app.with_text_layout_capacity(text_capacity(4, 24)).err(),
            Some(crate::text::TextLayoutError::InUse)
        );
    }

    #[test]
    fn layout_limit_replacement_resets_path_frames_only_after_success() {
        use crate::text::baseline::{DEFAULT_TOLERANCE, PlacementKey};
        use crate::text::{TextLayoutHandle, TextPath};

        fn frame_counts(world: &World, key: PlacementKey) -> (Option<usize>, Option<usize>) {
            let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
            let baselines = &storage.paths;
            (
                baselines.glyph_frames(key, <[_]>::len),
                baselines.caret_frames(key, <[_]>::len),
            )
        }

        for bounded in [false, true] {
            let mut app = App::headless(64, 32);
            app.with_default_widgets();
            if bounded {
                app.with_text_layout_capacity(text_capacity(2, 8)).unwrap();
            }
            let mut baseline = crate::render::path::Path::new();
            baseline
                .move_to(crate::types::Point::new(0, 16))
                .line_to(crate::types::Point::new(64, 16));
            let path = app.paths().insert(baseline).unwrap();
            let root = app.spawn_root().id();
            let label = app.compose(root, |cx| cx.spawn(Text::build("A").path(path)));
            app.render().unwrap();
            let handle = *app.world.get::<TextLayoutHandle>(label).unwrap();
            let key = PlacementKey::new(
                label,
                handle,
                TextPath::new(path),
                app.paths().revision(path).unwrap(),
                DEFAULT_TOLERANCE,
            );
            assert_eq!(frame_counts(&app.world, key), (Some(1), Some(2)));

            if bounded {
                assert_eq!(
                    app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
                        .err(),
                    Some(crate::text::TextLayoutError::InUse)
                );
                assert_eq!(frame_counts(&app.world, key), (Some(1), Some(2)));
            } else {
                app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
                    .unwrap();
                assert_eq!(frame_counts(&app.world, key), (None, None));
                app.world
                    .get_mut::<Text>(label)
                    .unwrap()
                    .set_content("ABCD");
                app.render_dirty().unwrap();
                assert_eq!(app.world.get::<TextLayoutHandle>(label), Some(&handle));
                assert_eq!(frame_counts(&app.world, key), (Some(4), Some(5)));
            }
        }
    }

    #[test]
    fn input_layout_is_prepared_after_cache_replacement_without_manual_invalidation() {
        use crate::ui::widgets::TextInput;
        use crate::ui::widgets::text_input::TextInputLayoutHandle;

        let glyph_count = |world: &World, handle| {
            world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage()
                .layouts
                .get(handle)
                .map(|layout| layout.glyphs().len())
        };
        let mut app = App::headless(64, 24);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let input = app.compose(root, |cx| {
            cx.spawn(
                TextInput::build()
                    .placeholder("ABC")
                    .style(crate::ui::Style {
                        layout: LayoutStyle {
                            width: Dimension::Px(Fixed::from_int(48)),
                            height: Dimension::Px(Fixed::from_int(16)),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            )
        });
        app.render().unwrap();
        app.render_dirty().unwrap();
        let handle = app.world.get::<TextInputLayoutHandle>(input).unwrap().0;
        assert_eq!(glyph_count(&app.world, handle), Some(3));

        app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
            .unwrap();
        assert_eq!(glyph_count(&app.world, handle), None);
        app.render_dirty().unwrap();
        let replacement = app.world.get::<TextInputLayoutHandle>(input).unwrap().0;
        assert_eq!(replacement, handle);
        assert_eq!(glyph_count(&app.world, replacement), Some(3));

        app.handle_memory_warning();
        assert_eq!(glyph_count(&app.world, replacement), None);
        app.render_dirty().unwrap();
        let restored = app.world.get::<TextInputLayoutHandle>(input).unwrap().0;
        assert_eq!(glyph_count(&app.world, restored), Some(3));
    }

    #[test]
    fn font_manager_replacement_refreshes_input_layout_in_a_dirty_frame() {
        use crate::render::font::{
            Font, FontBackend, FontFaceId, FontMetrics, FontProvider, FontToken, GlyphId,
            RasterGlyph, default_font_manager,
        };
        use crate::ui::widgets::TextInput;
        use crate::ui::widgets::text_input::TextInputLayoutHandle;

        struct SizedFont;
        impl FontProvider for SizedFont {
            fn face_id(&self) -> FontFaceId {
                FontFaceId::new(404)
            }
            fn map_char(&self, ch: char) -> Option<GlyphId> {
                Some(GlyphId::new(ch as u16))
            }
            fn glyph_advance(&self, _: GlyphId, ppem: u16) -> Option<Fixed> {
                Some(Fixed::from_int(i32::from(ppem)))
            }
            fn raster(&self, _: GlyphId, _: u16, _: u16) -> Option<RasterGlyph<'_>> {
                None
            }
            fn metrics(&self, ppem: u16) -> FontMetrics {
                FontMetrics {
                    ascender: Fixed::from_int(i32::from(ppem)),
                    descender: Fixed::ZERO,
                    line_height: Fixed::from_int(i32::from(ppem)),
                }
            }
        }
        let provider = alloc::rc::Rc::new(SizedFont);
        let fonts_at_size = |size| {
            let fonts = default_font_manager();
            fonts.add_static(
                FontToken::Default.cache_key(),
                Font {
                    family: "input-metrics",
                    size,
                    backend: FontBackend::Custom(provider.clone()),
                },
            );
            fonts
        };
        let mut app = App::headless(64, 32);
        app.with_default_widgets().with_fonts(fonts_at_size(8));
        let root = app.spawn_root().id();
        let input = app.compose(root, |cx| {
            cx.spawn(
                TextInput::build()
                    .placeholder("AB")
                    .style(crate::ui::Style {
                        layout: LayoutStyle {
                            width: Dimension::Px(Fixed::from_int(56)),
                            height: Dimension::Px(Fixed::from_int(24)),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            )
        });
        let input_layout = |world: &World| {
            let handle = world.get::<TextInputLayoutHandle>(input).unwrap().0;
            let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
            (handle, storage.layouts.get(handle).unwrap().measure())
        };
        app.render().unwrap();
        app.render_dirty().unwrap();
        let (original, before) = input_layout(&app.world);
        assert_eq!(before.width, 16 << 8);

        app.with_fonts(fonts_at_size(16));
        app.render_dirty().unwrap();
        let (replacement, after) = input_layout(&app.world);
        assert_ne!(replacement, original);
        assert_eq!(after.width, before.width * 2);
        assert_eq!(after.height, before.height * 2);
    }

    #[test]
    fn pending_text_requirement_cannot_discard_a_low_level_layout() {
        let mut app = App::headless(64, 32);
        app.with_default_widgets();
        let capacity = text_capacity(4, 24);
        app.require_text_layout_capacity(capacity).unwrap();
        let root = app.spawn_root().id();
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
            crate::ui! {
                Text ("LOW LEVEL", id: "low_level")
            }
        };
        crate::ui::render_system::try_update_layout(&mut app.world, root, &app.backend.viewport())
            .unwrap();
        assert_eq!(
            app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
                .err(),
            Some(crate::text::TextLayoutError::InUse)
        );
        assert_eq!(
            app.prepare_text_layout().err(),
            Some(crate::text::TextLayoutError::InUse)
        );
        assert_eq!(
            app.render(),
            Err(crate::render::RenderError::TextPreparation)
        );
        assert_eq!(
            app.text_layout_reservation,
            TextLayoutReservation::Pending(capacity)
        );
    }

    #[test]
    fn direct_render_prepares_pending_text_capacity_before_layout() {
        let mut app = App::headless(64, 32);
        app.with_default_widgets();
        app.require_text_layout_capacity(text_capacity(4, 24))
            .unwrap();
        let root = app.spawn_root().id();
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
            crate::ui! {
                Text ("READY", id: "auto_prepared")
            }
        };
        app.render().unwrap();
        assert_eq!(app.text_layout_reservation, TextLayoutReservation::Prepared);
        assert_eq!(app.last_text_layout_preparation_error(), None);
        assert_eq!(app.last_text_layout_failure(), None);
    }

    #[test]
    fn zero_text_requirement_is_bounded_and_rejects_later_text() {
        let mut app = App::headless(64, 32);
        app.with_default_widgets();
        let capacity = crate::text::TextLayoutCapacity::default();
        app.require_text_layout_capacity(capacity).unwrap();
        let root = app.spawn_root().id();
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
            crate::ui! {
                Text ("LATE", id: "late_text")
            }
        };
        assert_eq!(app.render(), Err(crate::render::RenderError::TextLayout));
        assert_eq!(app.last_text_layout_preparation_error(), None);
        assert!(matches!(
            app.last_text_layout_failure().unwrap().kind,
            TextLayoutFailureKind::Text(crate::text::TextLayoutError::Capacity { .. })
        ));
        assert_eq!(
            app.world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage()
                .layouts
                .bounded_capacity(),
            Some(capacity)
        );
    }

    #[test]
    fn text_limit_failure_keeps_the_last_frame_and_recovers_in_full_and_dirty_render() {
        let mut app = App::headless(48, 24);
        app.with_default_widgets();
        app.with_text_layout_limits(crate::text::TextLayoutLimits {
            text_bytes: 2,
            ..crate::text::TextLayoutLimits::HOST
        });
        let root = app.spawn_root().id();
        let label = WidgetBuilder::new(&mut app.world)
            .layout(LayoutStyle {
                width: Dimension::Content,
                height: Dimension::Content,
                ..LayoutStyle::default()
            })
            .id();
        app.world.insert(label, Text::from("OK"));
        app.world.insert(label, Parent(root));
        app.world.get_mut::<Children>(root).unwrap().0.push(label);

        app.render().unwrap();
        let before = app.backend.framebuffer().buf.as_slice().to_vec();
        let snapshot_ptr = app.world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;

        app.world
            .get_mut::<Text>(label)
            .unwrap()
            .set_content("LONG");
        assert_eq!(app.render(), Err(crate::render::RenderError::TextLayout));
        assert_eq!(app.backend.framebuffer().buf.as_slice(), before);
        assert_eq!(
            app.last_text_layout_failure(),
            Some(crate::ui::render_system::TextLayoutFailure {
                entity: label,
                stage: TextLayoutStage::Measure,
                kind: TextLayoutFailureKind::Text(crate::text::TextLayoutError::TextLimit {
                    required: 4,
                    limit: 2,
                }),
            })
        );
        assert_eq!(
            app.world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
            snapshot_ptr,
        );
        assert!(!crate::input::event::hit_test::geometry_matches(
            &app.world, root, 48, 24
        ));

        app.world.get_mut::<Text>(label).unwrap().set_content("OK");
        app.render().unwrap();
        assert_eq!(app.last_text_layout_failure(), None);

        app.world
            .get_mut::<Text>(label)
            .unwrap()
            .set_content("LONG");
        app.world.insert(root, Dirty);
        let before = app.backend.framebuffer().buf.as_slice().to_vec();
        assert_eq!(
            app.render_dirty(),
            Err(crate::render::RenderError::TextLayout)
        );
        assert_eq!(app.backend.framebuffer().buf.as_slice(), before);
        assert!(app.world.has::<Dirty>(root));
        assert_eq!(app.last_text_layout_failure().unwrap().entity, label);

        app.world.get_mut::<Text>(label).unwrap().set_content("OK");
        app.render_dirty().unwrap();
        assert_eq!(app.last_text_layout_failure(), None);
        assert!(crate::input::event::hit_test::geometry_matches(
            &app.world, root, 48, 24
        ));

        let mut baseline = crate::render::path::Path::new();
        baseline
            .move_to(crate::types::Point::new(0, 16))
            .line_to(crate::types::Point::new(40, 16));
        let path = app
            .world
            .resource_mut::<crate::render::path::PathStore>()
            .unwrap()
            .insert(baseline)
            .unwrap();
        app.world.widget_mut(label).unwrap().text_path(path);
        app.world
            .get_mut::<Text>(label)
            .unwrap()
            .set_content("LONG");
        app.world.insert(root, Dirty);
        let before = app.backend.framebuffer().buf.as_slice().to_vec();
        assert_eq!(
            app.render_dirty(),
            Err(crate::render::RenderError::TextLayout)
        );
        assert_eq!(app.backend.framebuffer().buf.as_slice(), before);
        assert_eq!(app.last_text_layout_failure().unwrap().entity, label);

        app.world.get_mut::<Text>(label).unwrap().set_content("OK");
        app.render_dirty().unwrap();
        assert_eq!(app.last_text_layout_failure(), None);
    }

    #[test]
    fn text_layout_budget_failure_reports_the_layout_stage_and_recovers() {
        let mut app = App::headless(48, 24);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let label = WidgetBuilder::new(&mut app.world)
            .layout(LayoutStyle {
                width: Dimension::Content,
                height: Dimension::Content,
                ..LayoutStyle::default()
            })
            .id();
        app.world.insert(label, Text::from("LABEL"));
        app.world.insert(label, Parent(root));
        app.world.get_mut::<Children>(root).unwrap().0.push(label);

        let mut node = LayoutNode::new(LayoutStyle::default());
        crate::ui::render_system::apply_text_intrinsic(&app.world, label, &mut node).unwrap();
        let measure_budget = app
            .world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .layouts
            .resident_bytes();
        app.with_text_layout_limits(
            crate::text::TextLayoutLimits::HOST.with_cache_bytes(measure_budget),
        );

        assert_eq!(app.render(), Err(crate::render::RenderError::TextLayout));
        let failure = app.last_text_layout_failure().unwrap();
        assert_eq!(failure.entity, label);
        assert_eq!(failure.stage, TextLayoutStage::Layout);
        assert!(matches!(
            failure.kind,
            TextLayoutFailureKind::Text(crate::text::TextLayoutError::CacheBudget { .. })
        ));

        app.with_text_layout_limits(crate::text::TextLayoutLimits::HOST);
        app.render().unwrap();
        assert_eq!(app.last_text_layout_failure(), None);
    }

    #[test]
    fn active_and_idle_frames_retain_plan_storage() {
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let target = WidgetBuilder::new(&mut app.world)
            .layout(LayoutStyle {
                width: Dimension::px(16),
                height: Dimension::px(16),
                ..LayoutStyle::default()
            })
            .id();
        app.world.insert(target, HitTarget);
        app.world.insert(target, Parent(root));
        app.world.get_mut::<Children>(root).unwrap().0.push(target);

        app.world.insert(root, Dirty);
        app.render_dirty().unwrap();
        assert!(crate::input::event::hit_test::geometry_matches(
            &app.world, root, 32, 32,
        ));
        assert_eq!(
            app.world.get::<crate::ui::ComputedRect>(root).unwrap().0,
            Rect::new(0, 0, 32, 32),
        );
        assert_eq!(
            hit_test(
                &app.world,
                root,
                Fixed::from_int(4),
                Fixed::from_int(4),
                32,
                32,
            ),
            Some(target),
            "the persistent render path must publish input geometry",
        );
        let scratch_ptr = app.dirty_plan.rects.as_ptr();
        let last = app.world.resource::<LastDirtyRegions>().unwrap();
        let last_ptr = last as *const LastDirtyRegions;
        let last_rects_ptr = last.0.rects.as_ptr();
        assert_eq!(last.0.rects.len(), 1);

        app.world.insert(root, Dirty);
        app.render_dirty().unwrap();
        let last = app.world.resource::<LastDirtyRegions>().unwrap();
        assert_eq!(app.dirty_plan.rects.as_ptr(), scratch_ptr);
        assert_eq!(last as *const LastDirtyRegions, last_ptr);
        assert_eq!(last.0.rects.as_ptr(), last_rects_ptr);

        app.render_dirty().unwrap();
        let last = app.world.resource::<LastDirtyRegions>().unwrap();
        assert!(last.0.is_empty());
        assert_eq!(app.dirty_plan.rects.as_ptr(), scratch_ptr);
        assert_eq!(last.0.rects.as_ptr(), last_rects_ptr);
    }

    #[test]
    fn viewport_change_invalidates_retained_layout_snapshot() {
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.world.insert(root, Dirty);
        app.render().unwrap();
        let snapshot_ptr = app.world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;

        let current = app.world.resource::<RenderViewport>().unwrap().0;
        app.prepare_render(crate::types::Viewport::new(64, 32, current.scale()));

        assert_eq!(
            app.world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
            snapshot_ptr
        );
        assert!(app.world.has::<Dirty>(root));
    }

    #[test]
    fn resized_app_reuses_snapshot_and_updates_pixels_and_hit_geometry() {
        use crate::surface::framebuf::FramebufSurface;
        use crate::types::Color;

        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let target = WidgetBuilder::new(&mut app.world)
            .layout(LayoutStyle {
                width: Dimension::percent(50),
                height: Dimension::px(16),
                ..LayoutStyle::default()
            })
            .id();
        app.world
            .get_mut::<crate::ui::Style>(target)
            .unwrap()
            .set_bg_color(Color::rgb(240, 20, 30));
        app.world.insert(target, HitTarget);
        app.world.insert(target, Parent(root));
        app.world.get_mut::<Children>(root).unwrap().0.push(target);

        app.render().unwrap();
        let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
        let snapshot_ptr = snapshot as *const LayoutSnapshot;
        let entities_ptr = snapshot.entities.as_ptr();
        assert_eq!(
            hit_test(
                &app.world,
                root,
                Fixed::from_int(24),
                Fixed::from_int(4),
                32,
                32
            ),
            None
        );

        let cb: HeadlessFlush = |_buf, _area| {};
        app.backend = FramebufSurface::new(64, 32, cb);
        app.render_dirty().unwrap();

        let resized = app.world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(resized as *const LayoutSnapshot, snapshot_ptr);
        assert_eq!(resized.entities.as_ptr(), entities_ptr);
        assert_eq!(
            app.world.get::<crate::ui::ComputedRect>(root).unwrap().0,
            Rect::new(0, 0, 64, 32)
        );
        assert_eq!(
            app.world
                .get::<crate::ui::ComputedRect>(target)
                .unwrap()
                .0
                .w,
            Fixed::from_int(32)
        );
        assert_eq!(
            hit_test(
                &app.world,
                root,
                Fixed::from_int(24),
                Fixed::from_int(4),
                64,
                32
            ),
            Some(target)
        );
        let texture = app.backend.framebuffer();
        let pixel = (4 * 64 + 24) * 4;
        assert_eq!(&texture.buf.as_slice()[pixel..pixel + 3], &[240, 20, 30]);
    }

    #[test]
    fn hiding_root_clears_previous_pixels_once() {
        use crate::types::Color;
        use crate::ui::Hidden;

        let mut app = App::headless(16, 16);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.world
            .get_mut::<crate::ui::Style>(root)
            .unwrap()
            .set_bg_color(Color::rgb(240, 20, 30));

        app.world.insert(root, Dirty);
        app.render_dirty().unwrap();
        assert_eq!(
            &app.backend.framebuffer().buf.as_slice()[..4],
            &[240, 20, 30, 255]
        );

        app.world.insert(root, Hidden);
        app.render_dirty().unwrap();
        let surface = app
            .world
            .resource::<Theme>()
            .unwrap()
            .resolve(crate::ui::theme::ColorToken::Surface);
        assert_eq!(
            &app.backend.framebuffer().buf.as_slice()[..4],
            &[surface.r, surface.g, surface.b, surface.a]
        );
        assert_eq!(
            app.world.resource::<LastDirtyRegions>().unwrap().0.rects,
            [Rect::new(0, 0, 16, 16)]
        );

        app.render_dirty().unwrap();
        assert!(
            app.world
                .resource::<LastDirtyRegions>()
                .unwrap()
                .0
                .is_empty()
        );

        app.world.remove::<Hidden>(root);
        app.world.insert(root, Dirty);
        app.render_dirty().unwrap();
        assert_eq!(
            &app.backend.framebuffer().buf.as_slice()[..3],
            &[240, 20, 30]
        );

        app.world.insert(root, Hidden);
        app.render().unwrap();
        assert_eq!(
            &app.backend.framebuffer().buf.as_slice()[..4],
            &[surface.r, surface.g, surface.b, surface.a]
        );
    }
}

#[cfg(test)]
mod queued_surface {
    use super::*;
    use crate::core::cache::InspectCaches;
    use crate::render::texture::Texture;
    use crate::surface::framebuf::FramebufSurface;
    use crate::surface::{DisplayInfo, FramebufferAccess, InputEvent};
    use crate::types::{Fixed, PhysicalRect};
    use alloc::collections::VecDeque;

    type FlushFn = fn(&[u8], PhysicalRect);

    pub(super) struct QueuedSurface {
        framebuffer: FramebufSurface<FlushFn>,
        pub(super) events: VecDeque<InputEvent>,
    }

    impl QueuedSurface {
        pub(super) fn new(physical_w: u16, physical_h: u16, scale: Fixed) -> Self {
            fn flush(_: &[u8], _: PhysicalRect) {}
            Self {
                framebuffer: FramebufSurface::with_scale(physical_w, physical_h, scale, flush),
                events: VecDeque::new(),
            }
        }
    }

    impl InspectCaches for QueuedSurface {}

    impl Surface for QueuedSurface {
        fn display_info(&self) -> DisplayInfo {
            self.framebuffer.display_info()
        }

        fn physical_size(&self) -> (u32, u32) {
            self.framebuffer.physical_size()
        }

        fn flush(&mut self, area: PhysicalRect) {
            self.framebuffer.flush(area);
        }

        fn poll_event(&mut self) -> Option<InputEvent> {
            self.events.pop_front()
        }
    }

    impl FramebufferAccess for QueuedSurface {
        fn framebuffer(&mut self) -> Texture<'_> {
            self.framebuffer.framebuffer()
        }
    }
}

#[cfg(test)]
mod fractional_scale_input_check {
    use super::queued_surface::QueuedSurface;
    use super::*;
    use crate::input::event::contract::Event;
    use crate::input::event::gesture::Tap;
    use crate::types::{Dimension, Fixed};
    use crate::ui::builder::WidgetBuilder;
    use crate::ui::layout::LayoutStyle;
    use crate::ui::{Children, HitTarget, Parent};

    #[derive(Default)]
    struct TapCount(u8);

    #[crate::event]
    fn count_tap(count: res!(mut TapCount)) -> bool {
        count.0 += 1;
        true
    }

    #[test]
    fn fractional_scale_input_uses_the_layout_logical_size() {
        let surface = QueuedSurface::new(128, 277, Fixed::from_f32(3.25));
        let mut app = App::with_factory(surface, SwRendererFactory::new());
        app.with_default_widgets().with_default_systems();
        app.world.insert_resource(TapCount::default());
        let root = app.spawn_root().id();
        let target = WidgetBuilder::new(&mut app.world)
            .layout(LayoutStyle {
                width: Dimension::px(32),
                height: Dimension::px(32),
                ..LayoutStyle::default()
            })
            .id();
        app.world.insert(target, HitTarget);
        crate::input::event::binding::install_entity_event(
            &mut app.world,
            target,
            Tap::EVENT,
            count_tap::callback::<Tap, crate::input::event::callback::NodeContext>(),
        );
        app.world.insert(target, Parent(root));
        app.world.get_mut::<Children>(root).unwrap().0.push(target);

        app.render().unwrap();
        app.backend.events.extend([
            InputEvent::PointerDown {
                id: 0,
                x: Fixed::from_int(8),
                y: Fixed::from_int(8),
            },
            InputEvent::PointerUp {
                id: 0,
                x: Fixed::from_int(8),
                y: Fixed::from_int(8),
            },
        ]);
        let original = app
            .world
            .resource::<GestureSystem>()
            .unwrap()
            .events
            .buffer
            .as_ptr();
        app.tick();

        assert_eq!(app.world.resource::<TapCount>().unwrap().0, 1);
        assert_ne!(
            app.world
                .resource::<GestureSystem>()
                .unwrap()
                .events
                .buffer
                .as_ptr(),
            original
        );
        for _ in 0..3 {
            app.backend.events.extend([
                InputEvent::PointerDown {
                    id: 0,
                    x: Fixed::from_int(8),
                    y: Fixed::from_int(8),
                },
                InputEvent::PointerUp {
                    id: 0,
                    x: Fixed::from_int(8),
                    y: Fixed::from_int(8),
                },
            ]);
        }
        app.tick();
        assert_eq!(app.world.resource::<TapCount>().unwrap().0, 4);
        assert_eq!(
            app.world
                .resource::<GestureSystem>()
                .unwrap()
                .events
                .buffer
                .as_ptr(),
            original
        );
    }
}

#[cfg(test)]
mod swap_tests {
    use super::*;

    #[test]
    fn into_backend_reuse_keeps_systems_bounded() {
        let mut app = App::headless(64, 64);
        app.with_default_widgets().with_default_systems();
        let baseline = app.systems.iter().count();
        assert!(baseline > 0);

        for _ in 0..5 {
            let backend = app.into_backend();
            app = App::new(backend);
            app.with_default_widgets().with_default_systems();
            assert_eq!(
                app.systems.iter().count(),
                baseline,
                "rebuilding the App must not accumulate systems",
            );
        }
    }

    #[test]
    fn into_backend_resets_root() {
        let mut app = App::headless(64, 64);
        let root = app.spawn_root().id();
        app.set_root(root);
        assert!(app.root.is_some());

        let backend = app.into_backend();
        let app = App::new(backend);
        assert!(app.root.is_none());
    }

    #[test]
    fn spawned_root_does_not_intercept_pointer_input() {
        let mut app = App::headless(64, 64);
        let root = app.spawn_root().id();
        assert!(app.world.get::<crate::ui::IgnoreHitTest>(root).is_some());
    }

    #[test]
    fn spawned_root_respects_safe_area_by_default() {
        let mut app = App::headless(64, 64);
        let root = app.spawn_root().id();
        assert!(app.world.get::<crate::ui::RespectSafeArea>(root).is_some());
    }

    #[test]
    fn root_can_opt_out_of_safe_area() {
        let mut app = App::headless(64, 64);
        let root = app.spawn_root().ignore_safe_area().id();
        assert!(app.world.get::<crate::ui::RespectSafeArea>(root).is_none());
    }

    #[test]
    fn swap_cycle_leaves_a_rooted_app() {
        use alloc::rc::Rc;
        use core::cell::RefCell;

        let mut first = App::headless(64, 64);
        let root = first.spawn_root().id();
        first.set_root(root);
        let cell = Rc::new(RefCell::new(Some(first)));

        for _ in 0..5 {
            let old = cell.borrow_mut().take().expect("app present before swap");
            let backend = old.into_backend();
            let mut next = App::new(backend);
            let root = next.spawn_root().id();
            next.set_root(root);
            *cell.borrow_mut() = Some(next);

            let guard = cell.borrow();
            let app = guard
                .as_ref()
                .expect("swap must restore the app, never leave None");
            assert!(app.root.is_some(), "rebuilt app must have a root");
        }
    }

    #[test]
    fn path_capacity_changes_preserve_retired_identities_and_enforce_limits() {
        use crate::render::path::{Path, PathStoreError};

        let mut app = App::headless(32, 32);
        app.with_path_capacity(3).unwrap();
        let retired = core::array::from_fn::<_, 3, _>(|_| app.paths().insert(Path::new()).unwrap());
        for id in retired {
            app.paths().remove(id).unwrap();
        }
        for capacity in [0, 1, 3, 1] {
            app.with_path_capacity(capacity).unwrap();
            let mut current = [None; 3];
            for slot in current.iter_mut().take(capacity) {
                *slot = Some(app.paths().insert(Path::new()).unwrap());
            }
            assert_eq!(
                app.paths().insert(Path::new()),
                Err(PathStoreError::Full { capacity }.into())
            );
            for id in retired {
                assert_eq!(
                    app.paths().with(id, |_| ()),
                    Err(PathStoreError::Missing(id).into())
                );
                assert_eq!(
                    app.paths().edit(id, |_| ()),
                    Err(PathStoreError::Missing(id).into())
                );
                assert_eq!(
                    app.paths().remove(id).unwrap_err(),
                    PathStoreError::Missing(id).into()
                );
            }
            for id in current.into_iter().flatten() {
                app.paths().remove(id).unwrap();
            }
        }
    }

    #[test]
    fn path_capacity_cannot_replace_a_live_store() {
        let mut app = App::headless(64, 64);
        let path = app
            .paths()
            .insert(crate::render::path::Path::new())
            .unwrap();

        let result = app.with_path_capacity(8);
        assert!(matches!(
            result,
            Err(crate::render::path::PathStoreError::InUse { paths: 1 })
        ));
        assert!(app.paths().with(path, |_| ()).is_ok());
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use alloc::rc::Rc;
    use core::cell::RefCell;

    struct SpawnBeforeRender(bool);

    impl<B, F> Plugin<B, F> for SpawnBeforeRender
    where
        B: Surface,
        F: RendererFactory<B>,
    {
        fn build(&mut self, _app: &mut App<B, F>) {}

        fn pre_render(&mut self, world: &mut World) {
            if !self.0 {
                world.spawn_empty();
                self.0 = true;
            }
        }
    }

    #[test]
    fn render_prepares_dirty_storage_after_plugin_spawns() {
        let mut app = App::headless(32, 32);
        let initial = app.world.live_entity_count();
        app.add_plugin(SpawnBeforeRender(false));
        let viewport = app.backend.viewport();
        app.prepare_render(viewport);

        let slots = app.world.allocated_entity_slots() as usize;
        let prepared = app
            .world
            .storage::<crate::ui::dirty::Dirty>()
            .unwrap()
            .reserved_entity_capacity();
        assert_eq!(app.world.live_entity_count(), initial + 1);
        assert!(prepared.0 >= slots);
        assert!(prepared.1 >= app.world.live_entity_count());
    }

    #[test]
    fn dirty_dense_reservation_excludes_released_entity_slots() {
        let mut app = App::headless(32, 32);
        let entities: alloc::vec::Vec<_> = (0..64).map(|_| app.world.spawn_empty()).collect();
        for entity in entities {
            assert!(app.world.despawn(entity));
        }
        app.prepare_dirty_storage();

        let prepared = app
            .world
            .storage::<crate::ui::dirty::Dirty>()
            .unwrap()
            .reserved_entity_capacity();
        assert!(prepared.0 >= app.world.allocated_entity_slots() as usize);
        assert!(prepared.1 < prepared.0);
    }

    #[derive(Default, Clone)]
    struct Trace {
        on_start: u32,
        on_suspend: u32,
        on_resume: u32,
        on_memory_warning: u32,
        user_inputs: u32,
        lifecycle_requests: u32,
        on_quit: u32,
    }

    struct TracePlugin {
        trace: Rc<RefCell<Trace>>,
    }

    impl<B, F> Plugin<B, F> for TracePlugin
    where
        B: Surface,
        F: RendererFactory<B>,
    {
        fn build(&mut self, _app: &mut App<B, F>) {}
        fn on_start(&mut self, _world: &mut World) {
            self.trace.borrow_mut().on_start += 1;
        }
        fn on_suspend(&mut self, _world: &mut World) {
            self.trace.borrow_mut().on_suspend += 1;
        }
        fn on_resume(&mut self, _world: &mut World) {
            self.trace.borrow_mut().on_resume += 1;
        }
        fn on_memory_warning(&mut self, _world: &mut World) {
            self.trace.borrow_mut().on_memory_warning += 1;
        }
        fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
            use crate::app::lifecycle::SuspendRequest;
            match event {
                InputEvent::AppSuspend => {
                    self.trace.borrow_mut().lifecycle_requests += 1;
                    world.insert_resource(SuspendRequest::Suspend);
                }
                InputEvent::AppResume => {
                    self.trace.borrow_mut().lifecycle_requests += 1;
                    world.insert_resource(SuspendRequest::Resume);
                }
                _ => self.trace.borrow_mut().user_inputs += 1,
            }
            false
        }
        fn on_quit(&mut self, _world: &mut World) {
            self.trace.borrow_mut().on_quit += 1;
        }
    }

    fn fresh() -> (
        App<crate::surface::framebuf::FramebufSurface<HeadlessFlush>>,
        Rc<RefCell<Trace>>,
    ) {
        let mut app = App::headless(32, 32);
        let trace = Rc::new(RefCell::new(Trace::default()));
        app.add_plugin(TracePlugin {
            trace: trace.clone(),
        });
        (app, trace)
    }

    #[test]
    fn suspend_resume_toggle_fires_hooks_once() {
        let (mut app, trace) = fresh();
        app.suspend();
        app.suspend();
        app.resume();
        app.resume();
        let t = trace.borrow();
        assert_eq!(t.on_suspend, 1, "duplicate suspend must not re-fire");
        assert_eq!(t.on_resume, 1, "duplicate resume must not re-fire");
    }

    #[test]
    fn is_suspended_reflects_state() {
        let (mut app, _) = fresh();
        assert!(!app.is_suspended());
        app.suspend();
        assert!(app.is_suspended());
        app.resume();
        assert!(!app.is_suspended());
    }

    #[test]
    fn tick_short_circuits_while_suspended() {
        let (mut app, _) = fresh();
        app.suspend();
        let quit = app.tick();
        assert!(!quit, "suspended tick returns without firing on_quit");
        assert!(
            app.is_suspended(),
            "tick must not flip the suspend state without a request"
        );
    }

    #[test]
    fn simulation_blocks_user_input_but_preserves_suspend_resume_and_quit() {
        use super::queued_surface::QueuedSurface;
        use crate::input::event::sim::{SimAction, SimTimeline};

        let mut app = App::new(QueuedSurface::new(32, 32, crate::types::Fixed::ONE));
        let trace = Rc::new(RefCell::new(Trace::default()));
        app.add_plugin(TracePlugin {
            trace: trace.clone(),
        });
        app.world
            .insert_resource(SimTimeline::new(alloc::vec![SimAction::wait(1000)]));
        let input = InputEvent::Key {
            code: 1,
            pressed: true,
        };
        app.backend
            .events
            .extend([input.clone(), InputEvent::AppSuspend]);
        assert!(!app.tick());
        assert!(app.is_suspended());
        assert_eq!(trace.borrow().user_inputs, 0);
        assert_eq!(trace.borrow().on_suspend, 1);

        app.backend.events.push_back(InputEvent::AppResume);
        assert!(!app.tick());
        assert!(!app.is_suspended());
        assert_eq!(trace.borrow().on_resume, 1);
        assert_eq!(trace.borrow().lifecycle_requests, 2);

        app.backend.events.extend([input, InputEvent::Quit]);
        assert!(app.tick());
        assert_eq!(trace.borrow().on_quit, 1);
        assert_eq!(trace.borrow().user_inputs, 0);
        assert!(app.world.resource::<SimTimeline>().unwrap().is_running());
    }

    #[test]
    fn suspend_request_resource_drives_state() {
        let (mut app, trace) = fresh();
        app.world
            .insert_resource(crate::app::lifecycle::SuspendRequest::Suspend);
        app.tick();
        assert!(app.is_suspended(), "Suspend request drained into state");
        app.world
            .insert_resource(crate::app::lifecycle::SuspendRequest::Resume);
        app.tick();
        assert!(!app.is_suspended(), "Resume request drained into state");
        let t = trace.borrow();
        assert_eq!(t.on_suspend, 1);
        assert_eq!(t.on_resume, 1);
    }

    #[test]
    fn memory_warning_preserves_state_and_notifies_plugins() {
        let (mut app, trace) = fresh();
        let root = app.spawn_root().id();
        app.set_root(root);
        let path = app
            .paths()
            .insert(crate::render::path::Path::new())
            .unwrap();

        app.handle_memory_warning();

        assert_eq!(trace.borrow().on_memory_warning, 1);
        assert!(app.paths().with(path, |_| ()).is_ok());
        assert_eq!(app.root, Some(root));
    }
}
