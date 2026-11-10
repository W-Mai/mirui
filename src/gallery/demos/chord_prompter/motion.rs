use crate::types::{Fixed, Transform};

/// Bar-relative motion in 1/256 units; independent of render-frame cadence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarMotion {
    pub travel: u16,
    pub impact: u16,
}

impl BarMotion {
    pub fn at(phase: u16, has_next: bool) -> Self {
        let phase = u32::from(phase.min(256));
        let travel = if has_next { phase } else { 0 };
        Self {
            travel: travel as u16,
            impact: Self::emphasis(travel as i32),
        }
    }

    pub fn emphasis(distance: i32) -> u16 {
        let near = 256_u32.saturating_sub(distance.unsigned_abs().saturating_mul(2));
        (near * near / 256) as u16
    }

    pub(super) fn transform(self, width: Fixed, offset: i32) -> Transform {
        let offset = offset * 256 - i32::from(self.travel);
        Transform::translate(
            (width * Fixed::from_ratio(offset, 256)).round(),
            Fixed::ZERO,
        )
    }
}

pub(super) fn card_width(width: Fixed) -> Fixed {
    (width * Fixed::from_ratio(3, 4)).round()
}

pub(super) fn card_transform(phase: u16, has_next: bool, width: Fixed, offset: i32) -> Transform {
    BarMotion::at(phase, has_next).transform(card_width(width), offset)
}

pub(super) fn chord_size(phase: u16, has_next: bool, large: bool, offset: i32) -> u16 {
    let motion = BarMotion::at(phase, has_next);
    let emphasized = BarMotion::emphasis(offset * 256 - i32::from(motion.travel)) >= 96;
    match (large, emphasized) {
        (true, true) => 48,
        (true, false) | (false, true) => 36,
        (false, false) => 24,
    }
}
