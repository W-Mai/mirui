pub mod ease;

use crate::ecs::{DeltaTimeMs, World};
use crate::types::Fixed;

pub use ease::{EaseCurve, EaseFn};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    Once,
    Loop,
    PingPong,
}

#[derive(Clone, Copy)]
pub struct Animation {
    pub from: Fixed,
    pub to: Fixed,
    pub duration_ms: u16,
    pub elapsed_ms: u16,
    pub ease: EaseFn,
    pub mode: PlayMode,
    pub spatial: Option<SpatialState>,
}

#[derive(Clone, Copy)]
pub struct SpatialState {
    pub step: Fixed,
    pub last_output: Fixed,
}

impl Animation {
    pub fn new(from: Fixed, to: Fixed, duration_ms: u16, ease: EaseFn, mode: PlayMode) -> Self {
        Self {
            from,
            to,
            duration_ms: duration_ms.max(1),
            elapsed_ms: 0,
            ease,
            mode,
            spatial: None,
        }
    }

    /// Spatial-uniform mode: output advances in fixed pixel steps.
    /// `steps` is the number of equal increments over the full range.
    /// Ease shape is preserved (fast sections update more frequently,
    /// slow sections update less frequently).
    pub fn spatial(
        from: Fixed,
        to: Fixed,
        duration_ms: u16,
        ease: EaseFn,
        mode: PlayMode,
        steps: u16,
    ) -> Self {
        let range = to - from;
        let step = if steps > 0 {
            range / Fixed::from_int(steps as i32)
        } else {
            Fixed::ONE
        };
        Self {
            from,
            to,
            duration_ms: duration_ms.max(1),
            elapsed_ms: 0,
            ease,
            mode,
            spatial: Some(SpatialState {
                step,
                last_output: from,
            }),
        }
    }

    pub fn ease_to(from: Fixed, to: Fixed, duration_ms: u16) -> Self {
        Self::new(from, to, duration_ms, ease::ease_out_quad, PlayMode::Once)
    }

    pub fn tick(&mut self, dt_ms: u16) {
        if self.is_finished() {
            return;
        }
        self.elapsed_ms = self.elapsed_ms.saturating_add(dt_ms);
        if self.elapsed_ms >= self.duration_ms {
            match self.mode {
                PlayMode::Once => self.elapsed_ms = self.duration_ms,
                PlayMode::Loop => {
                    self.elapsed_ms %= self.duration_ms;
                    if let Some(ref mut sp) = self.spatial {
                        sp.last_output = self.from;
                    }
                }
                PlayMode::PingPong => {
                    self.elapsed_ms %= self.duration_ms;
                    core::mem::swap(&mut self.from, &mut self.to);
                    if let Some(ref mut sp) = self.spatial {
                        sp.step = Fixed::ZERO - sp.step;
                        sp.last_output = self.from;
                    }
                }
            }
        }

        if let Some(ref mut sp) = self.spatial {
            let t = Fixed::from_raw(
                (self.elapsed_ms as i32) * Fixed::ONE.raw() / (self.duration_ms as i32),
            );
            let eased = (self.ease)(t);
            let raw_value = self.from + eased * (self.to - self.from);
            let diff = raw_value - sp.last_output;
            let abs_step = sp.step.abs();
            if abs_step.raw() > 0 && diff.abs() >= abs_step {
                let n = diff.raw() / sp.step.raw();
                sp.last_output += Fixed::from_raw(n * sp.step.raw());
            }
        }
    }

    pub fn current_value(&self) -> Fixed {
        if let Some(ref sp) = self.spatial {
            sp.last_output
        } else {
            let t = Fixed::from_raw(
                (self.elapsed_ms as i32) * Fixed::ONE.raw() / (self.duration_ms as i32),
            );
            let eased = (self.ease)(t);
            self.from + eased * (self.to - self.from)
        }
    }

    pub fn is_finished(&self) -> bool {
        self.mode == PlayMode::Once && self.elapsed_ms >= self.duration_ms
    }
}

pub trait AnimationComponent {
    fn animation(&self) -> &Animation;
    fn animation_mut(&mut self) -> &mut Animation;
}

use crate::ecs::MonoClock;

