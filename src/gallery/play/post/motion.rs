//! What moves at the post besides the parcels: the switch levers swinging
//! over, a ring from a station that takes a parcel (with a cross when it is
//! the wrong one), and the star of a clean shift. None of it is saved.

use crate::anim::{Spring, SpringConfig, Tween, TweenConfig, ease};
use crate::types::Fixed;

const LEVER: SpringConfig = crate::anim::SNAPPY;
const RING: TweenConfig = TweenConfig::new(400, &ease::OUT_CUBIC);
/// A star landing with a little overshoot.
const POP: SpringConfig = SpringConfig {
    duration_ms: 280,
    bounce: Fixed::from_ratio(77, 256),
};

/// Angle of a switch lever in degrees for the branch it sends parcels to:
/// the first switch sends branch 0 up to A, the second sends branch 1 down to C.
pub(crate) const fn lever_angle(switch: usize, branch: u8) -> i32 {
    match (switch, branch) {
        (0, 0) => -55,
        (1, 1) => 55,
        _ => 0,
    }
}

fn resting(value: Fixed, config: SpringConfig) -> Spring {
    Spring::preset(value, value, config)
}

#[derive(Clone, Copy)]
pub(crate) struct PostMotion {
    levers: [Spring; 2],
    rings: [Tween; 3],
    /// Stations whose last parcel was the wrong one, one bit each.
    missed: u8,
    star: Spring,
}

impl PostMotion {
    pub(super) fn new() -> Self {
        Self {
            levers: [
                resting(Fixed::from_int(lever_angle(0, 0)), LEVER),
                resting(Fixed::from_int(lever_angle(1, 0)), LEVER),
            ],
            rings: [Tween::preset(Fixed::ONE, Fixed::ONE, RING); 3],
            missed: 0,
            star: resting(Fixed::ZERO, POP),
        }
    }

    pub(super) fn swing(&mut self, switch: usize, branch: u8) {
        self.levers[switch].retarget(Fixed::from_int(lever_angle(switch, branch)), None);
    }

    pub(super) fn deliver(&mut self, station: usize, correct: bool) {
        self.rings[station] = Tween::preset(Fixed::ZERO, Fixed::ONE, RING);
        if correct {
            self.missed &= !(1 << station);
        } else {
            self.missed |= 1 << station;
        }
    }

    pub(super) fn pop_star(&mut self) {
        self.star.retarget(Fixed::ONE, None);
    }

    /// Runs the motion on; returns whether there was any to draw.
    pub(super) fn advance(&mut self, elapsed_ms: u16) -> bool {
        let moving = self.moving();
        for lever in &mut self.levers {
            lever.tick(elapsed_ms);
        }
        for ring in &mut self.rings {
            ring.tick(elapsed_ms);
        }
        self.star.tick(elapsed_ms);
        moving
    }

    fn moving(&self) -> bool {
        self.levers.iter().any(|lever| !lever.is_settled())
            || self.rings.iter().any(|ring| !ring.is_finished())
            || !self.star.is_settled()
    }

    /// The angle switch `switch`'s lever points at, in degrees.
    pub(crate) fn lever(&self, switch: usize) -> Fixed {
        self.levers[switch].value()
    }

    /// The ring spreading from `station`, from 0 to 1, and whether the
    /// parcel that started it was the right one.
    pub(crate) fn ring(&self, station: usize) -> Option<(Fixed, bool)> {
        let ring = &self.rings[station];
        (!ring.is_finished()).then(|| (ring.value(), self.missed & (1 << station) == 0))
    }

    /// Scale of the summary star, from 0 to about 1.
    pub(crate) fn star(&self) -> Fixed {
        self.star.value()
    }
}
