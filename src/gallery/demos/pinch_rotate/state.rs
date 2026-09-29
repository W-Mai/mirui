use crate::prelude::*;
use crate::types::{Fixed64, Transform};

#[crate::component(bind(model))]
pub(super) struct PinchTarget {
    pub(super) model: PinchModel,
}

#[crate::model]
#[derive(Debug)]
pub(super) struct PinchModel {
    scale: Fixed64,
    rotation: Fixed,
    pinch_events: u32,
    rotate_events: u32,
    mode: &'static str,
}

impl Default for PinchModel {
    fn default() -> Self {
        Self {
            scale: Fixed64::ONE,
            rotation: Fixed::ZERO,
            pinch_events: 0,
            rotate_events: 0,
            mode: "IDLE",
        }
    }
}

#[crate::model]
impl PinchModel {
    #[observe]
    pub(super) fn status(&self) -> PinchStatus {
        let rotation_deg = self.rotation * Fixed::from_int(180) / Fixed::PI;
        PinchStatus {
            mode: self.mode,
            scale_pct: (self.scale.to_fixed() * Fixed::from_int(100)).to_int(),
            rotation_deg: rotation_deg.to_int(),
            pinch_events: self.pinch_events,
            rotate_events: self.rotate_events,
        }
    }

    #[observe]
    pub(super) fn transform(&self) -> Transform {
        let scale = self.scale.to_fixed();
        let rotation_deg = self.rotation * Fixed::from_int(180) / Fixed::PI;
        Transform::scale(scale, scale).compose(&Transform::rotate_deg(rotation_deg))
    }

    pub(super) fn apply_pinch(&mut self, scale_delta: Fixed64) {
        let lo = Fixed64::from_ratio(65, 100);
        let hi = Fixed64::from_ratio(8, 5);
        self.scale = (self.scale * scale_delta).clamp(lo, hi);
        self.pinch_events = self.pinch_events.saturating_add(1);
        self.mode = if scale_delta > Fixed64::ONE {
            "EXPAND"
        } else if scale_delta < Fixed64::ONE {
            "SHRINK"
        } else {
            "PINCH"
        };
    }

    pub(super) fn apply_rotation(&mut self, angle: Fixed) {
        self.rotation += angle;
        self.rotate_events = self.rotate_events.saturating_add(1);
        self.mode = "ROTATE";
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PinchStatus {
    pub(super) mode: &'static str,
    pub(super) scale_pct: i32,
    pub(super) rotation_deg: i32,
    pub(super) pinch_events: u32,
    pub(super) rotate_events: u32,
}

impl core::fmt::Display for PinchStatus {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "{} · {}% · {} DEG · P{} R{}",
            self.mode, self.scale_pct, self.rotation_deg, self.pinch_events, self.rotate_events
        )
    }
}
