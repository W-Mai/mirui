#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use mirui::app::App;
use mirui::ecs::{Component, Entity, World};
use mirui::input::event::GestureHandler;
use mirui::input::event::bubble_dispatch_at;
use mirui::input::event::gesture::GestureEvent;
use mirui::input::event::multi_tap::MultiTapTracker;
use mirui::types::Fixed;
use mirui::ui::IdMap;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::widgets::switch::{Switch, SwitchEvent, SwitchHandler};
use mirui::{compose, model, ui};
use tracking_allocator::tracked_allocations;

#[derive(Debug, PartialEq, Eq)]
struct Marker(u32);

impl Component for Marker {}

struct Parent(Entity);

impl Component for Parent {}

#[model]
struct EventCounter {
    value: u32,
}

#[model]
impl EventCounter {
    fn increment(&mut self) {
        self.value += 1;
    }

    fn value(&self) -> u32 {
        self.value
    }
}

#[compose]
fn model_event(counter: model!(EventCounter)) {
    ui! {
        View () on Tap {
            counter.increment();
            commands!().insert(target, Marker(counter.value()));
        }
    };
}

struct DropMarker(Rc<Cell<u32>>);

impl Component for DropMarker {}

impl Drop for DropMarker {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn fresh_world() -> (World, Entity) {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(MultiTapTracker::new());
    let root = WidgetBuilder::new(&mut world).id();
    (world, root)
}

fn tap(target: Entity) -> GestureEvent {
    GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target,
    }
}

#[test]
fn event_commands_apply_in_source_order_after_component_reads_end() {
    let (mut world, root) = fresh_world();
    let target = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () on Tap {
                assert_eq!(com!(com!(target, Parent).unwrap().0, Marker) .map(| marker | marker.0), Some(0));
                commands!().insert(target, Marker(1));
                commands!().insert(target, Marker(2));
                assert_eq!(com!(target, Marker) .map(| marker | marker.0), Some(0));
            }
        }
    };
    world.insert(target, Marker(0));
    world.insert(target, Parent(target));

    bubble_dispatch_at(&mut world, &tap(target), 100);

    assert_eq!(world.get::<Marker>(target), Some(&Marker(2)));
}

#[test]
fn panicking_event_discards_commands_and_reuses_its_queue() {
    let (mut world, root) = fresh_world();
    let drops = Rc::new(Cell::new(0));
    let payload_drops = Rc::clone(&drops);
    let should_panic = Rc::new(Cell::new(true));
    let fail_once = Rc::clone(&should_panic);
    let target = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () on Tap {
                if fail_once.replace(false) {
                    commands!().insert(target, DropMarker(Rc::clone(&payload_drops)));
                    commands!().insert(target, Marker(1));
                    panic!("event failed");
                }
                commands!().insert(target, Marker(2));
            }
        }
    };
    world.insert(target, Marker(0));

    let result = catch_unwind(AssertUnwindSafe(|| {
        bubble_dispatch_at(&mut world, &tap(target), 100);
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
    assert!(world.get::<DropMarker>(target).is_none());
    assert_eq!(world.get::<Marker>(target), Some(&Marker(0)));

    bubble_dispatch_at(&mut world, &tap(target), 200);
    assert_eq!(world.get::<Marker>(target), Some(&Marker(2)));
}

#[test]
fn warmed_event_commands_do_not_allocate() {
    let (mut world, root) = fresh_world();
    let target = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () on Tap { commands!().insert(target, Marker(7)); }
        }
    };
    world.insert(target, Marker(0));

    bubble_dispatch_at(&mut world, &tap(target), 100);
    let allocations = tracked_allocations(|| {
        bubble_dispatch_at(&mut world, &tap(target), 200);
    });

    assert_eq!(allocations, 0);
    assert_eq!(world.get::<Marker>(target), Some(&Marker(7)));
}

#[test]
fn generated_event_handler_cannot_move_between_worlds() {
    let (mut first, root) = fresh_world();
    let target = {
        let mut cx = ::mirui::ui::UiScope::new(&mut first, root);
        ui! {
            View () on Tap { commands!().insert(target, Marker(1)); }
        }
    };
    let handler = first
        .remove::<GestureHandler>(target)
        .expect("generated gesture handler");

    let (mut second, second_root) = fresh_world();
    second.insert(second_root, handler);
    let result = catch_unwind(AssertUnwindSafe(|| {
        GestureHandler::trigger(&mut second, second_root, &tap(second_root));
    }));

    assert!(result.is_err());
    assert!(second.get::<Marker>(second_root).is_none());
}

#[test]
fn warmed_model_event_and_commands_do_not_allocate() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let counter = app.add_model(EventCounter { value: 0 });
    app.compose(root, |cx| model_event(cx, counter.clone()));
    let target = app
        .world
        .query::<GestureHandler>()
        .iter()
        .map(|(entity, _)| entity)
        .next()
        .expect("generated event target");
    let event = tap(target);

    assert_eq!(
        GestureHandler::trigger(&mut app.world, target, &event),
        Some(true)
    );
    let allocations = tracked_allocations(|| {
        assert_eq!(
            GestureHandler::trigger(&mut app.world, target, &event),
            Some(true)
        );
    });

    assert_eq!(allocations, 0);
    assert_eq!(counter.value(), 2);
    assert_eq!(app.world.get::<Marker>(target), Some(&Marker(2)));
}

#[test]
fn business_events_preserve_fields_bubbling_and_deferred_commands() {
    let (mut world, root) = fresh_world();
    let recipient = world.spawn(Marker(0));
    let switch = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            Switch () on Toggled {
                assert!(now);
                assert_eq!(com!(recipient, Marker), Some(& Marker(0)));
                commands!().insert(recipient, Marker(1));
                assert_eq!(com!(recipient, Marker), Some(& Marker(0)));
                false
            }
        }
    };
    let callback = world
        .get::<SwitchHandler>(switch)
        .expect("switch handler")
        .on_event
        .clone_out();

    let consumed = callback.call(&mut world, switch, &SwitchEvent::Toggled { now: true });

    assert!(!consumed);
    assert_eq!(world.get::<Marker>(recipient), Some(&Marker(1)));
}
