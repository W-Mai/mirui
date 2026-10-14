use crate::anim::{BOUNCY, INTERACTIVE, SMOOTH, SNAPPY, Transition, TweenConfig, ease};
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    Tween,
    Spring,
}

#[crate::model]
pub struct Studio {
    #[observe]
    pub(super) motion: MotionKind,
    #[observe]
    pub(super) effect: u8,
    #[observe]
    pub(super) preset: u8,
    #[observe]
    pub(super) scene: Scene,
}

impl Default for Studio {
    fn default() -> Self {
        Self {
            motion: MotionKind::Spring,
            effect: 1,
            preset: 1,
            scene: Scene::Garden,
        }
    }
}

#[crate::model]
impl Studio {
    pub(super) fn set_motion(&mut self, value: MotionKind) {
        if self.motion != value {
            self.motion = value;
            self.preset = 1;
        }
    }
    pub(super) fn set_effect(&mut self, value: u8) {
        self.effect = value;
    }
    pub(super) fn set_preset(&mut self, value: u8) {
        self.preset = value;
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

pub(super) fn transition_config(motion: MotionKind, preset: u8) -> Transition {
    match motion {
        MotionKind::Tween => TweenConfig::new(
            300,
            match preset {
                0 => &ease::LINEAR,
                1 => &ease::OUT_CUBIC,
                _ => &ease::IN_OUT_CUBIC,
            },
        )
        .into(),
        MotionKind::Spring => match preset {
            0 => SMOOTH,
            1 => SNAPPY,
            2 => BOUNCY,
            _ => INTERACTIVE,
        }
        .into(),
    }
}

pub(super) fn transition(effect: u8, motion: MotionKind, preset: u8) -> PageTransition {
    let config = transition_config(motion, preset);
    match effect {
        0 => PageTransition::None,
        1 => PageTransition::slide(config),
        _ => PageTransition::fade(config),
    }
}
