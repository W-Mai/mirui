use super::motion::{ScrollAnimation, project_target};
use super::system::{ScrollBounds, elastic_resist};
use super::velocity::VelocityTracker;
use crate::ecs::Entity;
use crate::types::Fixed;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Driver {
    Pointer,
    Stream,
}

struct InputSession {
    driver: Driver,
    velocity: VelocityTracker,
}

/// Retained input and animation state for scrolling.
#[derive(Default)]
pub struct ScrollMotion {
    pub(crate) target_entity: Option<Entity>,
    pub(crate) x: Option<ScrollAnimation>,
    pub(crate) y: Option<ScrollAnimation>,
    session: Option<InputSession>,
}

pub(super) fn displace(
    offset: (Fixed, Fixed),
    delta: (Fixed, Fixed),
    bounds: ScrollBounds,
    elastic: bool,
) -> (Fixed, Fixed) {
    let axis = |offset, delta, max, viewport| {
        if elastic {
            offset + elastic_resist(offset, delta, max, viewport)
        } else {
            (offset + delta).clamp(Fixed::ZERO, max)
        }
    };
    (
        axis(offset.0, delta.0, bounds.max_x, bounds.viewport_width),
        axis(offset.1, delta.1, bounds.max_y, bounds.viewport_height),
    )
}

impl ScrollMotion {
    pub(super) fn begin_pointer(&mut self, target: Entity, now: u32) {
        let mut velocity = VelocityTracker::default();
        velocity.record_delta(Fixed::ZERO, Fixed::ZERO, now);
        *self = Self {
            target_entity: Some(target),
            session: Some(InputSession {
                driver: Driver::Pointer,
                velocity,
            }),
            ..Self::default()
        };
    }

    pub(super) fn record(
        &mut self,
        target: Entity,
        delta: (Fixed, Fixed),
        driver: Driver,
        now: u32,
    ) {
        if self.target_entity != Some(target)
            || !self.session.as_ref().is_some_and(|session| {
                session.driver == driver
                    && (driver == Driver::Pointer || session.velocity.is_recent(now))
            })
        {
            self.session = Some(InputSession {
                driver,
                velocity: VelocityTracker::default(),
            });
        }
        self.target_entity = Some(target);
        self.x = None;
        self.y = None;
        if let Some(session) = self.session.as_mut() {
            session.velocity.record_delta(delta.0, delta.1, now);
        }
    }

