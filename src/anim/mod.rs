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
    spatial: Option<SpatialState>,
}

#[derive(Clone, Copy)]
struct SpatialState {
    t: Fixed,
    curve: EaseCurve,
    ds: Fixed,
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

    pub fn spatial(
        from: Fixed,
        to: Fixed,
        duration_ms: u16,
        curve: EaseCurve,
        mode: PlayMode,
    ) -> Self {
        Self {
            from,
            to,
            duration_ms: duration_ms.max(1),
            elapsed_ms: 0,
            ease: curve.eval,
            mode,
            spatial: Some(SpatialState {
                t: Fixed::ZERO,
                curve,
                ds: Fixed::ZERO,
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
        if let Some(ref mut sp) = self.spatial {
            if sp.ds == Fixed::ZERO {
                sp.ds = sp.curve.arc_length * Fixed::from_raw(dt_ms as i32)
                    / Fixed::from_raw(self.duration_ms as i32);
            }
            let deriv = (sp.curve.derivative)(sp.t);
            let norm_sq = Fixed::ONE + deriv * deriv;
            let inv_norm = norm_sq.rsqrt();
            let delta_t = sp.ds * inv_norm;
            sp.t += delta_t;
            if sp.t >= Fixed::ONE {
                match self.mode {
                    PlayMode::Once => {
                        sp.t = Fixed::ONE;
                        self.elapsed_ms = self.duration_ms;
                    }
                    PlayMode::Loop => {
                        sp.t -= Fixed::ONE;
                    }
                    PlayMode::PingPong => {
                        sp.t = Fixed::ONE - (sp.t - Fixed::ONE);
                        core::mem::swap(&mut self.from, &mut self.to);
                    }
                }
            } else {
                self.elapsed_ms = (sp.t * Fixed::from_raw(self.duration_ms as i32)).to_int() as u16;
            }
        } else {
            self.elapsed_ms = self.elapsed_ms.saturating_add(dt_ms);
            if self.elapsed_ms >= self.duration_ms {
                match self.mode {
                    PlayMode::Once => self.elapsed_ms = self.duration_ms,
                    PlayMode::Loop => self.elapsed_ms %= self.duration_ms,
                    PlayMode::PingPong => {
                        self.elapsed_ms %= self.duration_ms;
                        core::mem::swap(&mut self.from, &mut self.to);
                    }
                }
            }
        }
    }

    pub fn current_value(&self) -> Fixed {
        let t = if let Some(ref sp) = self.spatial {
            sp.t
        } else {
            Fixed::from_raw((self.elapsed_ms as i32) * Fixed::ONE.raw() / (self.duration_ms as i32))
        };
        let eased = (self.ease)(t);
        self.from + eased * (self.to - self.from)
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
            ease::IN_OUT_CUBIC,
            PlayMode::Once,
        );
        for _ in 0..100 {
            a.tick(16);
        }
        assert!(a.is_finished());
        assert_eq!(a.current_value().to_int(), 100);
    }

    #[test]
    fn spatial_uniform_arc_steps() {
        let mut a = Animation::spatial(
            Fixed::ZERO,
            Fixed::from_int(100),
            500,
            ease::IN_OUT_CUBIC,
            PlayMode::Once,
        );

        let mut prev_t = Fixed::ZERO;
        let mut prev_val = Fixed::ZERO;
        let mut arc_lengths = alloc::vec::Vec::new();

        for _ in 0..25 {
            a.tick(16);
            let sp = a.spatial.unwrap();
            let t = sp.t;
            let val = a.current_value();
            let dt = (t - prev_t).to_f32();
            let dv = (val - prev_val).to_f32() / 100.0;
            let arc = (dt * dt + dv * dv).sqrt();
            if arc > 0.0001 {
                arc_lengths.push(arc);
            }
            prev_t = t;
            prev_val = val;
        }

        if arc_lengths.len() >= 5 {
            let max = arc_lengths.iter().copied().fold(0.0f32, f32::max);
            let min = arc_lengths.iter().copied().fold(f32::MAX, f32::min);
            let ratio = max / min;
            assert!(
                ratio < 4.0,
                "arc steps should be roughly uniform, got ratio={ratio:.2} (max={max:.4}, min={min:.4})"
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
