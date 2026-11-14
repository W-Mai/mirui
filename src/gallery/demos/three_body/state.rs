#![cfg_attr(not(feature = "std"), allow(dead_code))]
//! Three bodies tied together by springs, bouncing inside a ring.
//!
//! The whole simulation is fixed-point arithmetic on the `Orbit` model: one
//! `tick` is one step of motion, and the view repaints when `frame` changes.

use crate::gallery::play::plane::Vec2;
use crate::prelude::Fixed;

pub(super) const BODIES: usize = 3;
/// Positions each body remembers for its tail.
pub(super) const TRAIL: usize = 36;
/// The world is a circle of this radius, scaled by the view to fit its stage.
pub(super) const ARENA: i32 = 100;
/// How far a body's centre can get from the wall.
pub(super) const BODY_RADIUS: i32 = 5;
/// The length at which a spring pulls neither way.
pub(super) const REST: i32 = 78;

const STIFFNESS: Fixed = Fixed::from_ratio(1, 128);
/// Speed kept per step, so a kick fades over a few seconds.
const DRAG: Fixed = Fixed::from_ratio(255, 256);
const TOP_SPEED: i32 = 4;
/// One body gets a nudge every this many steps, so the motion never settles.
const KICK_EVERY: u32 = 150;
const KICK: i32 = 2;
/// Steps already run when the model is created.
const WARM_UP_STEPS: u32 = 120;

/// A body and the path it has just taken.
#[derive(Clone, Copy)]
pub(super) struct Body {
    pub(super) at: Vec2,
    velocity: Vec2,
    trail: [Vec2; TRAIL],
    newest: usize,
}

impl Body {
    pub(super) fn new(at: Vec2, velocity: Vec2) -> Self {
        Self {
            at,
            velocity,
            trail: [at; TRAIL],
            newest: 0,
        }
    }

    /// Past positions, oldest first.
    pub(super) fn trail(&self) -> impl Iterator<Item = Vec2> + '_ {
        (1..=TRAIL).map(move |age| self.trail[(self.newest + age) % TRAIL])
    }

    pub(super) fn advance(&mut self) {
        self.velocity = self.velocity * DRAG;
        let speed = self.velocity.length();
        if speed > Fixed::from_int(TOP_SPEED) {
            self.velocity = self.velocity / speed * Fixed::from_int(TOP_SPEED);
        }
        self.at += self.velocity;
        self.bounce();
        self.newest = (self.newest + 1) % TRAIL;
        self.trail[self.newest] = self.at;
    }

    /// Reflect off the ring when the body would leave it.
    fn bounce(&mut self) {
        let limit = Fixed::from_int(ARENA - BODY_RADIUS);
        let distance = self.at.length();
        if distance <= limit {
            return;
        }
        let outward = self.at / distance;
        self.at = outward * limit;
        let approach = self.velocity.dot(outward);
        if approach.is_positive() {
            self.velocity = self.velocity - outward * (approach * 2);
        }
    }
}

#[crate::model]
pub struct Orbit {
    pub(super) bodies: [Body; BODIES],
    step: u32,
    #[observe]
    pub(super) frame: u32,
}

impl Default for Orbit {
    /// The bodies start on a ring, each heading round it, and have already
    /// run for a moment so the first frame shows their tails.
    fn default() -> Self {
        let bodies = core::array::from_fn(|i| {
            let angle = Fixed::from_int(90 + 360 * i as i32 / BODIES as i32);
            let at = Vec2::heading(angle) * Fixed::from_int(54);
            let tangent = Vec2::heading(angle + Fixed::from_int(90));
            Body::new(at, tangent * Fixed::from_ratio(3, 2))
        });
        let mut orbit = Self {
            bodies,
            step: 0,
            frame: 0,
        };
        for _ in 0..WARM_UP_STEPS {
            orbit.step();
        }
        orbit
    }
}

#[crate::model]
impl Orbit {
    /// Advance the simulation by one step.
    pub(super) fn tick(&mut self) {
        self.step();
        self.frame = self.frame.wrapping_add(1);
    }

    pub(super) fn positions(&self) -> [Vec2; BODIES] {
        self.bodies.map(|body| body.at)
    }
}

impl Orbit {
    fn step(&mut self) {
        self.step += 1;
        if self.step % KICK_EVERY == 0 {
            self.kick();
        }
        self.pull_together();
        for body in &mut self.bodies {
            body.advance();
        }
    }

    /// Every pair of bodies is joined by a spring.
    fn pull_together(&mut self) {
        let at = self.positions();
        for i in 0..BODIES {
            for j in (i + 1)..BODIES {
                let apart = at[j] - at[i];
                let length = apart.length();
                if length.is_zero() {
                    continue;
                }
                let pull = apart / length * (STIFFNESS * (length - Fixed::from_int(REST)));
                self.bodies[i].velocity += pull;
                self.bodies[j].velocity += -pull;
            }
        }
    }

    /// Nudge the next body along a heading that keeps turning.
    fn kick(&mut self) {
        let turn = self.step / KICK_EVERY;
        let heading = Vec2::heading(Fixed::from_int((turn * 137 % 360) as i32));
        self.bodies[turn as usize % BODIES].velocity += heading * Fixed::from_int(KICK);
    }
}
