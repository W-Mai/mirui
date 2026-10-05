//! Typed declarative timers driven by [`MonoClock`].
//!
//! Timer callbacks receive only the capabilities declared by `#[timer]`.
//! Model handles are captured when a timer is installed, structural changes
//! are deferred, and the runtime reuses its entity and command buffers.

use alloc::rc::Rc;
use alloc::vec::Vec;

use crate::core::model::ModelHandle;
use crate::ecs::commands::{CommandError, CommandQueue, DeferredCommands};
use crate::ecs::world::WorldId;
use crate::ecs::{Entity, MonoClock, World};

/// Lifecycle mode of an installed [`Timer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerMode {
    /// Fire once after the configured delay.
    After,
    /// Fire at every configured interval.
    Every,
    /// Fire until the remaining invocation count reaches zero.
    Repeat { remaining: u32 },
    /// Fire at each interval before the absolute deadline.
    Until { deadline_ms: u32 },
}

type StaticTimerCallback = for<'commands> fn(Entity, DeferredCommands<'commands>);
type BoundTimerCallback = dyn for<'commands> Fn(Entity, DeferredCommands<'commands>);

#[derive(Clone)]
enum TimerCallback {
    Static(StaticTimerCallback),
    Bound(Rc<BoundTimerCallback>),
}

impl TimerCallback {
    fn invoke(&self, entity: Entity, commands: DeferredCommands<'_>) {
        match self {
            Self::Static(run) => run(entity, commands),
            Self::Bound(run) => run(entity, commands),
        }
    }
}

/// Runtime timer installed by the module generated from `#[timer]`.
///
/// `next_at_ms` uses wrapping `u32` clock arithmetic. Periods must remain
/// below half the clock range for ordering across the wrap boundary.
pub struct Timer {
    period_ms: u32,
    next_at_ms: u32,
    mode: TimerMode,
    paused_at_ms: Option<u32>,
    armed: bool,
    callback: TimerCallback,
    owner: Option<WorldId>,
}

impl crate::ecs::Component for Timer {}

impl Timer {
    fn new(
        schedule: generated::TimerSchedule,
        callback: TimerCallback,
        owner: Option<WorldId>,
    ) -> Self {
        Self {
            period_ms: schedule.period_ms(),
            next_at_ms: 0,
            mode: schedule.mode(),
            paused_at_ms: None,
            armed: false,
            callback,
            owner,
        }
    }

    /// Returns whether this timer is currently paused.
    pub const fn is_paused(&self) -> bool {
        self.paused_at_ms.is_some()
    }

    /// Returns the timer's current lifecycle mode.
    pub const fn mode(&self) -> TimerMode {
        self.mode
    }

    /// Pause the timer at `now_ms`. Repeated calls are no-ops.
    pub fn pause(&mut self, now_ms: u32) {
        if self.paused_at_ms.is_none() {
            self.paused_at_ms = Some(now_ms);
        }
    }

    /// Resume the timer and preserve the remaining duration.
    pub fn resume(&mut self, now_ms: u32) {
        if let Some(paused_at) = self.paused_at_ms.take() {
            let slept = now_ms.wrapping_sub(paused_at);
            self.next_at_ms = self.next_at_ms.wrapping_add(slept);
        }
    }

    fn assert_owner(&self, world: &World) {
        self.assert_owner_id(world.id());
    }

    fn assert_owner_id(&self, world: WorldId) {
        if let Some(owner) = self.owner {
            assert_eq!(owner, world, "bound timer belongs to a different App");
        }
    }

    fn require_model<H: ModelHandle>(self, model: &H) -> Self {
        assert_eq!(
            self.owner,
            Some(crate::core::model::registered_owner(model)),
            "bound timer models belong to different Apps"
        );
        self
    }
}

pub(crate) mod generated {
    use super::{DeferredCommands, Entity, ModelHandle, Rc, Timer, TimerCallback, TimerMode};

