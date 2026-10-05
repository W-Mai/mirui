use mirui::app::App;
use mirui::ecs::Component;
use mirui::{model, system};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[model]
struct Counter {
    value: i32,
}

#[model]
impl Counter {
    fn add(&mut self, amount: i32) {
        self.value += amount;
    }

    fn value(&self) -> i32 {
        self.value
    }
}

#[derive(Clone, Copy)]
struct Position(i32);

impl Component for Position {}

#[derive(Clone, Copy)]
struct Velocity(i32);

impl Component for Velocity {}

struct Active;

impl Component for Active {}

struct Hidden;

impl Component for Hidden {}

#[derive(Clone, Copy)]
struct Integrated;

impl Component for Integrated {}

struct Settings {
    ticks: u32,
}

struct Missing;

mod nested {
    #[mirui::system(order = Animation, expect = super::Active)]
    pub fn integrate(
        counter: model!(super::Counter),
        settings: res!(mut super::Settings),
        missing: res!(optional super::Missing),
        rows: query!(
            Entity,
            super::Position,
            mut super::Velocity,
            optional super::Integrated;
            with super::Active;
            without super::Hidden;
        ),
        commands: commands!(),
    ) {
        assert!(missing.is_none());
        settings.ticks += 1;
        for (entity, position, velocity, integrated) in rows {
            counter.add(position.0 + velocity.0);
            velocity.0 += 1;
            if integrated.is_none() {
                commands.insert(entity, super::Integrated);
            }
        }
    }
}

#[test]
fn generated_system_composes_all_roles_and_reuses_its_command_queue() {
    let mut app = App::headless(32, 32);
    let counter = app.add_model(Counter { value: 0 });
    app.world.insert_resource(Settings { ticks: 0 });

    let visible = app.world.spawn_empty();
    app.world.insert(visible, Position(3));
    app.world.insert(visible, Velocity(4));
    app.world.insert(visible, Active);

    let hidden = app.world.spawn_empty();
    app.world.insert(hidden, Position(100));
    app.world.insert(hidden, Velocity(100));
    app.world.insert(hidden, Active);
    app.world.insert(hidden, Hidden);

    app.add_system(nested::integrate::system(counter.clone()));
    app.systems.run_all(&mut app.world).unwrap();

    assert_eq!(counter.value(), 7);
    assert_eq!(app.world.get::<Velocity>(visible).unwrap().0, 5);
    assert_eq!(app.world.get::<Velocity>(hidden).unwrap().0, 100);
    assert!(app.world.has::<Integrated>(visible));
    assert!(!app.world.has::<Integrated>(hidden));

    let allocations = tracked_allocations(|| app.systems.run_all(&mut app.world).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(counter.value(), 15);
    assert_eq!(app.world.resource::<Settings>().unwrap().ticks, 2);
}

struct Target(mirui::ecs::Entity);

#[system]
fn update_target(
    target: res!(Target),
    positions: query!(Position),
    velocities: query!(mut Velocity),
) {
    let position = positions.get(target.0).expect("target position");
    let same_position = positions.get(target.0).expect("shared target position");
    assert!(core::ptr::eq(position, same_position));
    velocities.get(target.0).expect("target velocity").0 += position.0;
    velocities
        .get(target.0)
        .expect("released target reborrow")
        .0 += position.0;
}

#[test]
fn generated_system_supports_shared_and_exclusive_point_fetches() {
    let mut app = App::headless(32, 32);
    let target = app.world.spawn_empty();
    app.world.insert(target, Position(3));
    app.world.insert(target, Velocity(4));
    app.world.insert_resource(Target(target));
    app.add_system(update_target::system());

    let allocations = tracked_allocations(|| app.systems.run_all(&mut app.world).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(app.world.get::<Velocity>(target).unwrap().0, 10);
}
