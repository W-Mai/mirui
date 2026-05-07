use crate::backend::{Backend, InputEvent};
use crate::components::button_system::button_system;
use crate::draw::SwRenderer;
use crate::ecs::{DeltaTime, ElapsedTime, Entity, System, SystemScheduler, World};
use crate::event::dispatch::dispatch;
use crate::types::Rect;
use crate::widget::render_system;

/// Main application entry point — ties World + Backend together
pub struct App<B: Backend> {
    pub world: World,
    pub backend: B,
    pub root: Option<Entity>,
    pub systems: SystemScheduler,
}

impl<B: Backend> App<B> {
    pub fn new(backend: B) -> Self {
        let mut world = World::new();
        world.insert_resource(DeltaTime(0.0));
        world.insert_resource(ElapsedTime(0.0));
        Self {
            world,
            backend,
            root: None,
            systems: SystemScheduler::new(),
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
        let buf = self.backend.framebuffer();
        let mut renderer = SwRenderer::new(buf, info.width as u32, info.height as u32);
        renderer.scale = info.scale;
        render_system::render(
            &self.world,
            root,
            info.width,
            info.height,
            info.scale,
            &mut renderer,
        );
        self.backend.flush(&Rect {
            x: 0,
            y: 0,
            w: info.width,
            h: info.height,
        });
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

            match self.poll_event() {
                Some(InputEvent::Quit) => break,
                Some(event) => {
                    if let Some(root) = self.root {
                        let info = self.backend.display_info();
                        button_system(&mut self.world, root, &event, info.width, info.height);
                        dispatch(&self.world, root, &event, info.width, info.height);
                    }
                }
                None => {}
            }

            self.render_dirty();
        }
    }

    /// Render only dirty regions. Falls back to full render if no dirty tracking.
    pub fn render_dirty(&mut self) {
        let Some(root) = self.root else { return };
        let info = self.backend.display_info();
        let scale = if info.scale == 0 { 1 } else { info.scale };

        // Collect dirty region
        let dirty = render_system::collect_dirty_region(
            &mut self.world,
            root,
            info.width,
            info.height,
            scale,
        );

        if let Some(area) = dirty {
            let buf = self.backend.framebuffer();
            let mut renderer = SwRenderer::new(buf, info.width as u32, info.height as u32);
            renderer.scale = scale;
            render_system::render_region(
                &self.world,
                root,
                info.width,
                info.height,
                scale,
                &area,
                &mut renderer,
            );
            self.backend.flush(&area);
        }
    }
}
