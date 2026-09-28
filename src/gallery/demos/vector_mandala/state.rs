use core::cell::RefCell;

use crate::render::scene::Scene;

pub struct VectorMandala {
    pub start_ms: u32,
    pub petals: u8,
    pub(super) frame: RefCell<Scene>,
    pub(super) emblem: RefCell<Option<Scene>>,
}

impl Default for VectorMandala {
    fn default() -> Self {
        Self {
            start_ms: 0,
            petals: 10,
            frame: RefCell::new(Scene::new()),
            emblem: RefCell::new(None),
        }
    }
}
