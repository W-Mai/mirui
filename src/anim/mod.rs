pub mod ease;

use crate::ecs::{DeltaTimeMs, World};
use crate::types::Fixed;

pub use ease::EaseFn;

// ─── Tween (deterministic duration + ease curve) ───────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    Once,
    Loop,
    PingPong,
}

#[derive(Clone, Copy)]
pub struct Tween {
    pub from: Fixed,
    pub to: Fixed,
    pub duration_ms: u16,
    pub elapsed_ms: u16,
    pub ease: EaseFn,
    pub mode: PlayMode,
}

impl Tween {
    pub fn new(from: Fixed, to: Fixed, duration_ms: u16, ease: EaseFn, mode: PlayMode) -> Self {
        Self {
            from,
            to,
            duration_ms: duration_ms.max(1),
            elapsed_ms: 0,
            ease,
            mode,
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
                PlayMode::Loop => self.elapsed_ms %= self.duration_ms,
                PlayMode::PingPong => {
                    self.elapsed_ms %= self.duration_ms;
                    core::mem::swap(&mut self.from, &mut self.to);
                }
            }
        }
    }

    pub fn value(&self) -> Fixed {
        let t = Fixed::from_raw(
            (self.elapsed_ms as i32) * Fixed::ONE.raw() / (self.duration_ms as i32),
        );
        let eased = (self.ease)(t);
        self.from + eased * (self.to - self.from)
    }

    pub fn is_finished(&self) -> bool {
        self.mode == PlayMode::Once && self.elapsed_ms >= self.duration_ms
    }
}

// ─── Spring (physical spring, Apple-style duration/bounce) ─────────────

#[derive(Clone, Copy)]
pub struct SpringConfig {
    pub duration_ms: u16,
    pub bounce: Fixed,
}