    pub(super) fn has_stream(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.driver == Driver::Stream)
    }

    pub(super) fn release_pointer(
        &mut self,
        target: Entity,
        offset: (Fixed, Fixed),
        bounds: ScrollBounds,
        now: u32,
    ) {
        if self.target_entity == Some(target)
            && self
                .session
                .as_ref()
                .is_some_and(|session| session.driver == Driver::Pointer)
        {
            self.release(offset, bounds, now);
        }
    }

    fn release(&mut self, offset: (Fixed, Fixed), bounds: ScrollBounds, now: u32) {
        let Some(session) = self.session.take() else {
            return;
        };
        let velocity = match session.driver {
            Driver::Pointer => session.velocity.velocity(now),
            Driver::Stream => session.velocity.last_velocity(),
        }
        .unwrap_or_default();
        let axis = |position: Fixed, velocity: Fixed, max: Fixed| {
            let edge = position.clamp(Fixed::ZERO, max);
            if position != edge {
                return Some(ScrollAnimation::new(position, edge, velocity));
            }
            if session.driver == Driver::Stream || velocity == Fixed::ZERO {
                return None;
            }
            let target = project_target(position, velocity, max);
            Some(ScrollAnimation::new(position, target, velocity))
        };
        self.x = axis(offset.0, velocity.0, bounds.max_x);
        self.y = axis(offset.1, velocity.1, bounds.max_y);
        if self.x.is_none() && self.y.is_none() {
            self.target_entity = None;
        }
    }

    pub(super) fn tick(
        &mut self,
        offset: (Fixed, Fixed),
        dt: u16,
        bounds: ScrollBounds,
        elastic: bool,
        now: u32,
    ) -> (Fixed, Fixed) {
        if let Some(session) = &self.session {
            if session.driver == Driver::Pointer || session.velocity.is_recent(now) {
                return if elastic {
                    offset
                } else {
                    displace(offset, (Fixed::ZERO, Fixed::ZERO), bounds, false)
                };
            }
            self.release(offset, bounds, now);
        }
        let axis = |animation: &mut Option<ScrollAnimation>, position: Fixed, max: Fixed| {
            let Some(motion) = animation else {
                return if elastic {
                    position
                } else {
                    position.clamp(Fixed::ZERO, max)
                };
            };
            let next = motion.tick(dt, max, elastic);
            if motion.is_settled() {
                *animation = None;
            }
            next
        };
        let next = (
            axis(&mut self.x, offset.0, bounds.max_x),
            axis(&mut self.y, offset.1, bounds.max_y),
        );
        if self.x.is_none() && self.y.is_none() {
            self.target_entity = None;
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: Entity = Entity {
        id: 1,
        generation: 0,
    };

    fn bounds() -> ScrollBounds {
        ScrollBounds {
            viewport_width: 100.into(),
            viewport_height: 100.into(),
            max_x: 200.into(),
            max_y: 200.into(),
            ..ScrollBounds::default()
        }
    }

    fn feed(
        motion: &mut ScrollMotion,
        offset: (Fixed, Fixed),
        delta: (Fixed, Fixed),
        driver: Driver,
        now: u32,
    ) -> (Fixed, Fixed) {
        let next = displace(offset, delta, bounds(), true);
        motion.record(TARGET, (next.0 - offset.0, next.1 - offset.1), driver, now);
        next
    }

    #[test]
    fn equal_content_motion_has_equal_release_trajectories_for_both_drivers() {
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for horizontal in [false, true] {
                    for direction in [-1, 1] {
                        for approach in [0, 24] {
                            let initial = if direction < 0 {
                                approach
                            } else {
                                200 - approach
                            };
                            let mut pointer = ScrollMotion::default();
                            let mut stream = ScrollMotion::default();
                            pointer.begin_pointer(TARGET, 0);
                            let mut p = (Fixed::from_int(initial), Fixed::from_int(initial));
                            let mut s = p;
                            let delta = if horizontal {
                                (Fixed::from_int(8 * direction), Fixed::ZERO)
                            } else {
                                (Fixed::ZERO, Fixed::from_int(8 * direction))
                            };
                            for now in [8, 16, 24, 32, 40, 48] {
                                p = feed(&mut pointer, p, delta, Driver::Pointer, now);
                                s = feed(&mut stream, s, delta, Driver::Stream, now);
                                assert_eq!(p, s);
                            }
                            pointer.release(p, bounds(), 48);
                            stream.release(s, bounds(), 80);
                            let (p_axis, s_axis) = if horizontal {
                                (pointer.x.as_ref(), stream.x.as_ref())
                            } else {
                                (pointer.y.as_ref(), stream.y.as_ref())
                            };
                            assert_eq!(p_axis.unwrap().velocity(), s_axis.unwrap().velocity());
                            assert_ne!(p_axis.unwrap().velocity(), Fixed::ZERO);
                            for frame in 0..400 {
                                p = pointer.tick(p, 8, bounds(), true, 56 + frame * 8);
                                s = stream.tick(s, 8, bounds(), true, 88 + frame * 8);
                                assert_eq!(p, s);
                            }
                            assert!(pointer.target_entity.is_none());
                            assert!(stream.target_entity.is_none());
                        }
                    }
                }
            }),
            0
        );
    }

    #[test]
    fn continuous_input_remains_in_control_through_outward_and_reverse_motion() {
        for driver in [Driver::Pointer, Driver::Stream] {
            let mut motion = ScrollMotion::default();
            if driver == Driver::Pointer {
                motion.begin_pointer(TARGET, 0);
            }
            let mut offset = (Fixed::ZERO, Fixed::ZERO);
            for sample in 1..65 {
                let direction = if sample <= 32 { -1 } else { 1 };
                let delta = (Fixed::ZERO, Fixed::from_ratio(direction, 4));
                let next = feed(&mut motion, offset, delta, driver, sample * 8);
                assert!((next.1 - offset.1) * direction > Fixed::ZERO);
                assert!(motion.x.is_none());
                assert!(motion.y.is_none());
                assert_eq!(motion.tick(next, 8, bounds(), true, sample * 8 + 7), next);
                offset = next;
            }
        }
    }

    #[test]
    fn a_gap_in_event_delivery_does_not_release_an_active_stream() {
        let mut motion = ScrollMotion::default();
        let mut offset = (Fixed::ZERO, Fixed::ZERO);
        for now in [0, 8, 16, 24] {
            offset = feed(
                &mut motion,
                offset,
                (Fixed::ZERO, (-4).into()),
                Driver::Stream,
                now,
            );
        }
        assert_eq!(motion.tick(offset, 16, bounds(), true, 40), offset);
        offset = feed(
            &mut motion,
            offset,
            (Fixed::ZERO, (-4).into()),
            Driver::Stream,
            48,
        );
        assert_eq!(motion.tick(offset, 16, bounds(), true, 64), offset);
        assert!(motion.has_stream());
        motion.tick(offset, 16, bounds(), true, 128);
        assert!(!motion.has_stream());
        assert!(motion.y.is_some());
    }

    #[test]
    fn new_input_takes_over_at_the_current_animated_position() {
        for driver in [Driver::Pointer, Driver::Stream] {
            let mut motion = ScrollMotion::default();
            if driver == Driver::Pointer {
                motion.begin_pointer(TARGET, 0);
            }
            let mut offset = (Fixed::ZERO, Fixed::ZERO);
            for now in [8, 16, 24, 32, 40, 48] {
                offset = feed(&mut motion, offset, (Fixed::ZERO, (-8).into()), driver, now);
            }
            motion.release(offset, bounds(), 48);
            offset = motion.tick(offset, 8, bounds(), true, 56);
            assert!(motion.y.is_some());
            let next = feed(&mut motion, offset, (Fixed::ZERO, 4.into()), driver, 64);
            assert_eq!(next, (offset.0, offset.1 + Fixed::from_int(4)));
            assert!(motion.y.is_none());
            assert_eq!(motion.tick(next, 8, bounds(), true, 65), next);
            let next = feed(&mut motion, next, (Fixed::ZERO, 4.into()), driver, 72);
            motion.release(next, bounds(), 72);
            assert!(motion.y.unwrap().velocity() > Fixed::ZERO);
        }
    }

    #[test]
    fn disabling_elastic_clamps_active_input_and_released_motion_to_new_bounds() {
        for driver in [Driver::Pointer, Driver::Stream] {
            for released in [false, true] {
                let mut motion = ScrollMotion::default();
                if driver == Driver::Pointer {
                    motion.begin_pointer(TARGET, 0);
                }
                let mut offset = (200.into(), 200.into());
                for now in [8, 16, 24, 32, 40, 48] {
                    offset = feed(&mut motion, offset, (8.into(), 8.into()), driver, now);
                }
                if released {
                    motion.release(offset, bounds(), 48);
                }
                for extent in [100, 0] {
                    let bounds = ScrollBounds {
                        max_x: extent.into(),
                        max_y: extent.into(),
                        ..bounds()
                    };
                    offset = motion.tick(offset, 8, bounds, false, 49);
                    assert_eq!(offset, (extent.into(), extent.into()));
                }
            }
        }
    }
}
