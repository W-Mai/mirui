pub mod ease;

use crate::ecs::{DeltaTimeMs, MonoClock, World};
use crate::types::{Fixed, Fixed64};

pub use ease::EaseFn;

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
        let t = Fixed::from_int(self.elapsed_ms as i32) / Fixed::from_int(self.duration_ms as i32);
        let eased = (self.ease)(t);
        self.from + eased * (self.to - self.from)
    }

    pub fn is_finished(&self) -> bool {
        self.mode == PlayMode::Once && self.elapsed_ms >= self.duration_ms
    }
}

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
        Self::preset(
            from,
            to,
            SpringConfig {
                duration_ms,
                bounce,
            },
        )
    }

    pub fn preset(from: Fixed, to: Fixed, config: SpringConfig) -> Self {
        let (stiffness, damping) = config_to_params(config);
        Self {
            stiffness,
            damping,
            position: from,
            velocity: Fixed::ZERO,
            target: to,
            origin: from,
            mode: SpringMode::Once,
            duration_hint_ms: config.duration_ms,
        }
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

    /// Idempotent on `target` — same value is a no-op so per-frame
    /// callers (e.g. boundary rebound) don't collapse `is_settled`'s
    /// span by resetting `origin = position` every tick. Config still
    /// applies regardless.
    pub fn retarget(&mut self, target: Fixed, config: Option<SpringConfig>) {
        if self.target != target {
            self.origin = self.position;
            self.target = target;
        }
        if let Some(c) = config {
            let (s, d) = config_to_params(c);
            self.stiffness = s;
            self.damping = d;
            self.duration_hint_ms = c.duration_ms;
        }
    }

    pub fn tick(&mut self, dt_ms: u16) {
        // Stability bound for semi-implicit Euler: ω₀·dt < 2 (ω₀ = √stiffness).
        // Pick N substeps so each sub_dt × ω₀ stays well below 1.
        let omega = self.stiffness.sqrt();
        let substep_count = ((dt_ms as u32 * omega.to_int().max(1) as u32) / 80).clamp(1, 32);

        let dt_total = Fixed64::from_fixed(Fixed::from_int(dt_ms as i32))
            / Fixed64::from_fixed(Fixed::from_int(1000));
        let sub_dt = dt_total / Fixed64::from_fixed(Fixed::from_int(substep_count as i32));

        let stiff = Fixed64::from_fixed(self.stiffness);
        let damp = Fixed64::from_fixed(self.damping);
        let target = Fixed64::from_fixed(self.target);
        let mut pos = Fixed64::from_fixed(self.position);
        let mut vel = Fixed64::from_fixed(self.velocity);

        for _ in 0..substep_count {
            let accel = (target - pos) * stiff - vel * damp;
            vel += accel * sub_dt;
            pos += vel * sub_dt;
        }

        self.position = pos.to_fixed();
        self.velocity = vel.to_fixed();

        if self.mode == SpringMode::Repeat && self.is_settled() {
            core::mem::swap(&mut self.origin, &mut self.target);
        }
    }

    pub fn value(&self) -> Fixed {
        if self.is_settled() {
            self.target
        } else {
            self.position
        }
    }

    pub fn is_settled(&self) -> bool {
        // 1.0 floor: from==to needs nonzero eps; unit amplitude reproduces
        // the old absolute-pixel behaviour.
        let span = (self.target - self.origin).abs().max(Fixed::ONE);
        let dist_eps = span / Fixed::from_int(200);
        let speed_eps = span * Fixed::from_int(2);
        (self.target - self.position).abs() < dist_eps && self.velocity.abs() < speed_eps
    }

    pub fn perceptual_duration(&self) -> u16 {
        self.duration_hint_ms
    }
}

