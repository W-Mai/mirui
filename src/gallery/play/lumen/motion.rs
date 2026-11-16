//! What moves on the board: the beam drawing along its path, a mirror
//! swinging a quarter turn, a fixed mirror refusing a tap, the receiver
//! lighting up and the stars of a solve. None of it is saved, and the next
//! input cuts any of it short.

use crate::anim::{Spring, SpringConfig, Tween, TweenConfig, ease};
use crate::types::Fixed;

/// A quarter turn that clicks into place.
const TURN: SpringConfig = crate::anim::SNAPPY;
/// A short, hard wobble.
const SHAKE: SpringConfig = SpringConfig {
    duration_ms: 240,
    bounce: Fixed::from_ratio(150, 256),
};
/// Sideways speed a refused tap gives a fixed mirror, in pixels a second.
const SHAKE_SPEED: i32 = 90;
/// A star landing with a little overshoot.
const POP: SpringConfig = SpringConfig {
    duration_ms: 280,
    bounce: Fixed::from_ratio(77, 256),
};
/// The ring that spreads from the receiver once it is lit.
const GLOW: TweenConfig = TweenConfig::new(360, &ease::OUT_CUBIC);
/// Time the beam takes for each cell it draws, and its bounds.
const BEAM_MS_PER_CELL: u16 = 24;
const BEAM_MIN_MS: u16 = 90;
const BEAM_MAX_MS: u16 = 360;
/// From the beam reaching a lit receiver to the result card.
pub(crate) const RESULT_DELAY_MS: u16 = 180;
/// Between two stars of the result.
const STAR_STAGGER_MS: u16 = 90;

/// The piece a nudge moves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Nudge {
    None,
    /// Rotatable mirror `index` swinging from its other orientation.
    Turn(u8),
    /// Fixed mirror `index` shaking off a tap.
    Shake(u8),
}

fn resting(value: Fixed, config: SpringConfig) -> Spring {
    Spring::preset(value, value, config)
}

#[derive(Clone, Copy)]
pub(crate) struct BoardMotion {
    /// Trace segments drawn so far.
    beam: Tween,
    nudge: Spring,
    nudged: Nudge,
    /// The ring around a lit receiver, from 0 to 1.
    glow: Tween,
    /// Time since the beam reached a lit receiver; zero before it does.
    since_lit_ms: u16,
    /// Scale of each result star, from 0 to 1.
    stars: [Spring; 3],
}

impl BoardMotion {
    pub(super) fn new() -> Self {
        Self {
            beam: Tween::preset(Fixed::ZERO, Fixed::ZERO, GLOW),
            nudge: resting(Fixed::ZERO, TURN),
            nudged: Nudge::None,
            glow: Tween::preset(Fixed::ONE, Fixed::ONE, GLOW),
            since_lit_ms: 0,
            stars: [resting(Fixed::ZERO, POP); 3],
        }
    }

    /// Draws the beam again from point `from` of a trace of `segments`
    /// steps; what lies before that point is shown at once.
    pub(super) fn draw_beam(&mut self, from: usize, segments: usize) {
        let from = from.min(segments);
        let cells = (segments - from) as u16;
        let duration = cells
            .saturating_mul(BEAM_MS_PER_CELL)
            .clamp(BEAM_MIN_MS, BEAM_MAX_MS);
        self.beam = Tween::preset(
            Fixed::from_int(from as i32),
            Fixed::from_int(segments as i32),
            TweenConfig::new(duration, &ease::OUT_QUAD),
        );
        self.since_lit_ms = 0;
        self.glow = Tween::preset(Fixed::ONE, Fixed::ONE, GLOW);
        self.stars = [resting(Fixed::ZERO, POP); 3];
    }

    pub(super) fn turn(&mut self, mirror: usize) {
        self.nudge = Spring::preset(Fixed::ZERO, Fixed::ONE, TURN);
        self.nudged = Nudge::Turn(mirror as u8);
    }

