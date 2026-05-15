use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::draw::SwRenderer;
use crate::draw::canvas::Canvas;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, System, SystemScheduler, World};
use crate::event::bubble_dispatch;
use crate::event::focus::{FocusState, focus_on_tap, key_dispatch};
use crate::event::gesture::GestureSystem;
use crate::event::hit_test::hit_test;
use crate::event::scroll::{ScrollDragState, ScrollSpring, scroll_inertia_system, scroll_system};
use crate::plugin::Plugin;
use crate::surface::{FramebufferAccess, InputEvent, Surface};
use crate::types::{Rect, Viewport};
use crate::widget::render_system;

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
}

impl<B: FramebufferAccess> App<B, SwRendererFactory> {
    pub fn new(backend: B) -> Self {
        Self::with_factory(backend, SwRendererFactory)
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
        Self {
            world,
            backend,
            factory,
            root: None,
            systems: SystemScheduler::new(),
            plugins: Vec::new(),
            #[cfg(feature = "perf")]
            perf: None,
        }
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
        crate::event::widget_input::attach_widget_input_handlers(&mut self.world, root);
        crate::event::sim::set_sim_root(&mut self.world, root);
    }

    /// Render one frame
    pub fn render(&mut self) {
        let Some(root) = self.root else { return };
        let info = self.backend.display_info();
        let transform = info.viewport();

        for p in &mut self.plugins {
            p.pre_render(&mut self.world);
        }

        let start_ns = self.clock_ns();

        render_system::update_layout(&mut self.world, root, &transform);

        {
            let mut renderer = self.factory.make(&mut self.backend, &transform);
            render_system::render(&self.world, root, &transform, &mut renderer);
        }
        let (pw, ph) = self.backend.physical_size();
        self.backend.flush(&Rect::new(0, 0, pw as u16, ph as u16));

        // Seed PrevRect so the next render_dirty frame's dirty union
        // covers pixels this full render actually wrote — otherwise
        // any widget that shrinks or moves between the full render
        // and the first dirty render leaves residue.
        render_system::seed_prev_rects(&mut self.world, root, &transform);

        let elapsed = self.clock_ns().saturating_sub(start_ns);
        for p in &mut self.plugins {
            p.post_render(&mut self.world, elapsed);
        }
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
            self.systems.run_all(&mut self.world);

            if let Some(gs) = self.world.resource_mut::<GestureSystem>() {
                gs.events.clear();
            }

            let mut logical: Option<(u16, u16)> = None;
            let mut quit = false;
            loop {
                match self.poll_event() {
                    Some(InputEvent::Quit) => {
                        quit = true;
                        break;
                    }
                    Some(event) => {
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
                            scroll_system(&mut self.world, root, &event, lw, lh);

                            let hit = match &event {
                                InputEvent::PointerDown { x, y, .. } => {
                                    hit_test(&self.world, root, *x, *y, lw, lh)
                                }
                                _ => None,
                            };
                            let now_ms = (self.clock_ns() / 1_000_000) as u32;
                            let scroll_claimed = self
                                .world
                                .resource::<ScrollDragState>()
                                .is_some_and(|s| s.active && s.resolved);
                            if let Some(gs) = self.world.resource_mut::<GestureSystem>() {
                                gs.recognizer.scroll_claimed = scroll_claimed;
                                gs.recognizer.update(&event, now_ms, hit, &mut gs.events);
                            }

                            key_dispatch(&mut self.world, &event);
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

            scroll_inertia_system(&mut self.world);
            if transient {
                self.render();
            } else {
                self.render_dirty();
            }
        }
    }

    /// Render only dirty regions. Falls back to full render if no dirty tracking.
    pub fn render_dirty(&mut self) {
        let Some(root) = self.root else { return };
        let info = self.backend.display_info();
        let transform = info.viewport();

        for p in &mut self.plugins {
            p.pre_render(&mut self.world);
        }

        let start_ns = self.clock_ns();

        let dirty = render_system::collect_dirty_region(&mut self.world, root, &transform);

        if let Some(area) = dirty {
            {
                let mut renderer = self.factory.make(&mut self.backend, &transform);
                render_system::render_region(&self.world, root, &transform, &area, &mut renderer);
            }
            let phys_area = transform.rect_to_physical(area);
            self.backend.flush(&phys_area);
        }

        let elapsed = self.clock_ns().saturating_sub(start_ns);
        for p in &mut self.plugins {
            p.post_render(&mut self.world, elapsed);
        }
    }
}
