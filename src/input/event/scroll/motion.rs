use crate::anim::{MAX_FRAME_DT_MS, SMOOTH};
use crate::types::{Fixed, Fixed64};

const PROJECTION_TIME: Fixed64 = Fixed64::from_ratio(120, 1000);

pub(crate) fn project_target(position: Fixed, velocity: Fixed, max: Fixed) -> Fixed {
    project(
        Fixed64::from_fixed(position),
        Fixed64::from_fixed(velocity),
        Fixed64::from_fixed(max.max(Fixed::ZERO)),
    )
    .to_fixed()
}

fn project(position: Fixed64, velocity: Fixed64, max: Fixed64) -> Fixed64 {
    (position + velocity * PROJECTION_TIME).clamp(Fixed64::ZERO, max)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Region {
    Content,
    Start,
    End,
}

impl Region {
    fn at(position: Fixed64, max: Fixed64) -> Self {
        if position < Fixed64::ZERO {
            Self::Start
        } else if position > max {
            Self::End
        } else {
            Self::Content
        }
    }

    fn moving_inward(self, velocity: Fixed64) -> bool {
        match self {
            Self::Start => velocity > Fixed64::ZERO,
            Self::End => velocity < Fixed64::ZERO,
            Self::Content => false,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ScrollAnimation {
    position: Fixed64,
    velocity: Fixed64,
    target: Fixed64,
}

impl ScrollAnimation {
    pub fn new(position: Fixed, target: Fixed, velocity: Fixed) -> Self {
        Self {
            position: Fixed64::from_fixed(position),
            velocity: Fixed64::from_fixed(velocity),
            target: Fixed64::from_fixed(target),
        }
    }

    pub fn value(&self) -> Fixed {
        self.position.to_fixed()
    }

    #[cfg(test)]
    pub fn velocity(&self) -> Fixed {
        self.velocity.to_fixed()
    }

    pub fn is_settled(&self) -> bool {
        self.position == self.target && self.velocity == Fixed64::ZERO
    }

    pub fn tick(&mut self, dt_ms: u16, max: Fixed, elastic: bool) -> Fixed {
        let max = Fixed64::from_fixed(max.max(Fixed::ZERO));
        self.target = self.target.clamp(Fixed64::ZERO, max);
        self.constrain(max, elastic);
        let dt_ms = dt_ms.min(MAX_FRAME_DT_MS);
        let steps = dt_ms.div_ceil(16).max(1);
        let dt = Fixed64::from_ratio(i64::from(dt_ms), 1000 * i64::from(steps));
        let omega = Fixed64::from_fixed(Fixed::PI) * Fixed64::from_int(2000)
            / Fixed64::from_int(i64::from(SMOOTH.duration_ms));
        let decay = Self::decay(omega, dt);
        for _ in 0..steps {
            let region = Region::at(self.position, max);
            let offset = self.position - self.target;
            let slope = self.velocity + omega * offset;
            // Boundary entry occurs where offset + slope * t == 0.
            let entry = region
                .moving_inward(slope)
                .then(|| -offset / slope)
                .filter(|entry| *entry <= dt);
            if let Some(entry) = entry {
                self.advance(entry, omega, Self::decay(omega, entry));
                self.position = self.target;
                self.target = project(self.position, self.velocity, max);
                let remainder = dt - entry;
                self.advance(remainder, omega, Self::decay(omega, remainder));
            } else {
                self.advance(dt, omega, decay);
            }
            self.constrain(max, elastic);
        }
        if (self.position - self.target).abs() < Fixed64::from_ratio(1, 256)
            && self.velocity.abs() < Fixed64::from_ratio(1, 64)
        {
            self.position = self.target;
            self.velocity = Fixed64::ZERO;
        }
        self.value()
    }

    fn decay(omega: Fixed64, dt: Fixed64) -> Fixed64 {
        let z = omega * dt;
        let half_z = z / Fixed64::from_int(2);
        let z_squared_over_twelve = z * z / Fixed64::from_int(12);
        // [2/2] Pade approximation of exp(-z), with z <= 0.202 per step.
        (Fixed64::ONE - half_z + z_squared_over_twelve)
            / (Fixed64::ONE + half_z + z_squared_over_twelve)
    }

    fn advance(&mut self, dt: Fixed64, omega: Fixed64, decay: Fixed64) {
        let offset = self.position - self.target;
        let drift = (self.velocity + omega * offset) * dt;
        self.position = self.target + (offset + drift) * decay;
        self.velocity = (self.velocity - omega * drift) * decay;
    }

    fn constrain(&mut self, max: Fixed64, elastic: bool) {
        let region = Region::at(self.position, max);
        match region {
            Region::Start => self.target = Fixed64::ZERO,
            Region::End => self.target = max,
            Region::Content => {}
        }
        if !elastic {
            self.position = self.position.clamp(Fixed64::ZERO, max);
            if region.moving_inward(self.velocity) {
                self.target = project(self.position, self.velocity, max);
            }
            if (self.position == Fixed64::ZERO && self.velocity < Fixed64::ZERO)
                || (self.position == max && self.velocity > Fixed64::ZERO)
            {
                self.velocity = Fixed64::ZERO;
            }
        }
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
                    let target = Fixed::from_int(if direction < 0 { 0 } else { 200 });
                    let mut motion = ScrollAnimation::new(
                        target + Fixed::from_int(10 * direction),
                        target,
                        Fixed::from_int(speed * direction),
                    );
                    let mut previous = Fixed::from_int(10);
                    let mut returning = false;
                    let mut finished = false;
                    for _ in 0..3000 / interval {
                        let position = motion.tick(interval, 200.into(), true);
                        let distance = (position - target) * direction;
                        assert!(distance >= Fixed::ZERO);
                        if distance < previous {
                            returning = true;
                        }
                        if returning {
                            assert!(distance <= previous);
                        }
                        if motion.is_settled() {
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
            let mut fine = ScrollAnimation::new((-8).into(), Fixed::ZERO, (-1000).into());
            let mut coarse = fine;
            for _ in 0..elapsed / 4 {
                fine.tick(4, 200.into(), true);
            }
            for _ in 0..elapsed / 32 {
                coarse.tick(32, 200.into(), true);
            }
            assert!((fine.position - coarse.position).abs() < Fixed64::from_ratio(1, 4));
            assert!((fine.velocity - coarse.velocity).abs() < Fixed64::from_int(2));
        }
    }

    #[test]
    fn opposite_edges_have_mirrored_motion() {
        let max = Fixed::from_int(200);
        for dt in [4, 8, 16, 32] {
            let mut start = ScrollAnimation::new((-8).into(), Fixed::ZERO, (-1000).into());
            let mut end = ScrollAnimation::new(208.into(), max, 1000.into());
            for _ in 0..2000 / dt {
                start.tick(dt, max, true);
                end.tick(dt, max, true);
                assert!(
                    (start.position + end.position - Fixed64::from_fixed(max)).abs()
                        < Fixed64::from_ratio(1, 64)
                );
                assert!((start.velocity + end.velocity).abs() < Fixed64::from_ratio(1, 32));
            }
        }
    }

    #[test]
    fn crossing_a_boundary_redirects_the_target_without_resetting_velocity() {
        let mut motion = ScrollAnimation::new(1.into(), 8.into(), (-1000).into());
        let mut control = motion;
        motion.tick(4, 200.into(), true);
        // Shifting the valid range keeps this control trajectory away from its edges.
        control.position += Fixed64::from_int(100);
        control.target += Fixed64::from_int(100);
        control.tick(4, 200.into(), true);
        assert!(motion.value() < Fixed::ZERO);
        assert_eq!(motion.target, Fixed64::ZERO);
        assert_eq!(motion.position, control.position - Fixed64::from_int(100));
        assert_eq!(motion.velocity(), control.velocity());
        assert!(motion.velocity() < Fixed::from_int(-800));
    }

    #[test]
    fn crossing_an_interior_target_does_not_replace_it_with_an_edge() {
        let mut motion = ScrollAnimation::new(80.into(), 100.into(), 1000.into());
        let mut crossed = false;
        for _ in 0..500 {
            crossed |= motion.tick(8, 200.into(), true) > Fixed::from_int(100);
            assert_eq!(motion.target, Fixed64::from_int(100));
        }
        assert!(crossed);
        assert!(motion.is_settled());
        assert_eq!(motion.value(), Fixed::from_int(100));
    }

    #[test]
    fn non_elastic_boundaries_remove_only_outward_velocity() {
        for direction in [-1, 1] {
            let target = Fixed::from_int(if direction < 0 { 0 } else { 200 });
            let position = target + Fixed::from_int(10 * direction);
            let mut outward = ScrollAnimation::new(position, target, (1000 * direction).into());
            assert_eq!(outward.tick(8, 200.into(), false), target);
            assert!(outward.is_settled());
            let mut inward = ScrollAnimation::new(position, target, (-1000 * direction).into());
            inward.tick(8, 200.into(), false);
            assert!((inward.value() - target) * direction < Fixed::ZERO);
            assert!(inward.velocity() * direction < Fixed::ZERO);
        }
    }

    #[test]
    fn inward_releases_continue_into_content_without_returning_to_the_old_edge() {
        for elastic in [false, true] {
            for direction in [-1, 1] {
                for dt in [4, 8, 16, 32] {
                    let edge = Fixed::from_int(if direction < 0 { 0 } else { 200 });
                    let mut motion = ScrollAnimation::new(
                        edge + Fixed::from_int(10 * direction),
                        edge,
                        Fixed::from_int(-1000 * direction),
                    );
                    let mut entered = false;
                    let mut previous = Fixed::ZERO;
                    for _ in 0..3000 / dt {
                        let position = motion.tick(dt, 200.into(), elastic);
                        let distance = (edge - position) * direction;
                        if distance > Fixed::ZERO && !entered {
                            entered = true;
                            assert!(motion.velocity() * direction < Fixed::ZERO);
                            assert!(
                                (Fixed64::from_fixed(edge) - motion.target) * i64::from(direction)
                                    > Fixed64::ZERO
                            );
                        }
                        if entered {
                            assert!(distance >= previous);
                            previous = distance;
                        }
                    }
                    assert!(entered);
                    assert!(motion.is_settled());
                    assert!(previous > Fixed::from_int(10));
                }
            }
        }
    }

    #[test]
    fn expanded_bounds_do_not_project_a_release_without_a_position_crossing() {
        let mut motion = ScrollAnimation::new(210.into(), 200.into(), (-1000).into());
        motion.tick(0, 200.into(), true);
        for _ in 0..16 {
            motion.tick(8, 300.into(), true);
            assert_eq!(motion.target, Fixed64::from_int(200));
        }
    }

    #[test]
    fn inward_boundary_handoff_is_independent_of_frame_partitioning() {
        for direction in [-1, 1] {
            let edge = Fixed::from_int(if direction < 0 { 0 } else { 200 });
            for elapsed in [32, 96, 320, 640] {
                let mut fine = ScrollAnimation::new(
                    edge + Fixed::from_int(10 * direction),
                    edge,
                    Fixed::from_int(-1000 * direction),
                );
                let mut coarse = fine;
                for _ in 0..elapsed / 4 {
                    fine.tick(4, 200.into(), true);
                }
                for _ in 0..elapsed / 32 {
                    coarse.tick(32, 200.into(), true);
                }
                assert!((fine.position - coarse.position).abs() < Fixed64::from_ratio(1, 4));
                assert!((fine.velocity - coarse.velocity).abs() < Fixed64::from_int(2));
                assert!((fine.target - coarse.target).abs() < Fixed64::from_ratio(1, 64));
            }
        }
    }

    #[test]
    fn projected_targets_use_the_same_displacement_and_bounds_as_motion() {
        assert_eq!(
            project_target(4.into(), (-1000).into(), 200.into()),
            Fixed::ZERO
        );
        assert_eq!(
            project_target(196.into(), 1000.into(), 200.into()),
            200.into()
        );
        assert_eq!(
            project_target(100.into(), 1000.into(), Fixed::ZERO),
            Fixed::ZERO
        );
        let position = Fixed::from_int(100);
        let velocity = Fixed::from_int(100);
        assert_eq!(
            project_target(position, velocity, 200.into()),
            project(
                Fixed64::from_fixed(position),
                Fixed64::from_fixed(velocity),
                Fixed64::from_int(200),
            )
            .to_fixed()
        );
    }

    #[test]
    fn changing_bounds_preserves_motion_and_clamps_the_destination() {
        let mut motion = ScrollAnimation::new(180.into(), 200.into(), 500.into());
        motion.tick(0, 100.into(), true);
        assert_eq!(motion.value(), Fixed::from_int(180));
        assert_eq!(motion.velocity(), Fixed::from_int(500));
        assert_eq!(motion.target, Fixed64::from_int(100));
        motion.tick(0, 400.into(), true);
        assert_eq!(motion.target, Fixed64::from_int(100));
        motion.tick(0, Fixed::ZERO, false);
        assert_eq!(motion.value(), Fixed::ZERO);
        assert!(motion.is_settled());
    }
}