pub fn sync_delta_time_ms(world: &mut World) {
    let ms = match world.resource_mut::<MonoClock>() {
        Some(fc) => {
            let now = (fc.clock)();
            let dt_ns = now.saturating_sub(fc.last_ns);
            fc.last_ns = now;
            (dt_ns / 1_000_000).clamp(1, 65535) as u16
        }
        None => 16,
    };
    world.insert_resource(DeltaTimeMs(ms));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_animation_progresses() {
        let mut a = Animation::new(
            Fixed::ZERO,
            Fixed::from_int(100),
            100,
            ease::linear,
            PlayMode::Once,
        );
        a.tick(50);
        let val = a.current_value();
        assert!((val.to_int() - 50).abs() <= 1);
        assert!(!a.is_finished());
    }

    #[test]
    fn animation_finishes_at_duration() {
        let mut a = Animation::ease_to(Fixed::ZERO, Fixed::from_int(10), 200);
        a.tick(200);
        assert!(a.is_finished());
        assert_eq!(a.current_value(), Fixed::from_int(10));
    }

    #[test]
    fn loop_wraps_around() {
        let mut a = Animation::new(
            Fixed::ZERO,
            Fixed::from_int(100),
            100,
            ease::linear,
            PlayMode::Loop,
        );
        a.tick(150);
        assert!(!a.is_finished());
        assert_eq!(a.elapsed_ms, 50);
    }

    #[test]
    fn pingpong_reverses() {
        let mut a = Animation::new(
            Fixed::ZERO,
            Fixed::from_int(100),
            100,
            ease::linear,
            PlayMode::PingPong,
        );
        a.tick(100);
        assert_eq!(a.from, Fixed::from_int(100));
        assert_eq!(a.to, Fixed::ZERO);
    }

    #[test]
    fn spatial_reaches_end() {
        let mut a = Animation::spatial(
            Fixed::ZERO,
            Fixed::from_int(100),
            1000,
            ease::ease_in_out_cubic,
            PlayMode::Once,
            20,
        );
        for _ in 0..100 {
            a.tick(16);
        }
        assert!(a.is_finished());
        assert_eq!(a.current_value().to_int(), 100);
    }

    #[test]
    fn spatial_same_duration_as_temporal() {
        let mut temporal = Animation::new(
            Fixed::ZERO,
            Fixed::from_int(100),
            1000,
            ease::ease_in_out_cubic,
            PlayMode::Once,
        );
        let mut spatial = Animation::spatial(
            Fixed::ZERO,
            Fixed::from_int(100),
            1000,
            ease::ease_in_out_cubic,
            PlayMode::Once,
            20,
        );

        let mut tf = 0u32;
        while !temporal.is_finished() && tf < 200 {
            temporal.tick(16);
            tf += 1;
        }
        let mut sf = 0u32;
        while !spatial.is_finished() && sf < 200 {
            spatial.tick(16);
            sf += 1;
        }
        assert_eq!(tf, sf);
    }

    #[test]
    fn spatial_all_steps_equal() {
        let mut a = Animation::spatial(
            Fixed::ZERO,
            Fixed::from_int(100),
            500,
            ease::ease_in_out_cubic,
            PlayMode::Once,
            20,
        );

        let mut prev_val = Fixed::ZERO;
        let mut deltas = alloc::vec::Vec::new();

        for _ in 0..40 {
            a.tick(16);
            let val = a.current_value();
            let d = val - prev_val;
            if d.raw() != 0 {
                deltas.push(d.raw());
            }
            prev_val = val;
        }

        let expected_step = Fixed::from_int(100).raw() / 20; // 5px in raw = 1280
        for &d in &deltas {
            assert_eq!(
                d % expected_step,
                0,
                "every step should be a multiple of {expected_step} raw, got {d}"
            );
        }
    }
}

pub fn run_animation<T: AnimationComponent + 'static>(
    world: &mut World,
    apply: fn(&mut World, crate::ecs::Entity, Fixed),
) {
    let dt = world.resource::<DeltaTimeMs>().map_or(16, |r| r.0);

    let mut entities = alloc::vec::Vec::new();
    world.query::<T>().collect_into(&mut entities);

    for e in entities {
        let (val, finished) = {
            let Some(comp) = world.get_mut::<T>(e) else {
                continue;
            };
            comp.animation_mut().tick(dt);
            (
                comp.animation().current_value(),
                comp.animation().is_finished(),
            )
        };
        apply(world, e, val);
        if finished {
            world.remove::<T>(e);
        }
    }
}
