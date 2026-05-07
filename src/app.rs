use crate::backend::{Backend, InputEvent};
use crate::components::button_system::button_system;
use crate::draw::SwRenderer;
use crate::ecs::{Entity, System, SystemScheduler, World};
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
        Self {
            world: World::new(),
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
        self.backend.flush();
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

            self.render();
        }
    }
}
