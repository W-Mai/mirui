//! Frozen `#[timer(...)]` authoring syntax and runtime behavior.

#![cfg(feature = "std")]

use mirui::app::App;
use mirui::core::time::mock;
use mirui::core::timer::{Timer, TimerMode};
use mirui::ecs::{Component, Entity, MonoClock};
use mirui_macros::{model, timer};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[derive(Debug, PartialEq, Eq)]
struct Fired(u32);

impl Component for Fired {}

#[model]
struct Counter {
    fires: u32,
    last_timer: Option<Entity>,
}

#[model]
impl Counter {
    fn record(&mut self, timer: Entity) {
        self.fires += 1;
        self.last_timer = Some(timer);
    }

    fn fires(&self) -> u32 {
        self.fires
    }

    fn last_timer(&self) -> Option<Entity> {
        self.last_timer
    }
}

#[timer(after = 100)]
fn after_once(timer: entity!(), commands: commands!()) {
    commands.insert(timer, Fired(1));
}

#[timer(every = 250)]
fn every_tick(counter: model!(Counter), timer: entity!(), commands: commands!()) {
    counter.record(timer);
    commands.insert(timer, Fired(counter.fires()));
}

#[timer(repeat = (3, 50))]
fn repeat_tick(counter: model!(Counter), timer: entity!()) {
    counter.record(timer);
}

#[timer(until = (99, 30))]
fn until_tick(counter: model!(Counter), timer: entity!()) {
    counter.record(timer);
}

#[timer(after = 10)]
fn capture_pair(first: model!(Counter), second: model!(Counter), timer: entity!()) {
    first.record(timer);
    second.record(timer);
}

fn fresh() -> App<mirui::surface::framebuf::FramebufSurface<fn(&[u8], mirui::types::PhysicalRect)>>
{
    let mut app = App::headless(32, 32);
    app.world.insert_resource(MonoClock::from_time_source());
    app.add_system(mirui::core::timer::timer_system::system());
    app
}

fn tick(app: &mut impl TimerTestApp, now_ms: u32) {
    mock::set_ms(now_ms as u64);
    app.run_systems();
}

trait TimerTestApp {
    fn run_systems(&mut self);
}

impl<B, F> TimerTestApp for App<B, F>
where
    B: mirui::surface::Surface,
    F: mirui::app::RendererFactory<B>,
{
    fn run_systems(&mut self) {
        self.systems.run_all(&mut self.world);
    }
}

#[test]
fn after_schedule_fires_once_and_applies_commands() {
    let _guard = mock::install();
    let mut app = fresh();
    let entity = app.add_timer(after_once::timer());
    assert_eq!(
        app.world.get::<Timer>(entity).unwrap().mode(),
        TimerMode::After
    );

    tick(&mut app, 0);
    tick(&mut app, 100);

    assert_eq!(app.world.get::<Fired>(entity), Some(&Fired(1)));
    assert!(app.world.get::<Timer>(entity).is_none());
}

#[test]
fn every_schedule_captures_the_selected_model_instance_and_timer_entity() {
    let _guard = mock::install();
    let mut app = fresh();
    let selected = app.add_model(Counter {
        fires: 0,
        last_timer: None,
    });
    let untouched = app.add_model(Counter {
        fires: 0,
        last_timer: None,
    });
    let entity = app.add_timer(every_tick::timer(selected.clone()));
    assert_eq!(
        app.world.get::<Timer>(entity).unwrap().mode(),
        TimerMode::Every
    );

    tick(&mut app, 0);
    tick(&mut app, 250);

    assert_eq!(selected.fires(), 1);
    assert_eq!(selected.last_timer(), Some(entity));
    assert_eq!(untouched.fires(), 0);
    assert_eq!(app.world.get::<Fired>(entity), Some(&Fired(1)));
    assert!(app.world.get::<Timer>(entity).is_some());
}

#[test]
fn warmed_bound_timer_and_commands_do_not_allocate() {
    let _guard = mock::install();
    let mut app = fresh();
    let counter = app.add_model(Counter {
        fires: 0,
        last_timer: None,
    });
    let entity = app.add_timer(every_tick::timer(counter.clone()));

    tick(&mut app, 0);
    tick(&mut app, 250);
    let allocations = tracked_allocations(|| tick(&mut app, 500));

    assert_eq!(allocations, 0);
    assert_eq!(counter.fires(), 2);
    assert_eq!(app.world.get::<Fired>(entity), Some(&Fired(2)));
}

#[test]
fn repeat_schedule_fires_the_declared_count() {
    let _guard = mock::install();
    let mut app = fresh();
    let counter = app.add_model(Counter {
        fires: 0,
        last_timer: None,
    });
    let entity = app.add_timer(repeat_tick::timer(counter.clone()));
    assert_eq!(
        app.world.get::<Timer>(entity).unwrap().mode(),
        TimerMode::Repeat { remaining: 3 }
    );

    tick(&mut app, 0);
    for step in [50, 100, 150] {
        tick(&mut app, step);
    }

    assert_eq!(counter.fires(), 3);
    assert!(app.world.get::<Timer>(entity).is_none());
}

#[test]
fn until_schedule_stops_before_the_next_deadline_crossing() {
    let _guard = mock::install();
    let mut app = fresh();
    let counter = app.add_model(Counter {
        fires: 0,
        last_timer: None,
    });
    let entity = app.add_timer(until_tick::timer(counter.clone()));
    assert_eq!(
        app.world.get::<Timer>(entity).unwrap().mode(),
        TimerMode::Until { deadline_ms: 99 }
    );

    tick(&mut app, 0);
    for step in [30, 60, 90, 120] {
        tick(&mut app, step);
    }

    assert_eq!(counter.fires(), 3);
    assert!(app.world.get::<Timer>(entity).is_none());
}

#[test]
fn every_captured_model_must_belong_to_the_installing_app() {
    let _guard = mock::install();
    let mut owning_app = fresh();
    let mut other_app = fresh();
    let counter = owning_app.add_model(Counter {
        fires: 0,
        last_timer: None,
    });

    let timer = every_tick::timer(counter);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        other_app.add_timer(timer);
    }));

    assert!(result.is_err());
}

#[test]
fn all_captured_models_must_share_one_app() {
    let _guard = mock::install();
    let mut first_app = fresh();
    let mut second_app = fresh();
    let first = first_app.add_model(Counter {
        fires: 0,
        last_timer: None,
    });
    let second = second_app.add_model(Counter {
        fires: 0,
        last_timer: None,
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        capture_pair::timer(first, second)
    }));

    assert!(result.is_err());
}