    /// Schedule descriptor emitted by `#[timer]`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TimerSchedule {
        After { period_ms: u32 },
        Every { period_ms: u32 },
        Repeat { count: u32, period_ms: u32 },
        Until { deadline_ms: u32, period_ms: u32 },
    }

    impl TimerSchedule {
        pub const fn after(period_ms: u32) -> Self {
            Self::After { period_ms }
        }

        pub const fn every(period_ms: u32) -> Self {
            Self::Every { period_ms }
        }

        pub fn repeat(count: u32, period_ms: u32) -> Self {
            assert!(count > 0, "timer repeat count must be greater than zero");
            Self::Repeat { count, period_ms }
        }

        pub const fn until(deadline_ms: u32, period_ms: u32) -> Self {
            Self::Until {
                deadline_ms,
                period_ms,
            }
        }

        pub(super) const fn period_ms(self) -> u32 {
            match self {
                Self::After { period_ms }
                | Self::Every { period_ms }
                | Self::Repeat { period_ms, .. }
                | Self::Until { period_ms, .. } => period_ms,
            }
        }

        pub(super) const fn mode(self) -> TimerMode {
            match self {
                Self::After { .. } => TimerMode::After,
                Self::Every { .. } => TimerMode::Every,
                Self::Repeat { count, .. } => TimerMode::Repeat { remaining: count },
                Self::Until { deadline_ms, .. } => TimerMode::Until { deadline_ms },
            }
        }
    }

    /// Constructor seam used by generated timer modules without Model captures.
    pub fn static_timer(
        schedule: TimerSchedule,
        run: for<'commands> fn(Entity, DeferredCommands<'commands>),
    ) -> Timer {
        Timer::new(schedule, TimerCallback::Static(run), None)
    }

    /// Constructor seam used by generated timer modules with Model captures.
    pub fn bound_timer<H: ModelHandle>(
        schedule: TimerSchedule,
        model: &H,
        run: impl for<'commands> Fn(Entity, DeferredCommands<'commands>) + 'static,
    ) -> Timer {
        Timer::new(
            schedule,
            TimerCallback::Bound(Rc::new(run)),
            Some(crate::core::model::registered_owner(model)),
        )
    }

    /// Require an additional captured Model to belong to the timer's App.
    pub fn require_timer_model<H: ModelHandle>(timer: Timer, model: &H) -> Timer {
        timer.require_model(model)
    }
}

#[derive(Default)]
struct TimerRuntime {
    entities: Vec<Entity>,
    commands: CommandQueue,
}

impl TimerRuntime {
    fn reserve_timers(&mut self, required: usize) {
        self.entities
            .try_reserve(required.saturating_sub(self.entities.len()))
            .expect("timer entity buffer capacity exhausted");
    }

    fn tick(&mut self, world: &mut World, now_ms: u32) -> Result<(), CommandError> {
        world.query::<Timer>().collect_into(&mut self.entities);
        let owner = world.id();

        for index in 0..self.entities.len() {
            let entity = self.entities[index];
            let callback = {
                let Some(timer) = world.get_mut::<Timer>(entity) else {
                    continue;
                };
                timer.assert_owner_id(owner);
                if timer.paused_at_ms.is_some() {
                    continue;
                }
                if !timer.armed {
                    timer.next_at_ms = now_ms.wrapping_add(timer.period_ms);
                    timer.armed = true;
                    continue;
                }
                if (now_ms.wrapping_sub(timer.next_at_ms) as i32) < 0 {
                    continue;
                }
                timer.callback.clone()
            };

            let mut batch = self.commands.begin();
            callback.invoke(entity, batch.commands());
            let result = batch.finish(world);

            if let Some(timer) = world.get_mut::<Timer>(entity) {
                let remove = match timer.mode {
                    TimerMode::After => true,
                    TimerMode::Every => {
                        timer.next_at_ms = now_ms.wrapping_add(timer.period_ms);
                        false
                    }
                    TimerMode::Repeat { remaining } => {
                        if remaining <= 1 {
                            true
                        } else {
                            timer.mode = TimerMode::Repeat {
                                remaining: remaining - 1,
                            };
                            timer.next_at_ms = now_ms.wrapping_add(timer.period_ms);
                            false
                        }
                    }
                    TimerMode::Until { deadline_ms } => {
                        let candidate = now_ms.wrapping_add(timer.period_ms);
                        if (candidate.wrapping_sub(deadline_ms) as i32) > 0 {
                            true
                        } else {
                            timer.next_at_ms = candidate;
                            false
                        }
                    }
                };
                if remove {
                    let _ = world.remove::<Timer>(entity);
                }
            }
            result?;
        }
        Ok(())
    }
}

