use alloc::format;

use crate::prelude::*;
use crate::types::Fixed64;

#[crate::component]
pub struct PinchTarget {
    pub(super) status: Signal<PinchStatus>,
    pub last_pinch: Fixed64,
    pub last_rotate: Fixed,
    pub visual_scale: Fixed,
    pub visual_scale64: Fixed64,
    pub visual_rotation: Fixed,
    pub pinch_events: u32,
    pub rotate_events: u32,
    pub mode: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PinchStatus {
    pub(super) mode: &'static str,
    pub(super) scale_pct: i32,
    pub(super) rotation_deg: i32,
    pub(super) pinch_events: u32,
    pub(super) rotate_events: u32,
}

impl PinchStatus {
    pub(super) const IDLE: Self = Self {
        mode: "IDLE",
        scale_pct: 100,
        rotation_deg: 0,
        pinch_events: 0,
        rotate_events: 0,
    };

    pub(super) fn label(self) -> alloc::string::String {
        format!(
            "{} · {}% · {} DEG · P{} R{}",
            self.mode, self.scale_pct, self.rotation_deg, self.pinch_events, self.rotate_events
        )
    }
}
