pub mod lifecycle;
pub mod plugin;
pub mod plugins;

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::app::plugin::Plugin;
use crate::ecs::{Entity, System, SystemScheduler, World};
use crate::input::event::bubble_dispatch_at;
use crate::input::event::focus::{FocusState, focus_on_tap};
use crate::input::event::gesture::GestureSystem;
use crate::input::event::scroll::{ScrollDragState, ScrollSpring};
use crate::render::renderer::Renderer;
use crate::surface::{FramebufferAccess, InputEvent, Surface};
use crate::types::Rect;
use crate::ui::Theme;
use crate::ui::offscreen::OffscreenBufferPool;
use crate::ui::render_system;
use crate::ui::view::{View, ViewRegistry};

pub use crate::render::factory::{RendererFactory, SwRendererFactory};

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
    pending_frame: Option<PendingFrame>,
    needs_full_first_frame: bool,
    suspended: bool,
    started: bool,
}

struct PendingFrame {
    frame_start: u64,
    input_end: u64,
    systems_end: u64,
}

impl<B: FramebufferAccess> App<B, SwRendererFactory> {
    pub fn new(backend: B) -> Self {
        Self::with_factory(backend, SwRendererFactory)
    }
}

type HeadlessFlush = fn(&[u8], &crate::types::Rect);

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

