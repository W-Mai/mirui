use super::system::{ScrollBounds, elastic_resist};
use super::velocity::VelocityTracker;
use crate::anim::{MAX_FRAME_DT_MS, SMOOTH};
use crate::ecs::Entity;
use crate::types::{Fixed, Fixed64};

#[derive(Clone, Copy)]
enum Edge {
    Start,
    End,
}

impl Edge {
    fn position(self, max: Fixed) -> Fixed {
        match self {
            Self::Start => Fixed::ZERO,
            Self::End => max,
        }
    }

    fn is_outward(self, delta: Fixed) -> bool {
        match self {
            Self::Start => delta < Fixed::ZERO,
            Self::End => delta > Fixed::ZERO,
        }
    }

    fn contains(self, offset: Fixed, max: Fixed) -> bool {
        match self {
            Self::Start => offset <= Fixed::ZERO,
            Self::End => offset >= max,
        }
    }
}

#[derive(Clone, Copy)]
struct Rebound {
    offset: Fixed64,
    velocity: Fixed64,
    target: Fixed,
}

impl Rebound {
    fn new(position: Fixed, target: Fixed, velocity: Fixed) -> Self {
        Self {
            offset: Fixed64::from_fixed(position - target),
            velocity: Fixed64::from_fixed(velocity),
            target,
        }
    }

    fn tick(&mut self, dt_ms: u16, target: Fixed) -> (Fixed, bool) {
        self.offset += Fixed64::from_fixed(self.target - target);
        self.target = target;
        let dt_ms = dt_ms.min(MAX_FRAME_DT_MS);
        let steps = dt_ms.div_ceil(16).max(1);
        let dt = Fixed64::from_ratio(i64::from(dt_ms), 1000 * i64::from(steps));
        let omega = Fixed64::from_fixed(Fixed::PI) * Fixed64::from_int(2000)
            / Fixed64::from_int(i64::from(SMOOTH.duration_ms));
        let z = omega * dt;
        let half_z = z / Fixed64::from_int(2);
        let z_squared_over_twelve = z * z / Fixed64::from_int(12);
        // [2/2] Pade approximation of exp(-z), with z <= 0.202 per step.
        let decay = (Fixed64::ONE - half_z + z_squared_over_twelve)
            / (Fixed64::ONE + half_z + z_squared_over_twelve);
        for _ in 0..steps {
            let drift = (self.velocity + omega * self.offset) * dt;
            self.offset = (self.offset + drift) * decay;
            self.velocity = (self.velocity - omega * drift) * decay;
        }
        let settled = self.offset.abs() < Fixed64::from_ratio(1, 256)
            && self.velocity.abs() < Fixed64::from_ratio(1, 64);
        let position = if settled {
            target
        } else {
            target + self.offset.to_fixed()
        };
        (position, settled)
    }
}

#[derive(Default)]
enum WheelAxis {
    #[default]
    Idle,
    Rebounding {
        edge: Edge,
        motion: Rebound,
    },
    Held(Edge),
}

impl WheelAxis {
    fn input(
        &mut self,
        offset: Fixed,
        delta: Fixed,
        max: Fixed,
        viewport: Fixed,
        velocity: Fixed,
        continuing: bool,
    ) -> Fixed {
        if delta == Fixed::ZERO {
            return offset;
        }
        // Wheel streams without native phases retain ownership through their tail.
        if continuing
            && let Self::Rebounding { edge, .. } | Self::Held(edge) = self
            && edge.is_outward(delta)
            && edge.contains(offset, max)
        {
            return offset;
        }
        let requested = offset + elastic_resist(offset, delta, max, viewport);
        let target = requested.clamp(Fixed::ZERO, max);
        if requested == target {
            *self = Self::Idle;
        } else {
            let edge = if requested < Fixed::ZERO {
                Edge::Start
            } else {
                Edge::End
            };
            // A reversal must not inherit the old direction's sampled velocity.
            let velocity = if (velocity > Fixed::ZERO) == (delta > Fixed::ZERO) {
                velocity
            } else {
                Fixed::ZERO
            };
            *self = Self::Rebounding {
                edge,
                motion: Rebound::new(requested, target, velocity),
            };
        }
        requested
    }

