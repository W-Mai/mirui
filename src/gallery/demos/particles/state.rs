#![cfg_attr(not(feature = "std"), allow(dead_code))]
//! Six dots drifting in a box.
//!
//! Only the dots carry state. The pulse rings and the sliding bars are drawn
//! from the frame count alone, so a `tick` is one step of the dots and one
//! count on `frame`.

use crate::gallery::play::plane::Vec2;
use crate::prelude::Fixed;

/// The stage in world units; the view fits it to its area.
pub(super) const STAGE: (i32, i32) = (400, 240);
pub(super) const DOTS: usize = 6;
pub(super) const DOT_RADIUS: i32 = 4;

/// Where a dot is and where it is heading, per step.
#[derive(Clone, Copy)]
pub(super) struct Dot {
    pub(super) at: Vec2,
    velocity: Vec2,
}

impl Dot {
    pub(super) fn new(at: Vec2, velocity: Vec2) -> Self {
        Self { at, velocity }
    }

    /// Move one step, turning round at the walls.
    pub(super) fn advance(&mut self) {
        self.at += self.velocity;
        let (width, height) = (Fixed::from_int(STAGE.0), Fixed::from_int(STAGE.1));
        let margin = Fixed::from_int(DOT_RADIUS);
        if self.at.x < margin || self.at.x > width - margin {
            self.velocity.x = -self.velocity.x;
            self.at.x = self.at.x.max(margin).min(width - margin);
        }
        if self.at.y < margin || self.at.y > height - margin {
            self.velocity.y = -self.velocity.y;
            self.at.y = self.at.y.max(margin).min(height - margin);
        }
    }
}

#[crate::model]
pub struct Swarm {
    pub(super) dots: [Dot; DOTS],
    #[observe]
    pub(super) frame: u32,
}

impl Default for Swarm {
    /// The dots start scattered, each with a heading of its own.
    fn default() -> Self {
        let mut seed = 0x9E37_79B9_u32;
        let mut random = move |below: u32| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % below) as i32
        };
        let dots = core::array::from_fn(|_| {
            let at = Vec2::new(
                Fixed::from_int(20 + random(STAGE.0 as u32 - 40)),
                Fixed::from_int(20 + random(STAGE.1 as u32 - 40)),
            );
            let heading = Vec2::new(
                Fixed::from_ratio(random(201) - 100, 80),
                Fixed::from_ratio(random(201) - 100, 80),
            );
            Dot::new(at, heading)
        });
        Self { dots, frame: 0 }
    }
}

#[crate::model]
impl Swarm {
    /// Advance every dot by one step.
    pub(super) fn tick(&mut self) {
        for dot in &mut self.dots {
            dot.advance();
        }
        self.frame = self.frame.wrapping_add(1);
    }

    pub(super) fn positions(&self) -> [Vec2; DOTS] {
        self.dots.map(|dot| dot.at)
    }
}
