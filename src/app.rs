use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::draw::SwRenderer;
use crate::draw::canvas::Canvas;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, System, SystemScheduler, World};
use crate::event::bubble_dispatch;
use crate::event::focus::{FocusState, focus_on_tap};
use crate::event::gesture::GestureSystem;
use crate::event::scroll::{ScrollDragState, ScrollSpring};
use crate::plugin::Plugin;
use crate::surface::{FramebufferAccess, InputEvent, Surface};
use crate::types::{Rect, Viewport};
use crate::widget::Theme;
use crate::widget::offscreen::OffscreenBufferPool;
use crate::widget::render_system;
use crate::widget::view::{View, ViewRegistry};

/// Builds a Renderer each frame, given mutable access to the backend and
/// the current logical/physical coord transform.
///
/// The factory is parameterised over the backend type so each GPU backend
/// can bind to its own concrete `B` and reach into backend-specific
/// resources (SDL canvas, wgpu device, VG-Lite context). CPU-raster
/// factories (like [`SwRendererFactory`]) use the [`FramebufferAccess`]
/// sub-trait bound to obtain a `Texture<'_>` from any compatible backend.
pub trait RendererFactory<B: Surface> {
    type Renderer<'a>: Renderer + Canvas
    where
        Self: 'a,
        B: 'a;
    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> Self::Renderer<'a>;
}

/// Default factory that produces plain `SwRenderer<'a>` on top of any
/// backend exposing a CPU framebuffer.
pub struct SwRendererFactory;

impl<B: FramebufferAccess> RendererFactory<B> for SwRendererFactory {
    type Renderer<'a>
        = SwRenderer<'a>
    where
        Self: 'a,
        B: 'a;
    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> SwRenderer<'a> {
        let tex = backend.framebuffer();
        let mut r = SwRenderer::new(tex);
        r.viewport = *transform;
        r
    }
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
    pub perf: Option<crate::draw::PerfCtx>,
    last_layout_ns: u64,
    last_render_ns: u64,
    last_flush_ns: u64,
    last_seed_prev_ns: u64,
    pending_frame: Option<PendingFrame>,
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
        world.insert_resource(OffscreenBufferPool::default());
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
        }
    }

    /// Replace the active [`Theme`]. Defaults to [`Theme::dark`].
    pub fn with_theme(&mut self, theme: Theme) -> &mut Self {
        self.world.insert_resource(theme);
        self
    }

    /// Set the offscreen render buffer pool's entry capacity. Pick a
    /// value large enough to hold every concurrently-cached widget
    /// buffer the app needs; lower values trade cache hit rate for
    /// memory. Pass `0` to disable the pool entirely —
    /// `OffscreenRender` entities then fall through to inline
    /// rendering on every frame.
    ///
    /// Replaces the existing pool, so call before any frame renders
    /// an [`crate::widget::OffscreenRender`] entity.
    pub fn with_offscreen_pool_capacity(&mut self, capacity: usize) -> &mut Self {
        self.world
            .insert_resource(OffscreenBufferPool::new(capacity));
        self
    }

    /// Runtime counterpart to `with_theme`: also forces a full-tree repaint.
    pub fn set_theme(&mut self, theme: Theme) {
        crate::widget::theme::set_theme(&mut self.world, theme);
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
        self.add_system(crate::timer::timer_system::system());
        self.add_system(crate::event::scroll::system::scroll_inertia_system::system());
        self.add_system(crate::widget::state::hover_system::system());
        self.add_system(crate::widget::state::press_system::system());
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

    fn clock_ns(&self) -> u64 {
        self.world
            .resource::<crate::ecs::MonoClock>()
            .map(|fc| (fc.clock)())
            .unwrap_or(0)
    }

    pub fn set_root(&mut self, root: Entity) {
        self.root = Some(root);
        self.world.insert_resource(crate::widget::WidgetRoot(root));
        crate::event::widget_input::attach_widget_input_handlers(&mut self.world, root);
        crate::event::sim::set_sim_root(&mut self.world, root);
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
            self.backend.flush(&Rect::new(0, 0, pw as u16, ph as u16));
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

        self.finalize_frame_stats();
        for p in &mut self.plugins {
            p.post_render(&mut self.world, render_ns);
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
        let snapshots = crate::cache::InspectCaches::inspect_caches(&self.backend)
            .map(|(_, c)| crate::cache::CacheStatsSnapshot::capture(c))
            .collect();
        self.world
            .insert_resource(crate::cache::CacheRegistry::from_snapshots(snapshots));
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
        let transient =
            self.backend.persistence() == crate::surface::BackbufferPersistence::Transient;
        self.render();
        loop {
            if let Some(gs) = self.world.resource_mut::<GestureSystem>() {
                gs.events.clear();
            }

            let frame_start = self.clock_ns();

            let mut logical: Option<(u16, u16)> = None;
            let mut quit = false;
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
                            .resource::<crate::event::sim::SimTimeline>()
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
                            crate::event::dispatch_input(
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
            for gesture in &pending {
                focus_on_tap(&mut self.world, gesture);
                bubble_dispatch(&mut self.world, gesture);
            }

            if quit {
                for p in &mut self.plugins {
                    p.on_quit(&mut self.world);
                }
                return;
            }

            let input_end = self.clock_ns();

            // Systems run after event/gesture dispatch so that anything
            // observing input-driven state (ScrollOffset, focus, ...)
            // sees the post-event values within the same frame.
            self.systems.run_all(&mut self.world);
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
        }
    }

    fn snapshot_system_perf(&mut self) {
        let want_reset = self
            .world
            .resource::<crate::plugins::perf_report::PerfResetFlag>()
            .map(|f| f.0)
            .unwrap_or(false);
        if want_reset {
            self.systems.reset_perf();
            self.world
                .insert_resource(crate::plugins::perf_report::PerfResetFlag(false));
        }

        let mut entries = alloc::vec::Vec::with_capacity(8);
        for s in self.systems.iter() {
            let avg = if s.call_count == 0 {
                0
            } else {
                (s.total_us / s.call_count as u64) as u32
            };
            entries.push(crate::plugins::perf_report::SystemStat {
                name: s.name,
                priority: s.priority,
                last_us: s.last_us,
                avg_us: avg,
                call_count: s.call_count,
            });
        }
        self.world
            .insert_resource(crate::plugins::perf_report::SystemPerfSnapshot { entries });
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

        let dirty = crate::trace_span!("frame.collect_dirty", {
            render_system::collect_dirty_region(&mut self.world, root, &transform)
        });
        let layout_end = self.clock_ns();
        // collect_dirty_region runs build_layout_tree + compute_layout
        // internally, so it counts as the dirty path's layout cost.
        self.last_layout_ns = layout_end.saturating_sub(layout_start);

        if let Some(area) = dirty {
            {
                crate::trace_span!("frame.render_region");
                let mut renderer = self.factory.make(&mut self.backend, &transform);
                render_system::render_region(&self.world, root, &transform, &area, &mut renderer);
            }
        }
        let render_end = self.clock_ns();
        self.last_render_ns = render_end.saturating_sub(layout_end);
        let render_ns = render_end.saturating_sub(layout_start);

        if let Some(area) = dirty {
            let phys_area = transform.rect_to_physical(area);
            {
                crate::trace_span!("frame.flush");
                self.backend.flush(&phys_area);
            }
        }
        self.last_flush_ns = self.clock_ns().saturating_sub(render_end);
        self.last_seed_prev_ns = 0;

        self.finalize_frame_stats();
        for p in &mut self.plugins {
            p.post_render(&mut self.world, render_ns);
        }
    }
}
