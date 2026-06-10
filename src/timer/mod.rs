//! Declarative timers — `Timer` component fires a callback at scheduled
//! `MonoClock` instants. Reset drift mode (no catch-up). Lifecycle
//! variants cover one-shot, periodic, bounded counts, and bounded wall
//! deadlines. See `.local/specs/timer/design.md` for the full surface.

use crate::ecs::{Entity, MonoClock, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerMode {
    After,
    Every,
    Repeat { remaining: u32 },
    Until { deadline_ms: u32 },
}

/// `wrapping_sub` makes the schedule wrap-safe across MonoClock's
/// 49.7-day u32 epoch wrap as long as `period_ms < 2^31`.
#[derive(Clone)]
pub struct Timer {
    period_ms: u32,
    next_at_ms: u32,
    mode: TimerMode,
    /// Some(now_ms_at_pause); resume() consumes it to push next_at_ms forward.
    paused_at_ms: Option<u32>,
    callback: fn(&mut World, Entity),
}

impl Timer {
    pub fn after(period_ms: u32, cb: fn(&mut World, Entity)) -> Self {
        Self {
            period_ms,
            next_at_ms: 0, // patched on first system tick.
            mode: TimerMode::After,
            paused_at_ms: None,
            callback: cb,
        }
    }

    pub fn every(period_ms: u32, cb: fn(&mut World, Entity)) -> Self {
        Self {
            period_ms,
            next_at_ms: 0,
            mode: TimerMode::Every,
            paused_at_ms: None,
            callback: cb,
        }
    }

    /// Panics on `times == 0` — silent self-remove would be a bug.
    pub fn repeat(times: u32, period_ms: u32, cb: fn(&mut World, Entity)) -> Self {
        assert!(times > 0, "Timer::repeat needs times > 0");
        Self {
            period_ms,
            next_at_ms: 0,
            mode: TimerMode::Repeat { remaining: times },
            paused_at_ms: None,
            callback: cb,
        }
    }

    /// `deadline_ms` is absolute on the MonoClock timeline (typically `now + N`).
    pub fn until(deadline_ms: u32, period_ms: u32, cb: fn(&mut World, Entity)) -> Self {
        Self {
            period_ms,
            next_at_ms: 0,
            mode: TimerMode::Until { deadline_ms },
            paused_at_ms: None,
            callback: cb,
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused_at_ms.is_some()
    }

    pub fn mode(&self) -> TimerMode {
        self.mode
    }

    /// Idempotent.
    pub fn pause(&mut self, now_ms: u32) {
        if self.paused_at_ms.is_none() {
            self.paused_at_ms = Some(now_ms);
        }
    }

    /// Idempotent. Pushes next_at_ms back by the paused duration so the timer effectively slept.
    pub fn resume(&mut self, now_ms: u32) {
        if let Some(paused_at) = self.paused_at_ms.take() {
            let slept = now_ms.wrapping_sub(paused_at);
            self.next_at_ms = self.next_at_ms.wrapping_add(slept);
        }
    }
}

/// No-op when MonoClock is missing.
#[crate::system(order = TIMER)]
pub fn timer_system(world: &mut World) {
    let Some(now) = world.resource::<MonoClock>().map(|c| c.now_ms()) else {
        return;
    };

    let entities: alloc::vec::Vec<Entity> = world.query::<Timer>().collect();
    for entity in entities {
        let Some(t) = world.get::<Timer>(entity).cloned() else {
            continue;
        };
        if t.paused_at_ms.is_some() {
            continue;
        }

        // next_at_ms == 0 = uninitialised; ctor couldn't read MonoClock.
        if t.next_at_ms == 0 {
            let mut t2 = t.clone();
            t2.next_at_ms = now.wrapping_add(t2.period_ms);
            world.insert(entity, t2);
            continue;
        }

        let due = (now.wrapping_sub(t.next_at_ms) as i32) >= 0;
        if !due {
            continue;
        }

        // Callback first so user can cancel via remove::<Timer>; we re-read after.
        (t.callback)(world, entity);

        let Some(after_cb) = world.get::<Timer>(entity).cloned() else {
            continue;
        };
        let mut next = after_cb;
        match next.mode {
            TimerMode::After => {
                world.remove::<Timer>(entity);
                continue;
            }
            TimerMode::Every => {
                next.next_at_ms = now.wrapping_add(next.period_ms);
            }
            TimerMode::Repeat { remaining } => {
                let left = remaining - 1;
                if left == 0 {
                    world.remove::<Timer>(entity);
                    continue;
                }
                next.mode = TimerMode::Repeat { remaining: left };
                next.next_at_ms = now.wrapping_add(next.period_ms);
            }
            TimerMode::Until { deadline_ms } => {
                let candidate = now.wrapping_add(next.period_ms);
                let past = (candidate.wrapping_sub(deadline_ms) as i32) > 0;
                if past {
                    world.remove::<Timer>(entity);
                    continue;
                }
                next.next_at_ms = candidate;
            }
        }
        world.insert(entity, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::time::mock;
    use crate::ecs::{MonoClock, World};
    use core::sync::atomic::{AtomicU32, Ordering};

    static FIRE_COUNT: AtomicU32 = AtomicU32::new(0);
    fn count_cb(_world: &mut World, _entity: Entity) {
        FIRE_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    fn reset_count() {
        FIRE_COUNT.store(0, Ordering::SeqCst);
    }

    fn fresh_world() -> World {
        let mut w = World::new();
        w.insert_resource(MonoClock::new(mock::clock_fn));
        w
    }

    fn anchor(world: &mut World, now_ms_init: u32) {
        mock::set_ms(now_ms_init as u64);
        timer_system(world);
    }

    #[test]
    fn pause_freezes_then_resume_restores_remaining_period() {
        let _g = mock::lock();
        mock::set_ms(0);
        reset_count();
        let mut w = fresh_world();
        let e = w.spawn_empty();
        w.insert(e, Timer::every(100, count_cb));

        anchor(&mut w, 0);

        // Pause halfway between anchor (next_at=100) and the first fire.
        mock::set_ms(60);
        let now = w.resource::<MonoClock>().unwrap().now_ms();
        w.get_mut::<Timer>(e).unwrap().pause(now);

        // Time keeps moving but the system must skip the timer.
        for step in [100, 200, 300] {
            mock::set_ms(step);
            timer_system(&mut w);
        }
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 0);

        // Resume at 300: the timer was paused for 240 ms, so the next
        // fire is original next_at (100) + 240 = 340.
        let now = w.resource::<MonoClock>().unwrap().now_ms();
        w.get_mut::<Timer>(e).unwrap().resume(now);
        assert!(!w.get::<Timer>(e).unwrap().is_paused());

        mock::set_ms(339);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 0);

        mock::set_ms(340);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn pause_and_resume_are_idempotent() {
        let _g = mock::lock();
        mock::set_ms(0);
        let mut w = fresh_world();
        let e = w.spawn_empty();
        w.insert(e, Timer::every(100, count_cb));
        anchor(&mut w, 0);

        let t = w.get_mut::<Timer>(e).unwrap();
        t.pause(50);
        t.pause(70); // second pause must not overwrite paused_at_ms.
        // Resume sees the original 50 anchor — slept = 200 - 50 = 150.
        t.resume(200);
        assert_eq!(t.next_at_ms, 100u32.wrapping_add(150));

        // Double resume is a no-op (already running).
        t.resume(300);
        assert_eq!(t.next_at_ms, 100u32.wrapping_add(150));
    }

    #[test]
    fn wrap_boundary_does_not_lose_a_fire() {
        // 49.7-day boundary: u32 ms rolls from u32::MAX to 0 mid-period.
        // The wrapping_sub due-check must still see the timer as due
        // when next_at is just past the wrap and now is just before.
        let _g = mock::lock();
        let pre_wrap = u32::MAX - 50;
        mock::set_ms(pre_wrap as u64);
        reset_count();
        let mut w = fresh_world();
        let e = w.spawn_empty();
        w.insert(e, Timer::every(100, count_cb));

        // Anchor at u32::MAX-50: next_at = (u32::MAX-50) + 100 wraps to 49.
        timer_system(&mut w);
        let stored = w.get::<Timer>(e).unwrap().next_at_ms;
        assert_eq!(stored, 49u32);

        mock::set_ms(48);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 0);

        mock::set_ms(49);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn after_fires_once_then_self_removes() {
        let _g = mock::lock();
        mock::set_ms(0);
        reset_count();
        let mut w = fresh_world();
        let e = w.spawn_empty();
        w.insert(e, Timer::after(50, count_cb));

        anchor(&mut w, 0);
        mock::set_ms(50);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);
        assert!(w.get::<Timer>(e).is_none(), "after-mode self-removes");

        mock::set_ms(200);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn repeat_fires_n_times_then_self_removes() {
        let _g = mock::lock();
        mock::set_ms(0);
        reset_count();
        let mut w = fresh_world();
        let e = w.spawn_empty();
        w.insert(e, Timer::repeat(3, 100, count_cb));

        anchor(&mut w, 0);
        for step in [100, 200, 300] {
            mock::set_ms(step);
            timer_system(&mut w);
        }
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 3);
        assert!(w.get::<Timer>(e).is_none());

        mock::set_ms(400);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 3);
    }

    #[test]
    #[should_panic(expected = "Timer::repeat needs times > 0")]
    fn repeat_zero_panics() {
        let _ = Timer::repeat(0, 100, count_cb);
    }

    #[test]
    fn until_stops_once_next_step_would_pass_deadline() {
        let _g = mock::lock();
        mock::set_ms(0);
        reset_count();
        let mut w = fresh_world();
        let e = w.spawn_empty();
        // deadline 250: fires at 100 and 200; the 300 step would cross
        // 250 so the system removes the component instead.
        w.insert(e, Timer::until(250, 100, count_cb));

        anchor(&mut w, 0);
        for step in [100, 200, 300] {
            mock::set_ms(step);
            timer_system(&mut w);
        }
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 2);
        assert!(w.get::<Timer>(e).is_none(), "until-mode self-removes");
    }

    #[test]
    fn every_fires_at_each_period_boundary() {
        let _g = mock::lock();
        mock::set_ms(0);
        reset_count();
        let mut w = fresh_world();
        let e = w.spawn_empty();
        w.insert(e, Timer::every(100, count_cb));

        // First tick anchors next_at = 0 + 100. No fire yet.
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 0);

        mock::set_ms(99);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 0);

        mock::set_ms(100);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);

        mock::set_ms(250);
        timer_system(&mut w);
        // Reset drift: fire once at 250, next_at <- 250+100 = 350.
        // No catch-up — even though 200 was skipped.
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 2);

        mock::set_ms(349);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 2);

        mock::set_ms(350);
        timer_system(&mut w);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 3);
    }
}
