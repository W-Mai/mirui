use crate::backend::{Backend, InputEvent};
use crate::components::button_system::button_system;
use crate::components::scroll_system::{ScrollDragState, scroll_inertia_system, scroll_system};
use crate::draw::SwDrawBackend;
use crate::draw::backend::DrawBackend;
use crate::draw::renderer::Renderer;
use crate::draw::texture::Texture;
use crate::ecs::{DeltaTime, ElapsedTime, Entity, System, SystemScheduler, World};
use crate::event::dispatch::dispatch;
use crate::types::{Fixed, Rect};
use crate::widget::render_system;

/// Builds a Renderer each frame from a borrowed framebuffer Texture.
/// `App` asks its factory for a fresh Renderer per render call so custom
/// renderers (e.g. `compose_backend!` outputs) can plug in where the
/// default `SwDrawBackend` used to be hard-coded.
pub trait RendererFactory {
    type Renderer<'a>: Renderer + DrawBackend
    where
        Self: 'a;
    fn make<'a>(&'a mut self, tex: Texture<'a>, scale: Fixed) -> Self::Renderer<'a>;
}

/// Default factory that produces plain `SwDrawBackend<'a>`.
pub struct SwDrawBackendFactory;

impl RendererFactory for SwDrawBackendFactory {
    type Renderer<'a> = SwDrawBackend<'a>;
    fn make<'a>(&'a mut self, tex: Texture<'a>, scale: Fixed) -> SwDrawBackend<'a> {
        let mut r = SwDrawBackend::new(tex);
        r.scale = scale;
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
            #[cfg(feature = "perf")]
            perf: None,
        }
    }

    pub fn add_system(&mut self, system: System) {
        self.systems.add(system);
    }

    pub fn set_root(&mut self, root: Entity) {
        self.root = Some(root);
    }

    /// Render one frame
    pub fn render(&mut self) {
        let Some(root) = self.root else { return };
        let info = self.backend.display_info();

        // Update layout → write ComputedRect to entities
        render_system::update_layout(&mut self.world, root, info.width, info.height, info.scale);

        {
            let buf = self.backend.framebuffer();
            let tex = Texture::new(buf, info.width, info.height, info.format);
            let mut renderer = self.factory.make(tex, info.scale);
            render_system::render(
                &self.world,
                root,
                info.width,
                info.height,
                info.scale,
                &mut renderer,
            );
        }
        self.backend
            .flush(&Rect::new(0, 0, info.width, info.height));
    }

    /// Get the dirty region (physical pixels) after event processing, clearing dirty flags.
    pub fn dirty_region(&mut self) -> Option<Rect> {
        let root = self.root?;
        let info = self.backend.display_info();
        render_system::collect_dirty_region(
            &mut self.world,
            root,
            info.width,
            info.height,
            info.scale,
        )
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

            // Drain all pending events
            loop {
                match self.poll_event() {
                    Some(InputEvent::Quit) => return,
                    Some(event) => {
                        if let Some(root) = self.root {
                            let info = self.backend.display_info();
                            let scale = if info.scale == Fixed::ZERO {
                                Fixed::ONE
                            } else {
                                info.scale
                            };
                            let lw = (Fixed::from(info.width) / scale).to_int() as u16;
                            let lh = (Fixed::from(info.height) / scale).to_int() as u16;
                            button_system(&mut self.world, root, &event, lw, lh);
                            scroll_system(&mut self.world, root, &event, lw, lh);
                            dispatch(&self.world, root, &event, lw, lh);
                        }
                    }
                    None => break,
                }
            }

            scroll_inertia_system(&mut self.world);
            self.render_dirty();
        }
    }

    /// Render only dirty regions. Falls back to full render if no dirty tracking.
    pub fn render_dirty(&mut self) {
        let Some(root) = self.root else { return };
        let info = self.backend.display_info();
        let scale = if info.scale == Fixed::ZERO {
            Fixed::ONE
        } else {
            info.scale
        };

        // Collect dirty region
        let dirty = render_system::collect_dirty_region(
            &mut self.world,
            root,
            info.width,
            info.height,
            scale,
        );

        if let Some(area) = dirty {
            {
                let buf = self.backend.framebuffer();
                let tex = Texture::new(buf, info.width, info.height, info.format);
                let mut renderer = self.factory.make(tex, scale);
                render_system::render_region(
                    &self.world,
                    root,
                    info.width,
                    info.height,
                    scale,
                    &area,
                    &mut renderer,
                );
            }
            self.backend.flush(&area);
        }
    }
}
