use crate::anim::{BOUNCY, SMOOTH, SNAPPY};
use crate::ui::widgets::navigation::PageTransition;

#[derive(Clone, Copy)]
pub enum Route {
    Library,
    Place(u8),
    Notes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    Garden,
    Coast,
    Night,
}

#[crate::model]
pub struct Studio {
    #[observe]
    pub(super) effect: u8,
    #[observe]
    pub(super) spring: u8,
    #[observe]
    pub(super) scene: Scene,
}

impl Default for Studio {
    fn default() -> Self {
        Self {
            effect: 1,
            spring: 1,
            scene: Scene::Garden,
        }
    }
}

#[crate::model]
impl Studio {
    pub(super) fn set_effect(&mut self, value: u8) {
        self.effect = value;
    }
    pub(super) fn set_spring(&mut self, value: u8) {
        self.spring = value;
    }
    pub(super) fn select(&mut self, scene: Scene) {
        self.scene = scene;
    }
    pub(super) fn next(&mut self) {
        self.scene = match self.scene {
            Scene::Garden => Scene::Coast,
            Scene::Coast => Scene::Night,
            Scene::Night => Scene::Garden,
        };
    }
}

pub(super) fn spring_config(spring: u8) -> crate::anim::SpringConfig {
    match spring {
        0 => SMOOTH,
        1 => SNAPPY,
        _ => BOUNCY,
    }
}

pub(super) fn transition(effect: u8, spring: u8) -> PageTransition {
    let config = spring_config(spring);
    match effect {
        0 => PageTransition::None,
        1 => PageTransition::slide(config),
        _ => PageTransition::fade(config),
    }
}
