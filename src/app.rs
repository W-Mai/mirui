use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::backend::{Backend, InputEvent};
use crate::components::button_system::button_system;
use crate::components::scroll_system::{ScrollDragState, scroll_inertia_system, scroll_system};
use crate::draw::SwDrawBackend;
use crate::draw::backend::DrawBackend;
use crate::draw::renderer::Renderer;
use crate::draw::texture::Texture;
use crate::ecs::{DeltaTime, ElapsedTime, Entity, System, SystemScheduler, World};
use crate::event::dispatch::dispatch;
use crate::plugin::Plugin;
use crate::types::{CoordTransform, Rect};
use crate::widget::render_system;

/// Monotonic clock the App uses to measure per-frame render time. Plugins can
/// swap it (e.g. StdInstantClockPlugin on std, ESP systimer on embedded). The
/// default returns 0, which makes `post_render` hooks see 0 and skip their
/// timing logic.
pub type ClockFn = Box<dyn FnMut() -> u64>;

/// Builds a Renderer each frame from a borrowed framebuffer Texture.
/// `App` asks its factory for a fresh Renderer per render call so custom
/// renderers (e.g. `compose_backend!` outputs) can plug in where the
/// default `SwDrawBackend` used to be hard-coded.
pub trait RendererFactory {
    type Renderer<'a>: Renderer + DrawBackend
    where
        Self: 'a;
    fn make<'a>(
        &'a mut self,
        tex: Texture<'a>,
        transform: &CoordTransform,
    ) -> Self::Renderer<'a>;
}

/// Default factory that produces plain `SwDrawBackend<'a>`.
pub struct SwDrawBackendFactory;

impl RendererFactory for SwDrawBackendFactory {
    type Renderer<'a> = SwDrawBackend<'a>;
    fn make<'a>(
        &'a mut self,
        tex: Texture<'a>,
        transform: &CoordTransform,
    ) -> SwDrawBackend<'a> {
        let mut r = SwDrawBackend::new(tex);
        r.scale = transform.scale();
        r
    }
}

/// Main application entry point — ties World + Backend + Renderer factory together
pub struct App<B: Backend, F: RendererFactory = SwDrawBackendFactory> {
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

impl<B: Backend> App<B, SwDrawBackendFactory> {
    pub fn new(backend: B) -> Self {
        Self::with_factory(backend, SwDrawBackendFactory)
    }
}

impl<B: Backend, F: RendererFactory> App<B, F> {
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
        let transform = info.transform();

        for p in &mut self.plugins {
            p.pre_render(&mut self.world);
        }

        let start_ns = (self.clock)();

        render_system::update_layout(&mut self.world, root, &transform);

        {
            let buf = self.backend.framebuffer();
            let tex = Texture::new(buf, info.width, info.height, info.format);
            let mut renderer = self.factory.make(tex, &transform);
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
        let transform = info.transform();
        render_system::collect_dirty_region(&mut self.world, root, &transform)
    }

    /// Poll one event
    pub fn poll_event(&mut self) -> Option<InputEvent> {
        self.backend.poll_event()
    }

    /// Simple run loop: systems + render + poll until quit
    pub fn run(&mut self) {
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
                                self.backend.display_info().transform().logical_size()
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
            self.render_dirty();
        }
    }

    /// Render only dirty regions. Falls back to full render if no dirty tracking.
    pub fn render_dirty(&mut self) {
        let Some(root) = self.root else { return };
        let info = self.backend.display_info();
        let transform = info.transform();

        for p in &mut self.plugins {
            p.pre_render(&mut self.world);
        }

        let start_ns = (self.clock)();

        let dirty = render_system::collect_dirty_region(&mut self.world, root, &transform);

        if let Some(area) = dirty {
            {
                let buf = self.backend.framebuffer();
                let tex = Texture::new(buf, info.width, info.height, info.format);
                let mut renderer = self.factory.make(tex, &transform);
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