    fn tick(&mut self, dt_ms: u16, max: Fixed) -> Option<Fixed> {
        let Self::Rebounding { edge, motion } = self else {
            return None;
        };
        let (position, settled) = motion.tick(dt_ms, edge.position(max));
        if settled {
            *self = Self::Held(*edge);
        }
        Some(position)
    }

    fn is_animating(&self) -> bool {
        matches!(self, Self::Rebounding { .. })
    }
}

#[derive(Default)]
pub(super) struct WheelScroll {
    target: Option<Entity>,
    velocity: VelocityTracker,
    x: WheelAxis,
    y: WheelAxis,
}

impl WheelScroll {
    pub fn input(
        &mut self,
        target: Entity,
        offset: (Fixed, Fixed),
        delta: (Fixed, Fixed),
        bounds: ScrollBounds,
        now: u32,
    ) -> (Fixed, Fixed) {
        let continuing = self.target == Some(target) && self.velocity.is_recent(now);
        if self.target != Some(target) {
            *self = Self::default();
        }
        self.target = Some(target);
        if !continuing {
            self.velocity = VelocityTracker::default();
        }
        self.velocity.record_delta(delta.0, delta.1, now);
        let (vx, vy) = self.velocity.velocity(now);
        (
            self.x.input(
                offset.0,
                delta.0,
                bounds.max_x,
                bounds.viewport_width,
                vx,
                continuing,
            ),
            self.y.input(
                offset.1,
                delta.1,
                bounds.max_y,
                bounds.viewport_height,
                vy,
                continuing,
            ),
        )
    }

    pub fn active_target(&self) -> Option<Entity> {
        self.target
            .filter(|_| self.x.is_animating() || self.y.is_animating())
    }