/// Install a timer and return the entity used by its `entity!()` capability.
pub(crate) fn install(world: &mut World, timer: Timer) -> Entity {
    timer.assert_owner(world);
    let required = world.query::<Timer>().iter().count().saturating_add(1);
    if world.resource::<TimerRuntime>().is_none() {
        world.insert_resource(TimerRuntime::default());
    }
    world
        .resource_mut::<TimerRuntime>()
        .expect("TimerRuntime was just inserted")
        .reserve_timers(required);
    let entity = world.spawn_empty();
    world.insert(entity, timer);
    entity
}

/// Advance every installed timer. Missing [`MonoClock`] or runtime state is a no-op.
///
/// A completed callback advances or ends its schedule even if command submission fails. The first failure stops the remaining timers and preserves completed updates.
pub fn timer_system(world: &mut World) -> Result<(), CommandError> {
    let Some(now_ms) = world.resource::<MonoClock>().map(MonoClock::now_ms) else {
        return Ok(());
    };
    world
        .with_resource_box::<TimerRuntime, _>(|runtime, world| runtime.tick(world, now_ms))
        .unwrap_or(Ok(()))
}

crate::ecs::system::framework_system!(pub timer_system, Timer);

#[cfg(test)]
mod tests {
    use super::generated::{TimerSchedule, static_timer};
    use super::*;
    use crate::core::time::mock;
    use crate::test_support::tracked_allocations;
    use core::sync::atomic::{AtomicU32, Ordering};

    static FIRE_COUNT: AtomicU32 = AtomicU32::new(0);

    fn count_cb(_entity: Entity, _commands: DeferredCommands<'_>) {
        FIRE_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    fn cancel_cb(entity: Entity, mut commands: DeferredCommands<'_>) {
        FIRE_COUNT.fetch_add(1, Ordering::SeqCst);
        commands.remove::<Timer>(entity);
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Fired(u32);

    impl crate::ecs::Component for Fired {}

    fn mark_cb(entity: Entity, mut commands: DeferredCommands<'_>) {
        let next = FIRE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
        commands.insert(entity, Fired(next));
    }

    fn ordered_cb(entity: Entity, mut commands: DeferredCommands<'_>) {
        commands.insert(entity, Fired(1));
        commands.insert(entity, Fired(2));
    }

    fn panic_cb(entity: Entity, mut commands: DeferredCommands<'_>) {
        commands.insert(entity, Fired(99));
        panic!("timer callback failed");
    }

    struct Pulse;

    impl crate::input::event::contract::Event for Pulse {
        type Route = crate::input::event::contract::EntityNotification;
        type Source = crate::input::event::contract::WidgetSource<Fired>;
        type Options = ();
    }

    fn submit_cb(entity: Entity, mut commands: DeferredCommands<'_>) {
        let next = FIRE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
        commands.insert(entity, Fired(next));
        commands.emit(entity, Pulse);
        commands.insert(entity, Fired(99));
    }

    fn remove_then_submit_cb(entity: Entity, mut commands: DeferredCommands<'_>) {
        commands.insert(entity, Fired(1));
        commands.remove::<Timer>(entity);
        commands.emit(entity, Pulse);
    }

    fn submission_failed(result: Result<(), CommandError>) {
        assert!(matches!(
            result,
            Err(CommandError::Commit(
                crate::ecs::CommandCommitError::MissingService { .. }
            ))
        ));
    }

    fn reset_count() {
        FIRE_COUNT.store(0, Ordering::SeqCst);
    }

    fn fresh_world() -> World {
        let mut world = World::new();
        world.insert_resource(MonoClock::from_time_source());
        world
    }

    fn add(world: &mut World, schedule: TimerSchedule, callback: StaticTimerCallback) -> Entity {
        install(world, static_timer(schedule, callback))
    }

    fn tick(world: &mut World, now_ms: u32) {
        mock::set_ms(now_ms as u64);
        timer_system(world).unwrap();
    }

    #[test]
    fn pause_freezes_then_resume_restores_remaining_period() {
        let _guard = mock::install();
        reset_count();
        let mut world = fresh_world();
        let entity = add(&mut world, TimerSchedule::every(100), count_cb);
        tick(&mut world, 0);

        world.get_mut::<Timer>(entity).unwrap().pause(60);
        for step in [100, 200, 300] {
            tick(&mut world, step);
        }
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 0);

        world.get_mut::<Timer>(entity).unwrap().resume(300);
        tick(&mut world, 339);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 0);
        tick(&mut world, 340);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn pause_and_resume_are_idempotent() {
        let _guard = mock::install();
        let mut world = fresh_world();
        let entity = add(&mut world, TimerSchedule::every(100), count_cb);
        tick(&mut world, 0);

        let timer = world.get_mut::<Timer>(entity).unwrap();
        timer.pause(50);
        timer.pause(70);
        timer.resume(200);
        assert_eq!(timer.next_at_ms, 250);

        timer.resume(300);
        assert_eq!(timer.next_at_ms, 250);
    }

    #[test]
    fn wrap_boundary_does_not_lose_a_fire() {
        let _guard = mock::install();
        reset_count();
        let mut world = fresh_world();
        let pre_wrap = u32::MAX - 50;
        let entity = add(&mut world, TimerSchedule::every(100), count_cb);

        tick(&mut world, pre_wrap);
        assert_eq!(world.get::<Timer>(entity).unwrap().next_at_ms, 49);
        tick(&mut world, 48);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 0);
        tick(&mut world, 49);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn schedules_fire_with_reset_drift_and_remove_at_their_bounds() {
        let _guard = mock::install();
        reset_count();
        let mut world = fresh_world();

        let after = add(&mut world, TimerSchedule::after(50), count_cb);
        tick(&mut world, 0);
        tick(&mut world, 50);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);
        assert!(world.get::<Timer>(after).is_none());

        reset_count();
        let repeat = add(&mut world, TimerSchedule::repeat(3, 100), count_cb);
        tick(&mut world, 0);
        for step in [100, 200, 300] {
            tick(&mut world, step);
        }
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 3);
        assert!(world.get::<Timer>(repeat).is_none());

