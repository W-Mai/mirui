use crate::core::reactive::Signal;
use crate::prelude::*;

pub(super) const BASE_STAGE_WIDTH: Fixed = Fixed::from_int(916);
pub(super) const BASE_STAGE_HEIGHT: Fixed = Fixed::from_int(360);

#[derive(Clone)]
pub(super) struct CurveModel {
    pub(super) phase: Signal<Fixed>,
    pub(super) amplitude: Signal<Fixed>,
    pub(super) speed: Signal<Fixed>,
    pub(super) reversed: Signal<bool>,
    pub(super) paused: Signal<bool>,
    pub(super) stage_size: Signal<CurveStageSize>,
}

impl Default for CurveModel {
    fn default() -> Self {
        Self {
            phase: Signal::new(Fixed::ZERO),
            amplitude: Signal::new(Fixed::from_int(68)),
            speed: Signal::new(Fixed::from_int(64)),
            reversed: Signal::new(false),
            paused: Signal::new(false),
            stage_size: Signal::new(CurveStageSize::BASE),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CurveStageSize {
    pub(super) width: Fixed,
    pub(super) height: Fixed,
}

impl CurveStageSize {
    const BASE: Self = Self {
        width: BASE_STAGE_WIDTH,
        height: BASE_STAGE_HEIGHT,
    };

    pub(super) fn from_dimensions(width: Fixed, height: Fixed) -> Self {
        Self {
            width: width.max(Fixed::from_int(120)),
            height: height.max(Fixed::from_int(220)),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct CurveNodes {
    pub(super) stage: Entity,
    pub(super) primary: Entity,
    pub(super) multiscript: Entity,
    pub(super) caption: Entity,
}

pub(super) enum CurveAction {
    SetAmplitude(Fixed),
    SetSpeed(Fixed),
    ToggleDirection,
    TogglePaused,
}

impl CurveAction {
    pub(super) fn publish(self, world: &mut World) {
        let Some(model) = world.resource::<CurveModel>().cloned() else {
            return;
        };
        match self {
            Self::SetAmplitude(value) => {
                let next = value
                    .round()
                    .clamp(Fixed::from_int(24), Fixed::from_int(96));
                if next != model.amplitude.get_untracked() {
                    model.amplitude.set(next);
                }
            }
            Self::SetSpeed(value) => {
                let next = value
                    .round()
                    .clamp(Fixed::from_int(20), Fixed::from_int(120));
                if next != model.speed.get_untracked() {
                    model.speed.set(next);
                }
            }
            Self::ToggleDirection => model.reversed.update(|value| *value = !*value),
            Self::TogglePaused => model.paused.update(|value| *value = !*value),
        }
    }
}