    pub fn tick(&mut self, dt_ms: u16, bounds: ScrollBounds) -> (Option<Fixed>, Option<Fixed>) {
        (
            self.x.tick(dt_ms, bounds.max_x),
            self.y.tick(dt_ms, bounds.max_y),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebound_retains_velocity_and_converges_without_crossing_or_tail_stall() {
        for interval in [4, 8, 16, 32] {
            for direction in [-1, 1] {
                for speed in [0, 100, 1000, 4000] {
                    let target = Fixed::from_int(200);
                    let mut motion = Rebound::new(
                        target + Fixed::from_int(10 * direction),
                        target,
                        Fixed::from_int(speed * direction),
                    );
                    let mut previous = (motion.offset.abs()).to_fixed();
                    let mut returning = false;
                    let mut finished = false;
                    for _ in 0..3000 / interval {
                        let (position, settled) = motion.tick(interval, target);
                        let distance = (position - target) * direction;
                        assert!(distance >= Fixed::ZERO);
                        if distance < previous {
                            returning = true;
                        }
                        if returning {
                            assert!(distance <= previous);
                        }
                        if settled {
                            assert_eq!(position, target);
                            assert!(previous <= Fixed::from_ratio(1, 128));
                            finished = true;
                            break;
                        }
                        previous = distance;
                    }
                    assert!(
                        finished,
                        "interval={interval}, direction={direction}, speed={speed}"
                    );
                }
            }
        }
    }

    #[test]
    fn rebound_trajectory_is_independent_of_frame_partitioning() {
        for elapsed in [32, 96, 320, 640] {
            let mut fine = Rebound::new((-8).into(), Fixed::ZERO, (-1000).into());
            let mut coarse = fine;
            for _ in 0..elapsed / 4 {
                fine.tick(4, Fixed::ZERO);
            }
            for _ in 0..elapsed / 32 {
                coarse.tick(32, Fixed::ZERO);
            }
            assert!((fine.offset - coarse.offset).abs() < Fixed64::from_ratio(1, 4));
            assert!((fine.velocity - coarse.velocity).abs() < Fixed64::from_int(2));
        }
    }

    #[test]
    fn outward_tail_cannot_reenergize_or_restart_a_finished_rebound() {
        let mut axis = WheelAxis::default();
        let mut position = axis.input(
            0.into(),
            (-8).into(),
            200.into(),
            100.into(),
            (-1000).into(),
            false,
        );
        let mut control = match axis {
            WheelAxis::Rebounding { motion, .. } => motion,
            _ => panic!("rebound"),
        };
        for _ in 0..160 {
            for _ in 0..4 {
                assert_eq!(
                    axis.input(
                        position,
                        (-1).into(),
                        200.into(),
                        100.into(),
                        (-500).into(),
                        true
                    ),
                    position
                );
            }
            let expected = control.tick(8, Fixed::ZERO).0;
            position = axis.tick(8, 200.into()).unwrap_or(position);
            assert_eq!(position, expected);
        }
        assert!(matches!(axis, WheelAxis::Held(Edge::Start)));
        assert_eq!(
            axis.input(
                position,
                Fixed::from_ratio(-1, 256),
                200.into(),
                100.into(),
                Fixed::ZERO,
                true
            ),
            Fixed::ZERO
        );
        assert!(!axis.is_animating());
        assert_eq!(
            axis.input(
                position,
                5.into(),
                200.into(),
                100.into(),
                (-10).into(),
                true
            ),
            5.into()
        );
        assert!(matches!(axis, WheelAxis::Idle));
    }

    #[test]
    fn new_sequence_and_reverse_input_take_over_immediately() {
        let mut axis = WheelAxis::default();
        let position = axis.input(
            0.into(),
            (-20).into(),
            200.into(),
            100.into(),
            (-1000).into(),
            false,
        );
        let reversed = axis.input(
            position,
            4.into(),
            200.into(),
            100.into(),
            (-1000).into(),
            true,
        );
        assert_eq!(reversed, position + Fixed::from_int(4));
        let restarted = axis.input(
            reversed,
            (-4).into(),
            200.into(),
            100.into(),
            Fixed::ZERO,
            false,
        );
        assert!(restarted < reversed);
    }

    #[test]
    fn wheel_sequence_preserves_entry_velocity_and_releases_after_a_pause() {
        let target = Entity {
            id: 1,
            generation: 0,
        };
        let bounds = ScrollBounds {
            viewport_width: 100.into(),
            viewport_height: 100.into(),
            max_x: 200.into(),
            max_y: 200.into(),
            ..ScrollBounds::default()
        };
        for horizontal in [false, true] {
            for sign in [-1, 1] {
                let mut wheel = WheelScroll::default();
                let start = if sign < 0 { 24 } else { 176 };
                let mut offset = (Fixed::from_int(start), Fixed::from_int(start));
                let delta = if horizontal {
                    (Fixed::from_int(8 * sign), Fixed::ZERO)
                } else {
                    (Fixed::ZERO, Fixed::from_int(8 * sign))
                };
                for time in [0, 8, 16, 24] {
                    offset = wheel.input(target, offset, delta, bounds, time);
                }
                let axis = if horizontal { &wheel.x } else { &wheel.y };
                let WheelAxis::Rebounding { motion, .. } = axis else {
                    panic!("rebound")
                };
                assert_eq!(motion.velocity, Fixed64::from_int(1000 * i64::from(sign)));
                assert_eq!(wheel.input(target, offset, delta, bounds, 32), offset);
                assert_ne!(wheel.input(target, offset, delta, bounds, 200), offset);
                let other = Entity {
                    id: 2,
                    generation: 0,
                };
                let center = (50.into(), 50.into());
                assert_eq!(
                    wheel.input(other, center, delta, bounds, 208),
                    (center.0 + delta.0, center.1 + delta.1)
                );
                assert_eq!(wheel.active_target(), None);
            }
        }
    }

    #[test]
    fn held_edge_does_not_block_newly_available_content() {
        let mut axis = WheelAxis::Held(Edge::End);
        assert_eq!(
            axis.input(
                200.into(),
                8.into(),
                400.into(),
                100.into(),
                1000.into(),
                true
            ),
            208.into()
        );
        assert!(matches!(axis, WheelAxis::Idle));
    }
}
