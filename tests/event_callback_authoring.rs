use mirui::__macro_support::{AppContext, CommandCapacity, EntityContext};
use mirui::app::App;
use mirui::input::event::contract::{EntityNotification, Event, WidgetSource};
use mirui::input::event::input::Key;
use mirui::input::event::lifecycle::{Started, Suspended};
use mirui::{component, event, model};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[component]
struct Pad;

#[component]
#[derive(Debug, PartialEq)]
struct Position(i32);

#[component]
struct Active;

#[component]
struct Hidden;

#[component]
struct Label;

struct Changed(String);

impl Event for Changed {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

struct Clock(i32);
struct Settings(i32);
struct Missing;

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

#[event]
fn update_pad(
    first: model!(Counter),
    step: i32,
    changed: event!(Changed),
    node: entity!(),
    clock: res!(Clock),
    settings: res!(mut Settings),
    missing: res!(optional Missing),
    rows: query!(mut Position, optional Label; with Active; without Hidden;),
    second: model!(Counter),
) {
    assert!(missing.is_none());
    let Some((position, label)) = rows.get(node) else {
        return;
    };
    assert!(label.is_none());
    position.0 += changed.0.len() as i32 + step;
    settings.0 += clock.0;
    first.add(step);
    second.add(step * 2);
}

#[test]
fn generated_callback_preserves_instance_identity_and_short_typed_access() {
    let mut app = App::headless(32, 32);
    let first = app.add_model(Counter { value: 0 });
    let second = app.add_model(Counter { value: 10 });
    app.world.insert_resource(Clock(2));
    app.world.insert_resource(Settings(0));
    let node = app.world.spawn_empty();
    app.world.insert(node, Pad);
    app.world.insert(node, Position(0));
    app.world.insert(node, Active);
    let handler = update_pad::callback::<EntityContext<Pad>>(first.clone(), 3, second.clone())
        .prepare(&mut app.world, CommandCapacity::default())
        .unwrap();
    let changed = Changed(String::from("hello"));
    let allocations = tracked_allocations(|| {
        for _ in 0..8 {
            assert_eq!(handler.invoke(&mut app.world, &changed, node), Ok(Some(())));
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(first.value(), 24);
    assert_eq!(second.value(), 58);
    assert_eq!(app.world.get::<Position>(node), Some(&Position(64)));
    assert_eq!(app.world.resource::<Settings>().unwrap().0, 16);
}

#[event]
fn increment(counter: model!(Counter), amount: i32) {
    counter.add(amount);
}

#[test]
fn payload_independent_callback_binds_multiple_event_types_and_app_context() {
    let mut app = App::headless(32, 32);
    let counter = app.add_model(Counter { value: 0 });
    let started = increment::callback::<Started, AppContext>(counter.clone(), 2)
        .prepare(&mut app.world, CommandCapacity::default())
        .unwrap();
    let suspended = increment::callback::<Suspended, AppContext>(counter.clone(), 3)
        .prepare(&mut app.world, CommandCapacity::default())
        .unwrap();
    assert_eq!(started.invoke(&mut app.world, &Started, ()), Ok(Some(())));
    assert_eq!(
        suspended.invoke(&mut app.world, &Suspended, ()),
        Ok(Some(()))
    );
    assert_eq!(counter.value(), 5);
}

#[event]
fn reject_alias(left: res!(Settings), right: res!(mut Settings)) {
    let _ = (left, right);
    panic!("invalid access plan reached the body");
}

#[test]
fn generated_plan_and_all_model_owners_are_checked_at_preparation() {
    let mut app = App::headless(32, 32);
    let alias = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        reject_alias::callback::<Started, AppContext>()
            .prepare(&mut app.world, CommandCapacity::default())
    }));
    assert!(alias.is_err());

    let mut other = App::headless(32, 32);
    let first = app.add_model(Counter { value: 0 });
    let second = other.add_model(Counter { value: 0 });
    let owners = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        update_pad::callback::<EntityContext<Pad>>(first, 1, second)
    }));
    assert!(owners.is_err());
}

#[event]
fn remove_position(node: entity!(), rows: query!(Position), commands: commands!()) -> bool {
    let Some(position) = rows.get(node) else {
        return false;
    };
    assert_eq!(position.0, 7);
    commands.remove::<Position>(node);
    assert_eq!(rows.get(node).unwrap().0, 7);
    true
}

#[test]
fn generated_commands_commit_after_query_borrows_end() {
    let mut app = App::headless(32, 32);
    let node = app.world.spawn_empty();
    app.world.insert(node, Pad);
    app.world.insert(node, Position(7));
    let handler = remove_position::callback::<Key, EntityContext<Pad>>()
        .prepare(
            &mut app.world,
            CommandCapacity {
                records: 1,
                payload_bytes: 0,
                payload_alignment: 1,
            },
        )
        .unwrap();
    let allocations = tracked_allocations(|| {
        assert_eq!(
            handler.invoke(
                &mut app.world,
                &Key {
                    code: 1,
                    pressed: true
                },
                node
            ),
            Ok(Some(mirui::input::event::BubbleControl::Prevent))
        )
    });
    assert_eq!(allocations, 0);
    assert!(!app.world.has::<Position>(node));
}

#[allow(non_snake_case)]
#[event]
fn generated_names(__ACCESS: u32, __require_copy: u32, settings: res!(mut Settings)) {
    settings.0 += (__ACCESS + __require_copy) as i32;
}

#[test]
fn generated_helpers_do_not_capture_authored_parameter_names() {
    let mut app = App::headless(32, 32);
    app.world.insert_resource(Settings(0));
    let handler = generated_names::callback::<Started, AppContext>(3, 4)
        .prepare(&mut app.world, CommandCapacity::default())
        .unwrap();
    handler.invoke(&mut app.world, &Started, ()).unwrap();
    assert_eq!(app.world.resource::<Settings>().unwrap().0, 7);
}
