mod frame;
pub mod lifecycle;
mod model;
pub mod plugin;
pub mod plugins;
mod runner;
mod timing;

#[cfg(test)]
use crate::ecs::{FrameOperation, FramePhase, TimingOutcome};
use frame::clone_texture_owned;
pub use model::ModelBuilder;
pub use runner::Runner;

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
#[cfg(test)]
use crate::render::renderer::Renderer;
use crate::surface::{FramebufferAccess, InputEvent, Surface};
use crate::text::TextGeometry;
use crate::types::PhysicalRect;
#[cfg(test)]
use crate::types::Rect;
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
    let (pw, ph) = transform.physical_size();
    let surface = world
        .resource::<Theme>()
        .expect("App always owns an active Theme")
        .resolve(crate::ui::theme::ColorToken::Surface);
    renderer.clear(&PhysicalRect::from_size(pw, ph), &surface);
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
    dirty_plan: DirtyRegions,
    timing: timing::OperationTiming,
    needs_full_redraw: bool,
    resource_wait_revision: Option<(u64, u64, u64)>,
    painted_root_hidden: bool,
    suspended: bool,
    started: bool,
    host_focus: Option<bool>,
    host_visibility: Option<bool>,
    quitting: bool,
    text_layout_started: bool,
    dirty_reserved_slots: u32,
    dirty_reserved_live_entities: usize,
    text_layout_reservation: TextLayoutReservation,
    text_layout_preparation_error: Option<crate::text::TextLayoutError>,
    text_raster_scratch_extent: Option<(u16, u16)>,
    text_raster_scratch_preparation_error: Option<crate::render::RenderError>,
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

    /// Prepare an independent Model. Call `finish()` to register it and obtain its shared handle; dropping the builder performs no registration or IO.
    pub fn add_model<M: crate::core::model::Model>(&mut self, value: M) -> ModelBuilder<'_, M> {
        ModelBuilder::new(&mut self.world, value)
    }

    /// Access synchronous persistence operations. Install `PersistencePlugin` before registering persistent Models.
    #[cfg(feature = "persistence")]
    pub fn persistence(&mut self) -> crate::core::persistence::Persistence<'_> {
        crate::core::persistence::Persistence::new(&mut self.world)
    }

    pub(crate) fn text_input_unavailable(&self) -> u8 {
        u8::from(self.host_focus == Some(false))
            | (u8::from(self.host_visibility == Some(false)) << 1)
            | (u8::from(self.suspended) << 2)
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
        world.insert_resource(alloc::rc::Rc::new(crate::core::reactive::PaintWake::new()));
        crate::input::event::hit_test::prepare_geometry_tracking(&mut world);
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
            dirty_plan: DirtyRegions::with_rect_capacity(render_system::DIRTY_PLAN_RECT_CAPACITY),
            timing: timing::OperationTiming::default(),
            needs_full_redraw: true,
            resource_wait_revision: None,
            painted_root_hidden: false,
            suspended: false,
            started: false,
            host_focus: None,
            host_visibility: None,
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
            .invalidate_query_dependency::<crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle>();
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
            .invalidate_query_dependency::<crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle>();
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
            .filter(|(_, text)| text.capacity().is_some())
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
            crate::input::event::hit_test::set_preparation_blocked(&mut self.world, false);
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
                drop(storage);
                crate::input::event::hit_test::set_preparation_blocked(&mut self.world, true);
                return Err(error);
            }
            cache.limits()
        };
        let cache = match crate::text::layout::TextLayoutCache::try_new_bounded(limits, capacity) {
            Ok(cache) => cache,
            Err(error) => {
                self.text_layout_preparation_error = Some(error);
                crate::input::event::hit_test::set_preparation_blocked(&mut self.world, true);
                return Err(error);
            }
        };
        self.world
            .resource::<Outputs<TextGeometry>>()
            .expect("the app owns a text-layout cache")
            .replace_text_layouts(cache);
        self.text_layout_reservation = TextLayoutReservation::Prepared;
        self.text_layout_preparation_error = None;
        crate::input::event::hit_test::set_preparation_blocked(&mut self.world, false);
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

    /// Consume an independent completed output, retaining its pixel storage, geometry and original output-budget charge.
    ///
    /// Synchronizes lifecycle and output observations before checking availability. Returns `None` for missing or unavailable output, active consumers/history, a drawing batch or conflicting storage access. Backend validation failures propagate before storage is detached. This operation does not render. Call [`Self::render`] first to include newer application changes in the capture.
    ///
    /// The source's `OffscreenRender` remains application-controlled and later rendering can recreate its output. Native access follows its creating backend's lifetime and thread rules. The returned texture can be shared through a cache handle.
    pub fn take_widget_output(
        &mut self,
        entity: crate::ecs::Entity,
    ) -> Result<Option<crate::ui::WidgetTextureOwned>, crate::render::RenderError> {
        ViewRegistry::reconcile_observations(&mut self.world);
        if !self.world.is_alive(entity) {
            return Ok(None);
        }
        self.world
            .resource::<Outputs<WidgetTexture>>()
            .map_or(Ok(None), |outputs| outputs.take_texture(entity))
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
            return clone_texture_owned(output.texture()).map(Some);
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
            .map(|output| clone_texture_owned(output.texture()))
            .transpose();

        if !already_requested {
            self.world.remove::<OffscreenRender>(entity);
            crate::ui::offscreen::reconcile_texture_outputs(&self.world);
        }

        render_result.and(result)
    }

    /// Register one widget kind (built-in or user-defined).
    ///
    /// Contributed systems are installed in the App scheduler. Register a kind once and include its model-bound systems in the same list; repeated registration adds another descriptor and another set of systems.
    pub fn with_widget<S>(&mut self, view: View<S>) -> &mut Self
    where
        S: IntoIterator,
        S::Item: Into<System>,
    {
        let view = view.install(|system| {
            self.add_system(system);
        });
        if let Some(reg) = self.world.resource_mut::<ViewRegistry>() {
            reg.insert(view);
        }
        self
    }

    /// Register widget kinds with the same system-source type; also accepts a `ViewRegistry`.
    ///
    /// Chain [`Self::with_widget`] calls when system arrays have different lengths or source types.
    pub fn with_widgets<S>(&mut self, views: impl IntoIterator<Item = View<S>>) -> &mut Self
    where
        S: IntoIterator,
        S::Item: Into<System>,
    {
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
            crate::ui::text_input::host_input(&self.world, &InputEvent::AppSuspend);
            self.suspended = true;
            let now_ms = (self.clock_ns() / 1_000_000) as u32;
            crate::input::event::cancel_active_input(
                &mut self.world,
                now_ms,
                crate::input::event::press::PressCancelReason::Suspended,
            );
            self.dispatch_gestures();
            for p in &mut self.plugins {
                p.on_suspend(&mut self.world);
            }
            self.notify_app(crate::input::event::lifecycle::Suspended);
        }
    }

    pub fn resume(&mut self) {
        if self.suspended {
            crate::ui::text_input::host_input(&self.world, &InputEvent::AppResume);
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
        if let Some(relations) = self
            .world
            .resource_mut::<crate::ui::outputs::relations::OutputRelations>()
        {
            relations.trim_memory();
        }
        if let Some(snapshot) = self
            .world
            .resource_mut::<crate::ui::render_system::LayoutSnapshot>()
        {
            snapshot.trim_memory();
        }
        if let Some(outputs) = self.world.resource::<Outputs<WidgetTexture>>() {
            outputs.trim_memory();
        }
        if let Some(outputs) = self.world.resource::<Outputs<TextGeometry>>() {
            outputs.trim_text_memory();
            self.world
                .invalidate_query_dependency::<crate::text::TextLayoutHandle>();
            self.world.invalidate_query_dependency::<
                crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle,
            >();
        }
        self.factory.trim_memory();
        for plugin in &mut self.plugins {
            plugin.on_memory_warning(&mut self.world);
        }
        self.needs_full_redraw = true;
        if let Some(root) = self.root {
            self.world.invalidate(root);
        }
        self.notify_app(crate::input::event::lifecycle::MemoryWarning);
    }

    fn notify_app<E: crate::input::event::lifecycle::AppNotice>(&mut self, notice: E) {
        if let Err(error) = crate::ui::delivery::notify_app(&mut self.world, notice) {
            crate::warn!("App notification failed: {:?}", error);
        }
    }

    fn flush_event_notifications(&mut self) {
        if let Err(error) = crate::ui::delivery::flush(&mut self.world) {
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
        crate::input::event::dispatch_pending_gestures(&mut self.world, now_ms);
    }

    fn finish_quit(&mut self) {
        if !self.quitting {
            self.quitting = true;
            let now_ms = (self.clock_ns() / 1_000_000) as u32;
            crate::input::event::cancel_active_input(
                &mut self.world,
                now_ms,
                crate::input::event::press::PressCancelReason::Platform,
            );
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
        crate::ui::text_input::host_input(&self.world, event);
        if matches!(
            event,
            InputEvent::HostFocus { focused: false }
                | InputEvent::HostVisibility { visible: false }
        ) {
            let now_ms = (self.clock_ns() / 1_000_000) as u32;
            crate::input::event::cancel_active_input(
                &mut self.world,
                now_ms,
                crate::input::event::press::PressCancelReason::Platform,
            );
            self.dispatch_gestures();
        }
        let mut consumed = false;
        for plugin in &mut self.plugins {
            let accepted = plugin.on_event(&mut self.world, event);
            consumed |= accepted;
            if accepted && event.is_user_input() {
                break;
            }
        }
        match *event {
            InputEvent::HostFocus { focused } if self.host_focus != Some(focused) => {
                self.host_focus = Some(focused);
                self.notify_app(crate::input::event::lifecycle::HostFocusChanged { focused });
            }
            InputEvent::HostVisibility { visible } if self.host_visibility != Some(visible) => {
                self.host_visibility = Some(visible);
                self.notify_app(crate::input::event::lifecycle::HostVisibilityChanged { visible });
            }
            _ => {}
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
    /// ```
    /// use mirui::prelude::*;
    ///
    /// let mut app = App::headless(128, 128);
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
    /// ```
    /// use mirui::prelude::*;
    /// use mirui::ui::widgets::Text;
    ///
    /// #[compose]
    /// fn build_panel() -> Entity {
    ///     ui! {
    ///         View(width: 128, height: 64) {
    ///             Text("Ready")
    ///         }
    ///     }
    /// }
    ///
    /// let mut app = App::headless(128, 128);
    /// app.with_default_widgets();
    /// let root = app.spawn_root().id();
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
        crate::ui::lifecycle::attach_handlers_for(&mut self.app.world, self.entity);
        self.app.set_root(self.entity);
        self.entity
    }
}

#[cfg(all(target_arch = "wasm32", feature = "web-canvas"))]
mod web_runner;

#[cfg(test)]
#[path = "../../tests/unit/app/app_invariant_tests.rs"]
mod app_invariant_tests;
#[cfg(test)]
#[path = "../../tests/unit/app/compose_entry_tests.rs"]
mod compose_entry_tests;
#[cfg(test)]
#[path = "../../tests/unit/app/fractional_scale_input_check.rs"]
mod fractional_scale_input_check;
#[cfg(test)]
#[path = "../../tests/unit/app/frame_background_tests.rs"]
mod frame_background_tests;
#[cfg(test)]
#[path = "../../tests/unit/app/frame_submission_tests.rs"]
mod frame_submission_tests;
#[cfg(test)]
#[path = "../../tests/unit/app/host_state_tests.rs"]
mod host_state_tests;
#[cfg(test)]
#[path = "../../tests/unit/app/lifecycle_tests.rs"]
mod lifecycle_tests;
#[cfg(test)]
#[path = "../../tests/unit/app/multi_tap_initialization_tests.rs"]
mod multi_tap_initialization_tests;
#[cfg(test)]
#[path = "../../tests/unit/app/queued_surface.rs"]
mod queued_surface;
#[cfg(test)]
#[path = "../../tests/unit/app/snapshot_tests.rs"]
mod snapshot_tests;
#[cfg(test)]
#[path = "../../tests/unit/app/swap_tests.rs"]
mod swap_tests;
