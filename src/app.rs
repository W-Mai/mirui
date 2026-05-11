use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::backend::{Backend, FramebufferAccess, InputEvent};
use crate::components::button_system::button_system;
use crate::components::scroll_system::{ScrollDragState, scroll_inertia_system, scroll_system};
use crate::draw::SwDrawBackend;
use crate::draw::backend::DrawBackend;
use crate::draw::renderer::Renderer;
use crate::ecs::{DeltaTime, ElapsedTime, Entity, System, SystemScheduler, World};
use crate::event::dispatch::dispatch;
use crate::plugin::Plugin;
use crate::types::{Rect, Viewport};
use crate::widget::render_system;

/// Monotonic clock the App uses to measure per-frame render time. Plugins can
/// swap it (e.g. StdInstantClockPlugin on std, ESP systimer on embedded). The
/// default returns 0, which makes `post_render` hooks see 0 and skip their
/// timing logic.
pub type ClockFn = Box<dyn FnMut() -> u64>;

/// Builds a Renderer each frame, given mutable access to the backend and
/// the current logical/physical coord transform.
///
/// The factory is parameterised over the backend type so each GPU backend
/// can bind to its own concrete `B` and reach into backend-specific
/// resources (SDL canvas, wgpu device, VG-Lite context). CPU-raster
/// factories (like [`SwDrawBackendFactory`]) use the [`FramebufferAccess`]
/// sub-trait bound to obtain a `Texture<'_>` from any compatible backend.
pub trait RendererFactory<B: Backend> {
    type Renderer<'a>: Renderer + DrawBackend
    where
        Self: 'a,
        B: 'a;
    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> Self::Renderer<'a>;
}

/// Default factory that produces plain `SwDrawBackend<'a>` on top of any
/// backend exposing a CPU framebuffer.
pub struct SwDrawBackendFactory;

impl<B: FramebufferAccess> RendererFactory<B> for SwDrawBackendFactory {
    type Renderer<'a>
        = SwDrawBackend<'a>
    where
        Self: 'a,
        B: 'a;
    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> SwDrawBackend<'a> {
        let tex = backend.framebuffer();
        let mut r = SwDrawBackend::new(tex);
        r.viewport = *transform;
        r
    }
}

/// Main application entry point — ties World + Backend + Renderer factory together
pub struct App<B: Backend, F: RendererFactory<B> = SwDrawBackendFactory> {
    pub world: World,
    pub backend: B,
    pub factory: F,
    pub root: Option<Entity>,
    pub systems: SystemScheduler,
    pub clock: ClockFn,
    plugins: Vec<Box<dyn Plugin<B, F>>>,
    #[cfg(feature = "perf")]
    pub perf: Option<crate::draw::PerfCtx>,
}

impl<B: FramebufferAccess> App<B, SwDrawBackendFactory> {
    pub fn new(backend: B) -> Self {
        Self::with_factory(backend, SwDrawBackendFactory)
    }
}

impl<B: Backend, F: RendererFactory<B>> App<B, F> {
    pub fn with_factory(backend: B, factory: F) -> Self {
        let mut world = World::new();
        world.insert_resource(DeltaTime(0.0));
        world.insert_resource(ElapsedTime(0.0));
        world.insert_resource(ScrollDragState::default());
        let info = backend.display_info();
        world.insert_resource(info);
        Self {
            world,
            backend,
            factory,
            root: None,
            systems: SystemScheduler::new(),
            clock: Box::new(|| 0),
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

    pub fn set_root(&mut self, root: Entity) {
        self.root = Some(root);
    }

    /// Render one frame
    pub fn render(&mut self) {
        let Some(root) = self.root else { return };
        let info = self.backend.display_info();
        let transform = info.viewport();

        for p in &mut self.plugins {
            p.pre_render(&mut self.world);
        }

        let start_ns = (self.clock)();

        render_system::update_layout(&mut self.world, root, &transform);

        {
            let mut renderer = self.factory.make(&mut self.backend, &transform);
            render_system::render(&self.world, root, &transform, &mut renderer);
        }
        self.backend
            .flush(&Rect::new(0, 0, info.width, info.height));

        let elapsed = (self.clock)().saturating_sub(start_ns);
        for p in &mut self.plugins {
            p.post_render(&mut self.world, elapsed);
        }
    }

    /// Get the dirty region (physical pixels) after event processing, clearing dirty flags.
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
            self.backend.persistence() == crate::backend::BackbufferPersistence::Transient;
        self.render();
        loop {
            self.systems.run_all(&mut self.world);

            // Snapshot once per iteration so every event in this drain sees
            // the same logical-size, and we stop reconstructing transform per
            // event.
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
                            button_system(&mut self.world, root, &event, lw, lh);
                            scroll_system(&mut self.world, root, &event, lw, lh);
                            dispatch(&self.world, root, &event, lw, lh);
                        }
                    }
                    None => break,
                }
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

        let start_ns = (self.clock)();

        let dirty = render_system::collect_dirty_region(&mut self.world, root, &transform);

        if let Some(area) = dirty {
            {
                let mut renderer = self.factory.make(&mut self.backend, &transform);
                render_system::render_region(&self.world, root, &transform, &area, &mut renderer);
            }
            self.backend.flush(&area);
        }

        let elapsed = (self.clock)().saturating_sub(start_ns);
        for p in &mut self.plugins {
            p.post_render(&mut self.world, elapsed);
        }
    }
}