pub const SMOOTH: SpringConfig = SpringConfig {
    duration_ms: 500,
    bounce: Fixed::ZERO,
};
pub const SNAPPY: SpringConfig = SpringConfig {
    duration_ms: 300,
    bounce: Fixed::from_raw(38),
};
pub const BOUNCY: SpringConfig = SpringConfig {
    duration_ms: 500,
    bounce: Fixed::from_raw(77),
};
pub const INTERACTIVE: SpringConfig = SpringConfig {
    duration_ms: 200,
    bounce: Fixed::ZERO,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpringMode {
    Once,
    Repeat,
}

#[derive(Clone, Copy)]
pub struct Spring {
    stiffness: Fixed,
    damping: Fixed,
    pub position: Fixed,
    pub velocity: Fixed,
    pub target: Fixed,
    origin: Fixed,
    mode: SpringMode,
    duration_hint_ms: u16,
}

impl Spring {
    pub fn new(from: Fixed, to: Fixed, duration_ms: u16, bounce: Fixed) -> Self {
        let config = SpringConfig {
            duration_ms,
            bounce,
        };
        let (stiffness, damping) = config_to_params(config);
        Self {
            stiffness,
            damping,
            position: from,
            velocity: Fixed::ZERO,
            target: to,
            origin: from,
            mode: SpringMode::Once,
            duration_hint_ms: duration_ms,
        }
    }

    pub fn preset(from: Fixed, to: Fixed, config: SpringConfig) -> Self {
        Self::new(from, to, config.duration_ms, config.bounce)
    }

    pub fn with_params(from: Fixed, to: Fixed, stiffness: Fixed, damping: Fixed) -> Self {
        Self {
            stiffness,
            damping,
            position: from,
            velocity: Fixed::ZERO,
            target: to,
            origin: from,
            mode: SpringMode::Once,
            duration_hint_ms: 500,
        }
    }

    pub fn with_velocity(mut self, velocity: Fixed) -> Self {
        self.velocity = velocity;
        self
    }

    pub fn repeat(mut self) -> Self {
        self.mode = SpringMode::Repeat;
        self
    }

    pub fn set_velocity(&mut self, velocity: Fixed) {
        self.velocity = velocity;
    }

    pub fn retarget(&mut self, target: Fixed, config: Option<SpringConfig>) {
        self.target = target;
        if let Some(c) = config {
            let (s, d) = config_to_params(c);
            self.stiffness = s;
            self.damping = d;
            self.duration_hint_ms = c.duration_ms;
        }
    }

    pub fn tick(&mut self, dt_ms: u16) {
        let dt = Fixed::from_raw((dt_ms as i32) * 256 / 1000);
        let displacement = self.target - self.position;
        let accel_raw = {
            let sf = (displacement.raw() as i64 * self.stiffness.raw() as i64) >> 8;
            let df = (self.velocity.raw() as i64 * self.damping.raw() as i64) >> 8;
            (sf - df).clamp(i32::MIN as i64, i32::MAX as i64) as i32
        };
        let accel = Fixed::from_raw(accel_raw);
        self.velocity += accel * dt;
        self.position += self.velocity * dt;

        if self.mode == SpringMode::Repeat && self.is_settled() {
            let new_target = if self.target == self.origin {
                self.origin + (self.origin - self.target)
            } else {
                self.origin
            };
            core::mem::swap(&mut self.origin, &mut self.target);
            self.target = new_target;
        }
    }

    pub fn value(&self) -> Fixed {
        self.position
    }

    pub fn is_settled(&self) -> bool {
        let dist = (self.target - self.position).abs();
        let speed = self.velocity.abs();
        dist < Fixed::ONE && speed < Fixed::from_int(2)
    }

    pub fn perceptual_duration(&self) -> u16 {
        self.duration_hint_ms
    }
}

fn config_to_params(config: SpringConfig) -> (Fixed, Fixed) {
    let dur_ms = config.duration_ms.max(1) as i64;
    let two_pi = 1608i64; // 2π in Q24.8 raw
    let stiffness_raw = (two_pi * two_pi * 1000 * 1000) / (dur_ms * dur_ms * 256);
    let damping_raw = if config.bounce.raw() >= 0 {
        let one_minus_bounce = 256i64 - config.bounce.raw() as i64;
        (4 * two_pi * one_minus_bounce) / (dur_ms * 256 / 1000)
    } else {
        let bounce_abs = (-config.bounce.raw()) as i64;
        let denom = dur_ms * 256 / 1000 + 4 * two_pi * bounce_abs / 256;
        (4 * two_pi * 256) / denom.max(1)
    };
    (
        Fixed::from_raw(stiffness_raw.clamp(1, i32::MAX as i64) as i32),
        Fixed::from_raw(damping_raw.clamp(1, i32::MAX as i64) as i32),
    )
}

// ─── Motion enum (unified interface) ───────────────────────────────────

#[derive(Clone, Copy)]
pub enum Motion {
    Tween(Tween),
    Spring(Spring),
}

impl Motion {
    pub fn tick(&mut self, dt_ms: u16) {
        match self {
            Self::Tween(t) => t.tick(dt_ms),
            Self::Spring(s) => s.tick(dt_ms),
        }
    }

    pub fn value(&self) -> Fixed {
        match self {
            Self::Tween(t) => t.value(),
            Self::Spring(s) => s.value(),
        }
    }

    pub fn is_done(&self) -> bool {
        match self {
            Self::Tween(t) => t.is_finished(),
            Self::Spring(s) => s.is_settled() && s.mode == SpringMode::Once,
        }
    }
}

impl From<Tween> for Motion {
    fn from(t: Tween) -> Self {
        Self::Tween(t)
    }
}

impl From<Spring> for Motion {
    fn from(s: Spring) -> Self {
        Self::Spring(s)
    }
}

// ─── System support ────────────────────────────────────────────────────

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

pub trait MotionComponent {
    fn motion(&self) -> &Motion;
    fn motion_mut(&mut self) -> &mut Motion;
}

pub fn run_motion<T: MotionComponent + 'static>(
    world: &mut World,
    apply: fn(&mut World, crate::ecs::Entity, Fixed),
) {
    let dt = world.resource::<DeltaTimeMs>().map_or(16, |r| r.0);

    let mut entities = alloc::vec::Vec::new();
    world.query::<T>().collect_into(&mut entities);

    for e in entities {
        let (val, done) = {
            let Some(comp) = world.get_mut::<T>(e) else {
                continue;
            };
            comp.motion_mut().tick(dt);
            (comp.motion().value(), comp.motion().is_done())
        };
        apply(world, e, val);
        if done {
            world.remove::<T>(e);
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tween_linear_progresses() {
        let mut t = Tween::new(
            Fixed::ZERO,
            Fixed::from_int(100),
            100,
            ease::linear,
            PlayMode::Once,
        );
        t.tick(50);
        assert!((t.value().to_int() - 50).abs() <= 1);
        assert!(!t.is_finished());
    }

    #[test]
    fn tween_finishes_at_duration() {
        let mut t = Tween::ease_to(Fixed::ZERO, Fixed::from_int(10), 200);
        t.tick(200);
        assert!(t.is_finished());
        assert_eq!(t.value(), Fixed::from_int(10));
    }

    #[test]
    fn tween_loop_wraps() {
        let mut t = Tween::new(
            Fixed::ZERO,
            Fixed::from_int(100),
            100,
            ease::linear,
            PlayMode::Loop,
        );
        t.tick(150);
        assert!(!t.is_finished());
        assert_eq!(t.elapsed_ms, 50);
    }

    #[test]
    fn tween_pingpong_reverses() {
        let mut t = Tween::new(
            Fixed::ZERO,
            Fixed::from_int(100),
            100,
            ease::linear,
            PlayMode::PingPong,
        );
        t.tick(100);
        assert_eq!(t.from, Fixed::from_int(100));
        assert_eq!(t.to, Fixed::ZERO);
    }

    #[test]
    fn spring_settles() {
        let mut s = Spring::new(Fixed::ZERO, Fixed::from_int(100), 500, Fixed::ZERO);
        for _ in 0..200 {
            s.tick(16);
        }
        assert!(s.is_settled());
        assert!((s.position.to_int() - 100).abs() <= 1);
    }

    #[test]
    fn spring_retarget_preserves_velocity() {
        let mut s = Spring::new(Fixed::ZERO, Fixed::from_int(100), 300, Fixed::ZERO);
        for _ in 0..10 {
            s.tick(16);
        }
        let vel_before = s.velocity;
        s.retarget(Fixed::from_int(50), None);
        assert_eq!(s.velocity, vel_before);
        assert_eq!(s.target, Fixed::from_int(50));
    }

    #[test]
    fn spring_with_bounce_overshoots() {
        let mut s = Spring::new(Fixed::ZERO, Fixed::from_int(100), 500, Fixed::from_raw(200));
        let mut max_pos = Fixed::ZERO;
        for _ in 0..200 {
            s.tick(16);
            if s.position > max_pos {
                max_pos = s.position;
            }
        }
        assert!(
            max_pos.to_int() > 100,
            "bouncy spring should overshoot: max={}",
            max_pos.to_int()
        );
    }

    #[test]
    fn motion_tween_works() {
        let mut m: Motion = Tween::ease_to(Fixed::ZERO, Fixed::from_int(50), 100).into();
        m.tick(100);
        assert!(m.is_done());
        assert_eq!(m.value().to_int(), 50);
    }

    #[test]
    fn motion_spring_works() {
        let mut m: Motion = Spring::preset(Fixed::ZERO, Fixed::from_int(80), SMOOTH).into();
        for _ in 0..200 {
            m.tick(16);
        }
        assert!(m.is_done());
        assert!((m.value().to_int() - 80).abs() <= 1);
    }
}