/// WWDC23 formulas (mass = 1):
///   stiffness = (2π / duration_s)²
///   damping (bounce ≥ 0) = 4π · (1 - bounce) / duration_s
///   damping (bounce < 0) = 4π / (duration_s + 4π · |bounce|)
fn config_to_params(config: SpringConfig) -> (Fixed, Fixed) {
    let dur_s = Fixed64::from_fixed(Fixed::from_int(config.duration_ms.max(1) as i32))
        / Fixed64::from_fixed(Fixed::from_int(1000));
    let two_pi = Fixed64::from_fixed(Fixed::PI) * Fixed64::from_int(2);
    let four_pi = two_pi + two_pi;
    let bounce = Fixed64::from_fixed(config.bounce);

    let omega = two_pi / dur_s;
    let stiffness = omega * omega;

    let damping = if config.bounce >= Fixed::ZERO {
        four_pi * (Fixed64::ONE - bounce) / dur_s
    } else {
        let abs_bounce = Fixed64::ZERO - bounce;
        four_pi / (dur_s + four_pi * abs_bounce)
    };

    (stiffness.to_fixed(), damping.to_fixed())
}

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
    fn spring_200ms_settles_quickly() {
        let mut s = Spring::new(Fixed::ZERO, Fixed::from_int(14), 200, Fixed::ZERO);
        let mut elapsed = 0u32;
        while !s.is_settled() && elapsed < 1000 {
            s.tick(16);
            elapsed += 16;
        }
        assert!(
            elapsed < 350,
            "200ms spring should settle in < 350ms, took {elapsed}ms"
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

#[cfg(test)]
mod tween_zero_one_check {
    use super::*;

    #[test]
    fn tween_0_to_1_yields_intermediate_values() {
        let mut t = Tween::ease_to(Fixed::ZERO, Fixed::ONE, 250);
        let mut samples = alloc::vec::Vec::new();
        for _ in 0..16 {
            samples.push(t.value().to_f32());
            t.tick(16);
        }
        let unique: alloc::collections::BTreeSet<_> =
            samples.iter().map(|f| (f * 1000.0) as i32).collect();
        assert!(
            unique.len() >= 8,
            "expected >=8 unique frames, got {} samples: {:?}",
            unique.len(),
            samples
        );
        assert!(samples.first().unwrap().abs() < 0.05, "should start near 0");
        assert!(
            (samples.last().unwrap() - 1.0).abs() < 0.05,
            "should end near 1"
        );
    }
}

#[cfg(test)]
mod settle_threshold_check {
    use super::*;

    #[test]
    fn spring_on_normalized_range_produces_intermediate_frames() {
        let mut s = Spring::new(Fixed::ZERO, Fixed::ONE, 250, Fixed::ZERO);
        let mut samples = alloc::vec::Vec::new();
        for _ in 0..30 {
            samples.push(s.value().to_f32());
            s.tick(16);
        }
        let unique: alloc::collections::BTreeSet<_> =
            samples.iter().map(|f| (f * 1000.0) as i32).collect();
        assert!(
            unique.len() >= 8,
            "expected >=8 distinct frames, got {}: {:?}",
            unique.len(),
            samples
        );
    }

    #[test]
    fn spring_zero_amplitude_settles_immediately() {
        let s = Spring::new(Fixed::from_int(50), Fixed::from_int(50), 250, Fixed::ZERO);
        assert!(s.is_settled());
        assert_eq!(s.value(), Fixed::from_int(50));
    }

    #[test]
    fn retarget_resets_origin_to_current_position() {
        let mut s = Spring::new(Fixed::ZERO, Fixed::from_int(100), 300, Fixed::ZERO);
        for _ in 0..6 {
            s.tick(16);
        }
        let mid = s.position;
        assert!(mid > Fixed::ZERO && mid < Fixed::from_int(100));
        s.retarget(Fixed::from_int(20), None);
        assert_eq!(s.origin, mid);
        for _ in 0..200 {
            s.tick(16);
        }
        assert!(s.is_settled());
        assert!((s.position - Fixed::from_int(20)).abs() < Fixed::ONE);
    }

    #[test]
    fn retarget_with_same_target_is_idempotent() {
        let mut s = Spring::new(Fixed::ZERO, Fixed::from_int(100), 300, Fixed::ZERO);
        for _ in 0..6 {
            s.tick(16);
        }
        let origin_before = s.origin;
        let position_before = s.position;
        let velocity_before = s.velocity;
        s.retarget(Fixed::from_int(100), None);
        assert_eq!(s.origin, origin_before);
        assert_eq!(s.position, position_before);
        assert_eq!(s.velocity, velocity_before);
        assert_eq!(s.target, Fixed::from_int(100));
    }

    #[test]
    fn boundary_rebound_settles_under_repeated_retarget() {
        let mut s = Spring::preset(
            Fixed::from_int(-10),
            Fixed::from_int(-50),
            crate::anim::SMOOTH,
        )
        .with_velocity(Fixed::from_int(-5));
        let mut elapsed = 0u32;
        while !s.is_settled() && elapsed < 5000 {
            s.tick(16);
            if s.position < Fixed::ZERO {
                s.retarget(Fixed::ZERO, Some(crate::anim::SMOOTH));
            }
            elapsed += 16;
        }
        assert!(s.is_settled(), "spring stalled at position={}", s.position);
        assert!(
            (s.position - Fixed::ZERO).abs() < Fixed::from_int(2),
            "settled too far from zero: position={}",
            s.position
        );
    }

    /// Stress: 1000 randomised (from, to, duration, bounce) springs
    /// must converge near the target within 3 × duration without
    /// going NaN or oscillating unboundedly. Catches integration
    /// blow-ups (sub-step too coarse for ω·dt < 2) and any future
    /// stiffness/damping table regression. Only excludes the
    /// documented unstable bounce ≥ 0.8 region.
    ///
    /// "Convergence" here is a physics check, not the is_settled()
    /// API: |position − target| ≤ 5% of |target − origin| with at
    /// least Fixed::ONE floor (so unit spans don't zero-out the
    /// tolerance). Tighter than is_settled()'s velocity check on
    /// purpose — we want to catch divergence, not API thresholds.
    #[test]
    fn spring_settle_stress_1000() {
        let mut seed: u32 = 0x9e3779b9;
        let mut rand = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };

        let mut failures: alloc::vec::Vec<(usize, i32, i32, u16, i32, i32)> =
            alloc::vec::Vec::new();
        for i in 0..1000 {
            let from = ((rand() as i32) % 1000) - 500;
            let to = {
                let mut t = ((rand() as i32) % 1000) - 500;
                if t == from {
                    t += 1;
                }
                t
            };
            let duration = 100u16 + ((rand() % 1901) as u16);
            // bounce ∈ [0, 0.7] — bounce ≥ 0.8 known to ring forever.
            let bounce_raw = (rand() % 180) as i32;
            let bounce = Fixed::from_raw(bounce_raw);

            let mut sp = Spring::new(Fixed::from_int(from), Fixed::from_int(to), duration, bounce);
            let budget_ms = (duration as u32 * 3).max(500);
            let mut elapsed = 0u32;
            while elapsed < budget_ms {
                sp.tick(16);
                elapsed += 16;
            }

            let span = (to - from).abs().max(1);
            let tol = (span / 20).max(1);
            let err = (sp.position.to_int() - to).abs();
            if err > tol {
                failures.push((i, from, to, duration, bounce_raw, sp.position.to_int()));
            }
        }
        assert!(
            failures.is_empty(),
            "{}/1000 springs diverged: first 5 = {:?}",
            failures.len(),
            &failures[..failures.len().min(5)],
        );
    }
}