        reset_count();
        let until = add(&mut world, TimerSchedule::until(250, 100), count_cb);
        tick(&mut world, 0);
        for step in [100, 200, 300] {
            tick(&mut world, step);
        }
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 2);
        assert!(world.get::<Timer>(until).is_none());

        reset_count();
        let every = add(&mut world, TimerSchedule::every(100), count_cb);
        tick(&mut world, 0);
        tick(&mut world, 100);
        tick(&mut world, 250);
        tick(&mut world, 349);
        tick(&mut world, 350);
        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 3);
        assert!(world.get::<Timer>(every).is_some());
    }

    #[test]
    fn callback_commands_apply_before_lifecycle_update() {
        let _guard = mock::install();
        reset_count();
        let mut world = fresh_world();
        let entity = add(&mut world, TimerSchedule::every(10), cancel_cb);

        tick(&mut world, 0);
        tick(&mut world, 10);

        assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);
        assert!(world.get::<Timer>(entity).is_none());
    }

    #[test]
    fn callback_commands_apply_in_source_order() {
        let _guard = mock::install();
        let mut world = fresh_world();
        let entity = add(&mut world, TimerSchedule::after(10), ordered_cb);

        tick(&mut world, 0);
        tick(&mut world, 10);

        assert_eq!(world.get::<Fired>(entity), Some(&Fired(2)));
    }

    #[test]
    fn submission_failures_advance_each_schedule_without_repeating_the_same_call() {
        let _guard = mock::install();
        for (schedule, remaining, ends_at_second_call) in [
            (TimerSchedule::after(10), None, true),
            (TimerSchedule::every(10), Some(TimerMode::Every), false),
            (
                TimerSchedule::repeat(2, 10),
                Some(TimerMode::Repeat { remaining: 1 }),
                true,
            ),
            (TimerSchedule::until(10, 10), None, true),
            (
                TimerSchedule::until(25, 10),
                Some(TimerMode::Until { deadline_ms: 25 }),
                true,
            ),
        ] {
            reset_count();
            let mut world = fresh_world();
            let entity = add(&mut world, schedule, submit_cb);
            tick(&mut world, 0);
            mock::set_ms(10);
            submission_failed(timer_system(&mut world));
            assert_eq!(world.get::<Fired>(entity), Some(&Fired(1)));
            assert_eq!(world.get::<Timer>(entity).map(Timer::mode), remaining);
            if let Some(timer) = world.get::<Timer>(entity) {
                assert_eq!(timer.next_at_ms, 20);
            }
            assert!(world.resource::<TimerRuntime>().is_some());
            assert_eq!(tracked_allocations(|| tick(&mut world, 10)), 0);
            assert_eq!(FIRE_COUNT.load(Ordering::SeqCst), 1);
            mock::set_ms(20);
            let allocations = tracked_allocations(|| {
                if remaining.is_some() {
                    submission_failed(timer_system(&mut world));
                } else {
                    timer_system(&mut world).unwrap();
                }
            });
            assert_eq!(allocations, 0);
            assert_eq!(world.get::<Timer>(entity).is_none(), ends_at_second_call);
            assert_eq!(
                FIRE_COUNT.load(Ordering::SeqCst),
                if remaining.is_some() { 2 } else { 1 }
            );
        }
    }

    #[test]
    fn submission_error_survives_timer_removal_and_stops_remaining_timers() {
        let _guard = mock::install();
        reset_count();
        let mut world = fresh_world();
        let first = add(&mut world, TimerSchedule::every(10), remove_then_submit_cb);
        let second = add(&mut world, TimerSchedule::after(10), mark_cb);
        tick(&mut world, 0);
        mock::set_ms(10);
        submission_failed(timer_system(&mut world));
        assert!(world.get::<Timer>(first).is_none());
        assert_eq!(world.get::<Fired>(first), Some(&Fired(1)));
        assert!(world.get::<Fired>(second).is_none());
        assert_eq!(tracked_allocations(|| tick(&mut world, 10)), 0);
        assert_eq!(world.get::<Fired>(second), Some(&Fired(1)));
        assert!(world.get::<Timer>(second).is_none());
    }

    #[test]
    fn callback_panic_discards_commands_and_restores_runtime() {
        let _guard = mock::install();
        reset_count();
        let mut world = fresh_world();
        let panicking = add(&mut world, TimerSchedule::every(10), panic_cb);
        tick(&mut world, 0);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tick(&mut world, 10);
        }));
        assert!(result.is_err());
        assert!(world.get::<Fired>(panicking).is_none());

        let _ = world.remove::<Timer>(panicking);
        let healthy = add(&mut world, TimerSchedule::after(1), mark_cb);
        tick(&mut world, 10);
        tick(&mut world, 11);
        assert_eq!(world.get::<Fired>(healthy), Some(&Fired(1)));
    }

    #[test]
    fn bound_timer_cannot_move_between_worlds_after_installation() {
        let _guard = mock::install();
        let first = fresh_world();
        let timer = Timer::new(
            TimerSchedule::every(10),
            TimerCallback::Static(count_cb),
            Some(first.id()),
        );

        let mut second = fresh_world();
        let seed = add(&mut second, TimerSchedule::after(1), count_cb);
        let _ = second.remove::<Timer>(seed);
        let entity = second.spawn_empty();
        second.insert(entity, timer);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tick(&mut second, 0);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn warmed_tick_reuses_entity_and_command_buffers() {
        let _guard = mock::install();
        reset_count();
        let mut world = fresh_world();
        let entity = add(&mut world, TimerSchedule::every(1), mark_cb);
        world.insert(entity, Fired(0));

        tick(&mut world, 0);
        tick(&mut world, 1);
        let allocations = tracked_allocations(|| tick(&mut world, 2));

        assert_eq!(allocations, 0);
        assert_eq!(world.get::<Fired>(entity), Some(&Fired(2)));
    }

    #[test]
    #[should_panic(expected = "timer repeat count must be greater than zero")]
    fn repeat_zero_panics() {
        let _ = TimerSchedule::repeat(0, 100);
    }
}