impl<B: Surface, F: RendererFactory<B>> App<B, F> {
    pub fn with_factory(backend: B, factory: F) -> Self {
        let mut world = World::new();
        world.insert_resource(ScrollDragState::default());
        world.insert_resource(ScrollSpring::default());
        world.insert_resource(GestureSystem::default());
        world.insert_resource(FocusState::default());
        let info = backend.display_info();
        world.insert_resource(info);
        world.insert_resource(ViewRegistry::default());
        world.insert_resource(Theme::default());
        world.insert_resource(crate::render::font::default_font_manager());
        world.insert_resource(crate::core::i18n::I18n::default());
        world.insert_resource(OffscreenBufferPool::default());
        world.insert_resource(crate::ui::IdMap::new());
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
            pending_frame: None,
            needs_full_first_frame: true,
            suspended: false,
            started: false,
        }
    }

    /// Replace the active [`Theme`]. Defaults to [`Theme::dark`].
    pub fn with_theme(&mut self, theme: Theme) -> &mut Self {
        self.world.insert_resource(theme);
        self
    }

    /// The default manager falls back to the bundled 8x8 bitmap for any
    /// unregistered token; passing a custom manager replaces it.
    pub fn with_fonts(&mut self, fonts: crate::render::font::FontManager) -> &mut Self {
        self.world.insert_resource(fonts);
        self
    }

    pub fn with_i18n(&mut self, i18n: crate::core::i18n::I18n) -> &mut Self {
        self.world.insert_resource(i18n);
        self
    }

    /// Set the offscreen render buffer pool's byte budget. Pick a
    /// value large enough to hold every concurrently-cached widget
    /// buffer the app needs (each is `width × height ×
    /// bytes_per_pixel`, so a 128×64 RGB565 buffer is 16 KiB) plus
    /// some eviction headroom; lower values trade cache hit rate for
    /// resident memory. Pass `0` to disable the pool entirely —
    /// `OffscreenRender` entities then fall through to inline
    /// rendering on every frame.
    ///
    /// Replaces the existing pool, so call before any frame renders
    /// an [`crate::ui::OffscreenRender`] entity.
    pub fn with_offscreen_pool_budget(&mut self, budget_bytes: usize) -> &mut Self {
        self.world
            .insert_resource(OffscreenBufferPool::with_budget(budget_bytes));
        self
    }

    /// Runtime counterpart to `with_theme`: also forces a full-tree repaint.
    pub fn set_theme(&mut self, theme: Theme) {
        crate::ui::theme::set_theme(&mut self.world, theme);
    }

    /// Owned snapshot of the entity's rendered output. One-off cost:
    /// triggers a full-tree render if the entity isn't already
    /// cached. For sustained access use [`WidgetTextureRef`].
    ///
    /// `None` when the renderer doesn't expose offscreen rendering
    /// (GPU backends) or when the pool can't fit the entity's buffer.
    pub fn snapshot_widget(
        &mut self,
        entity: crate::ecs::Entity,
    ) -> Option<crate::render::texture::Texture<'static>> {
        use crate::ui::OffscreenRender;
        use crate::ui::offscreen::{OffscreenAutoAdded, WidgetTextureAccess};

        if let Some(snap) = self.world.texture_of(entity) {
            return Some(clone_texture_owned(&snap.borrow()));
        }

        let already_explicit = self.world.get::<OffscreenRender>(entity).is_some();
        if !already_explicit {
            self.world.insert(entity, OffscreenRender::default());
            self.world.insert(entity, OffscreenAutoAdded);
        }

        self.render();

        let result = self
            .world
            .texture_of(entity)
            .map(|snap| clone_texture_owned(&snap.borrow()));

        if !already_explicit {
            self.world.remove::<OffscreenRender>(entity);
            self.world.remove::<OffscreenAutoAdded>(entity);
        }

        result
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
        self.add_system(crate::ui::offscreen::maintain_widget_texture_refs::system());
        self
    }

    pub fn add_system(&mut self, system: System) -> &mut Self {
        self.systems.add(system);
        self
    }

    /// Register a plugin. Runs `plugin.build(self)` immediately, then stores
    /// the instance so later lifecycle hooks (pre/post_render, on_event,
    /// on_quit) can be dispatched to it.
    pub fn add_plugin<P: Plugin<B, F> + 'static>(&mut self, mut plugin: P) -> &mut Self {
        plugin.build(self);
        self.plugins.push(Box::new(plugin));
        self
    }

    pub fn suspend(&mut self) {
        if !self.suspended {
            self.suspended = true;
            for p in &mut self.plugins {
                p.on_suspend(&mut self.world);
            }
        }
    }

    pub fn resume(&mut self) {
        if self.suspended {
            self.suspended = false;
            for p in &mut self.plugins {
                p.on_resume(&mut self.world);
            }
        }
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    fn clock_ns(&self) -> u64 {
        self.world
            .resource::<crate::ecs::MonoClock>()
            .map(|fc| (fc.clock)())
            .unwrap_or(0)
    }

    pub fn set_root(&mut self, root: Entity) {
        self.root = Some(root);
        self.world.insert_resource(crate::ui::WidgetRoot(root));
        crate::input::event::widget_input::attach_widget_input_handlers(&mut self.world, root);
        crate::input::event::sim::set_sim_root(&mut self.world, root);
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
    /// and [`FlexDirection::Column`][crate::ui::layout::FlexDirection::Column].
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
        RootBuilder { app: self, entity }
    }

    /// Render one frame
    #[mirui::trace_fn("frame.full")]
    pub fn render(&mut self) {
        let Some(root) = self.root else { return };
        let info = self.backend.display_info();
        let transform = info.viewport();

        for p in &mut self.plugins {
            p.pre_render(&mut self.world);
        }

        let layout_start = self.clock_ns();

        {
            crate::trace_span!("frame.layout");
            render_system::update_layout(&mut self.world, root, &transform);
        }
        let layout_end = self.clock_ns();
        self.last_layout_ns = layout_end.saturating_sub(layout_start);

        {
            crate::trace_span!("frame.render");
            let mut renderer = self.factory.make(&mut self.backend, &transform);
            render_system::render(&self.world, root, &transform, &mut renderer);
        }
        let render_end = self.clock_ns();
        self.last_render_ns = render_end.saturating_sub(layout_end);
        let render_ns = render_end.saturating_sub(layout_start);

        let (pw, ph) = self.backend.physical_size();
        {
            crate::trace_span!("frame.flush");
            self.backend.begin_flush();
            self.backend.flush(&Rect::new(0, 0, pw as u16, ph as u16));
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
    /// clearing dirty flags in the process. Multiply by `display_info().scale`
    /// (or use `Viewport::rect_to_physical`) for physical coordinates.
    pub fn dirty_region(&mut self) -> Option<Rect> {
        let root = self.root?;
        let info = self.backend.display_info();
        let transform = info.viewport();
        render_system::collect_dirty_region(&mut self.world, root, &transform)
    }

    /// Poll one event
    pub fn poll_event(&mut self) -> Option<InputEvent> {
        self.backend.poll_event()
    }

    /// Systems + render + poll until quit. Persistent backends take the
    /// `render_dirty` fast path; Transient backends render every frame.
    pub fn run(&mut self) {
        if !self.started {
            self.started = true;
            for p in &mut self.plugins {
                p.on_start(&mut self.world);
            }
        }
        self.render();
        loop {
            if self.tick() {
                return;
            }
        }
    }

    /// One frame. Returns `true` on `Quit` (after `on_quit` hooks).
    pub fn tick(&mut self) -> bool {
        if self.suspended {
            // Fan-out plugin on_event while suspended so AutoSuspendOnFocus-style
            // plugins can write SuspendRequest::Resume in response to lifecycle events.
            // Widget dispatch is skipped — suspended means no UI interaction.
            loop {
                match self.poll_event() {
                    Some(InputEvent::Quit) => {
                        for p in &mut self.plugins {
                            p.on_quit(&mut self.world);
                        }
                        return true;
                    }
                    Some(event) => {
                        for p in &mut self.plugins {
                            if p.on_event(&mut self.world, &event) {
                                break;
                            }
                        }
                    }
                    None => break,
                }
            }
            if let Some(req) = self
                .world
                .remove_resource::<crate::app::lifecycle::SuspendRequest>()
            {
                match req {
                    crate::app::lifecycle::SuspendRequest::Resume => self.resume(),
                    crate::app::lifecycle::SuspendRequest::Suspend => {}
                }
            }
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
                        // Active sim timelines own PointerCursor; real
                        // input racing them corrupts the demo.
                        let sim_running = self
                            .world
                            .resource::<crate::input::event::sim::SimTimeline>()
                            .is_some_and(|t| t.is_running());
                        if sim_running {
                            continue;
                        }
                        let mut consumed = false;
                        for p in &mut self.plugins {
                            if p.on_event(&mut self.world, &event) {
                                consumed = true;
                                break;
                            }
                        }
                        if consumed {
                            continue;
                        }
                        if let Some(root) = self.root {
                            let (lw, lh) = *logical.get_or_insert_with(|| {
                                self.backend.display_info().viewport().logical_size()
                            });
                            let now_ms = (self.clock_ns() / 1_000_000) as u32;
                            crate::input::event::dispatch_input(
                                &mut self.world,
                                root,
                                &event,
                                now_ms,
                                lw,
                                lh,
                            );
                        }
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

            let pending: Vec<_> = self
                .world
                .resource_mut::<GestureSystem>()
                .map(|gs| gs.events.drain().collect())
                .unwrap_or_default();
            let now_ms = (self.clock_ns() / 1_000_000) as u32;
            for gesture in &pending {
                focus_on_tap(&mut self.world, gesture);
                bubble_dispatch_at(&mut self.world, gesture, now_ms);
            }

            if quit {
                for p in &mut self.plugins {
                    p.on_quit(&mut self.world);
                }
                return true;
            }
        }

        let input_end = self.clock_ns();

        {
            crate::trace_span!("frame.systems");
            self.systems.run_all(&mut self.world);
        }
        crate::core::reactive::flush_signal_dirty(&mut self.world);
        self.snapshot_system_perf();
        let systems_end = self.clock_ns();

        self.pending_frame = Some(PendingFrame {
            frame_start,
            input_end,
            systems_end,
        });
        if transient {
            self.render();
        } else {
            self.render_dirty();
        }
        self.backend.frame_end();

        if let Some(req) = self
            .world
            .remove_resource::<crate::app::lifecycle::SuspendRequest>()
        {
            match req {
                crate::app::lifecycle::SuspendRequest::Suspend => self.suspend(),
                crate::app::lifecycle::SuspendRequest::Resume => {}
            }
        }

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
                    priority: s.priority,
                    last_us: s.last_us,
                    avg_us: avg,
                    call_count: s.call_count,
                });
        }
    }

    /// Render only dirty regions. Falls back to full render if no dirty tracking.
    #[mirui::trace_fn("frame.dirty")]
    pub fn render_dirty(&mut self) {
        let Some(root) = self.root else { return };
        let info = self.backend.display_info();
        let transform = info.viewport();

        for p in &mut self.plugins {
            p.pre_render(&mut self.world);
        }

        let layout_start = self.clock_ns();

        let force_full = self.needs_full_first_frame && self.backend.buffer_count() > 1;
        let plan = if force_full {
            let (lw, lh) = transform.logical_size();
            crate::ui::dirty::DirtyRegions {
                rects: alloc::vec![Rect::new(0, 0, lw, lh)],
                shifts: alloc::vec::Vec::new(),
            }
        } else {
            crate::trace_span!("frame.collect_dirty", {
                render_system::collect_dirty_regions(&mut self.world, root, &transform)
            })
        };
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

            self.world
                .insert_resource(crate::ui::render_system::LastDirtyRegions(plan.clone()));
            for sop in &plan.shifts {
                renderer.scroll_target_region(&sop.area, sop.dx, sop.dy);
            }

            // Union into one bbox: a single tree walk is ~3x cheaper than
            // N walks even when the union over-paints the gaps.
            if let Some(union_rect) = plan.rects.iter().copied().reduce(|a, b| a.union(&b)) {
                if let Some(snapshot) = self
                    .world
                    .resource::<crate::ui::render_system::LayoutSnapshot>()
                {
                    render_system::render_region_cached(
                        &self.world,
                        snapshot,
                        &union_rect,
                        &mut renderer,
                    );
                } else {
                    render_system::render_region(
                        &self.world,
                        root,
                        &transform,
                        &union_rect,
                        &mut renderer,
                    );
                }
            }

            drop(renderer);

            let render_end = self.clock_ns();
            self.last_render_ns = render_end.saturating_sub(layout_end);
            let render_ns = render_end.saturating_sub(layout_start);
            {
                crate::trace_span!("frame.flush");
                self.backend.begin_flush();
                for rect in &plan.rects {
                    let phys = transform.rect_to_physical(*rect);
                    self.backend.flush(&phys);
                }
                for sop in &plan.shifts {
                    let phys = transform.rect_to_physical(sop.area);
                    self.backend.flush(&phys);
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
            return;
        }

        // Idle frame: clear LastDirtyRegions so consumers (cursor
        // feedback, perf-plan-probe) can distinguish "this frame
        // produced no shift" from "stale plan from N frames ago".
        self.world
            .insert_resource(crate::ui::render_system::LastDirtyRegions::default());

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

    /// Register the root via [`App::set_root`] and return its entity.
    pub fn id(self) -> Entity {
        self.app.set_root(self.entity);
        self.entity
    }
}

/// Owns an [`App`] and drives [`tick`][App::tick] from a non-blocking
/// loop. Native: [`run_blocking`][Runner::run_blocking]. Wasm:
/// [`start_animation_frame`][Runner::start_animation_frame].
pub struct Runner<B: Surface, F: RendererFactory<B> = SwRendererFactory> {
    app: App<B, F>,
}

impl<B: Surface, F: RendererFactory<B>> Runner<B, F> {
    /// `App::run` with `-> !`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_blocking(mut self) -> ! {
        self.app.render();
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
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use alloc::rc::Rc;
    use core::cell::RefCell;

    #[derive(Default, Clone)]
    struct Trace {
        on_start: u32,
        on_suspend: u32,
        on_resume: u32,
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
}