    pub(super) fn shake(&mut self, fixed: usize) {
        self.nudge = resting(Fixed::ZERO, SHAKE).with_velocity(Fixed::from_int(SHAKE_SPEED));
        self.nudged = Nudge::Shake(fixed as u8);
    }

    /// Runs the motion on; `earned` stars pop once a solved beam has arrived.
    /// Returns whether anything is still moving.
    pub(super) fn advance(&mut self, elapsed_ms: u16, solved: bool, earned: u8) -> bool {
        self.beam.tick(elapsed_ms);
        if self.nudged != Nudge::None {
            self.nudge.tick(elapsed_ms);
            if self.nudge.is_settled() {
                self.nudged = Nudge::None;
            }
        }
        if solved && self.beam.is_finished() {
            if self.since_lit_ms == 0 {
                self.glow = Tween::preset(Fixed::ZERO, Fixed::ONE, GLOW);
            }
            let before = self.since_lit_ms;
            self.since_lit_ms = self.since_lit_ms.saturating_add(elapsed_ms.max(1));
            for (index, star) in self.stars.iter_mut().enumerate() {
                let at = RESULT_DELAY_MS + index as u16 * STAR_STAGGER_MS;
                if index < usize::from(earned) && before < at && self.since_lit_ms >= at {
                    star.retarget(Fixed::ONE, None);
                }
            }
        }
        self.glow.tick(elapsed_ms);
        for star in &mut self.stars {
            star.tick(elapsed_ms);
        }
        self.moving()
    }

    /// Ends every motion where it was heading.
    pub(super) fn finish(&mut self, solved: bool, earned: u8) {
        let segments = self.beam.to;
        self.beam = Tween::preset(segments, segments, GLOW);
        self.nudge = resting(Fixed::ZERO, TURN);
        self.nudged = Nudge::None;
        self.glow = Tween::preset(Fixed::ONE, Fixed::ONE, GLOW);
        if solved {
            self.since_lit_ms = self.since_lit_ms.max(u16::MAX / 2);
            for (index, star) in self.stars.iter_mut().enumerate() {
                let scale = if index < usize::from(earned) {
                    Fixed::ONE
                } else {
                    Fixed::ZERO
                };
                *star = resting(scale, POP);
            }
        }
    }

    pub(super) fn moving(&self) -> bool {
        !self.beam.is_finished()
            || self.nudged != Nudge::None
            || !self.glow.is_finished()
            || self.stars.iter().any(|star| !star.is_settled())
    }

    /// How far along the trace the beam is drawn, in segments.
    pub(crate) fn front(&self) -> Fixed {
        self.beam.value()
    }

    pub(crate) fn beam_complete(&self) -> bool {
        self.beam.is_finished()
    }

    /// How far rotatable mirror `index` has swung, from 0 to about 1.
    pub(crate) fn turn_of(&self, index: usize) -> Option<Fixed> {
        (self.nudged == Nudge::Turn(index as u8)).then(|| self.nudge.value())
    }

    /// Sideways offset of fixed mirror `index`, in pixels.
    pub(crate) fn shake_of(&self, index: usize) -> Fixed {
        if self.nudged == Nudge::Shake(index as u8) {
            self.nudge.value()
        } else {
            Fixed::ZERO
        }
    }

    /// The spreading ring of a lit receiver, from 0 to 1; `None` once gone.
    pub(crate) fn glow(&self) -> Option<Fixed> {
        (!self.glow.is_finished()).then(|| self.glow.value())
    }

    /// The beam reached a lit receiver long enough ago to show the result.
    pub(crate) fn result_ready(&self) -> bool {
        self.since_lit_ms >= RESULT_DELAY_MS
    }

    /// Scale of result star `index`, from 0 to about 1.
    pub(crate) fn star(&self, index: usize) -> Fixed {
        self.stars[index].value()
    }
}
