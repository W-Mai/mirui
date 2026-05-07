use crate::backend::{Backend, InputEvent};
use crate::components::button_system::button_system;
use crate::draw::SwRenderer;
use crate::ecs::{Entity, World};
use crate::event::dispatch::dispatch;
use crate::widget::render_system;

/// Main application entry point — ties World + Backend together
pub struct App<B: Backend> {
    pub world: World,
    pub backend: B,
    pub root: Option<Entity>,
}

impl<B: Backend> App<B> {
    pub fn new(backend: B) -> Self {
        Self {
            world: World::new(),
            backend,
            root: None,
        }
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
        render_system::render(&self.world, root, info.width, info.height, &mut renderer);
        self.backend.flush();
    }

    /// Poll one event
    pub fn poll_event(&mut self) -> Option<InputEvent> {
        self.backend.poll_event()
    }

    /// Simple run loop: render + poll until quit
    pub fn run(&mut self) {
        self.render();
        loop {
            match self.poll_event() {
                Some(InputEvent::Quit) => break,
                Some(event) => {
                    if let Some(root) = self.root {
                        let info = self.backend.display_info();
                        button_system(&mut self.world, root, &event, info.width, info.height);
                        dispatch(&self.world, root, &event, info.width, info.height);
                    }
                    self.render();
                }
                None => {}
            }
        }
    }
}
