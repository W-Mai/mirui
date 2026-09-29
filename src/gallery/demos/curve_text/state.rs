use crate::prelude::Fixed;

pub(super) const BASE_STAGE_WIDTH: Fixed = Fixed::from_int(916);
pub(super) const BASE_STAGE_HEIGHT: Fixed = Fixed::from_int(360);

#[crate::model]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CurveModel {
    phase: Fixed,
    amplitude: Fixed,
    speed: Fixed,
    reversed: bool,
    paused: bool,
    stage_size: CurveStageSize,
}

impl Default for CurveModel {
    fn default() -> Self {
        Self {
            phase: Fixed::ZERO,
            amplitude: Fixed::from_int(68),
            speed: Fixed::from_int(64),
            reversed: false,
            paused: false,
            stage_size: CurveStageSize::BASE,
        }
    }
}

#[crate::model]
impl CurveModel {
    #[observe]
    pub(super) fn phase(&self) -> Fixed {
        self.phase
    }

    #[observe]
    pub(super) fn amplitude(&self) -> Fixed {
        self.amplitude
    }

    #[observe]
    pub(super) fn speed(&self) -> Fixed {
        self.speed
    }

    #[observe]
    pub(super) fn reversed(&self) -> bool {
        self.reversed
    }

    #[observe]
    pub(super) fn paused(&self) -> bool {
        self.paused
    }

    #[observe]
    pub(super) fn stage_size(&self) -> CurveStageSize {
        self.stage_size
    }

    pub(super) fn set_phase(&mut self, phase: Fixed) {
        if self.phase == phase {
            return;
        }
        self.phase = phase;
    }

    pub(super) fn set_amplitude(&mut self, value: Fixed) {
        let value = value
            .round()
            .clamp(Fixed::from_int(24), Fixed::from_int(96));
        replace(&mut self.amplitude, value);
    }

    pub(super) fn set_speed(&mut self, value: Fixed) {
        let value = value
            .round()
            .clamp(Fixed::from_int(20), Fixed::from_int(120));
        replace(&mut self.speed, value);
    }

    pub(super) fn toggle_direction(&mut self) {
        self.reversed = !self.reversed;
    }

    pub(super) fn toggle_paused(&mut self) {
        self.paused = !self.paused;
    }

    pub(super) fn set_stage_size(&mut self, size: CurveStageSize) {
        replace(&mut self.stage_size, size);
    }
}

fn replace<T: Eq>(current: &mut T, next: T) {
    if *current == next {
        return;
    }
    *current = next;
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
