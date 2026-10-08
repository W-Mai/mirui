#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use mirui::app::App;
use mirui::ecs::{Component, Entity, World};
use mirui::input::event::BubbleControl;
use mirui::input::event::contract::Event;
use mirui::input::event::gesture::{GestureEvent, Tap};
use mirui::input::event::multi_tap::MultiTapTracker;
use mirui::types::Fixed;
use mirui::ui::IdMap;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::widgets::switch::{Switch, Toggled};
use mirui::{compose, event, model, ui};
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
        View (id: "counter_event") on Tap {
            counter.increment();
            commands!().insert(event.target, Marker(counter.value()));
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

fn event_capacity() -> mirui::app::EventCapacity {
    mirui::app::EventCapacity::new()
        .commands(
            mirui::ecs::CommandCapacity::new()
                .insert::<Marker>(2)
                .insert::<DropMarker>(1),
        )
        .notifications::<Toggled>(1)
        .flush_limit(1)
}

fn fresh_world() -> (World, Entity) {
    let mut app = App::headless(32, 32);
    app.with_default_widgets()
        .with_event_capacity(event_capacity());
    let mut world = app.world;
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

fn dispatch(world: &mut World, event: &GestureEvent, now_ms: u32) -> BubbleControl {
    let decision = mirui::__macro_support::dispatch_entity_gesture(world, event, now_ms).unwrap();
    mirui::__macro_support::flush_entity_events(world).unwrap();
    decision
}

#[event]
fn registered_model_commands(
    counter: model!(EventCounter),
    input: event!(Tap),
    node: entity!(),
    commands: commands!(),
) {
    assert_eq!(input.target, node);
    counter.increment();
    commands.insert(node, Marker(counter.value()));
}

fn propagate_tap(input: &Tap) -> bool {
    input.x < Fixed::ZERO
}

#[event]
fn aliased_resources(writes: res!(mut CaptureWrites), snapshot: res!(CaptureWrites)) {
    writes.0 += snapshot.0 + 1;
}

#[test]
fn callback_values_accept_functions_and_preserve_descriptor_requirements() {
    let mut first = App::headless(32, 32);
    let foreign = first.add_model(EventCounter { value: 0 });
    let mut app = App::headless(32, 32);
    app.with_event_capacity(event_capacity());
    app.add_resource(CaptureWrites::default());
    let local = app.add_model(EventCounter { value: 0 });
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            Column() {
                View(id: "function") on Tap(propagate_tap)
                View(id: "local") on Tap(registered_model_commands::callback(local.clone()))
                View(id: "foreign") on Tap(registered_model_commands::callback(foreign.clone()))
                View(id: "alias") on Tap(aliased_resources::callback())
            }
        };
    });
    let function = app.world.find_by_id("function").unwrap();
    let local_target = app.world.find_by_id("local").unwrap();
    let foreign_target = app.world.find_by_id("foreign").unwrap();
    let alias_target = app.world.find_by_id("alias").unwrap();
    assert!(app.world.has::<mirui::ui::HitTarget>(function));
    assert!(app.world.has::<mirui::ui::HitTarget>(local_target));
    assert!(!app.world.has::<mirui::ui::HitTarget>(foreign_target));
    assert!(!app.world.has::<mirui::ui::HitTarget>(alias_target));
    assert_eq!(
        dispatch(&mut app.world, &tap(function), 100),
        BubbleControl::Allow
    );
    assert_eq!(
        dispatch(&mut app.world, &tap(local_target), 200),
        BubbleControl::Prevent
    );
    assert_eq!(app.world.get::<Marker>(local_target), Some(&Marker(1)));
    assert_eq!(local.value(), 1);
    assert_eq!(foreign.value(), 0);
    assert_eq!(app.world.resource::<CaptureWrites>().unwrap().0, 0);
    let allocations = tracked_allocations(|| {
        dispatch(&mut app.world, &tap(function), 300);
    });
    assert_eq!(allocations, 0);

    let mut default_budget = App::headless(32, 32);
    let default_model = default_budget.add_model(EventCounter { value: 0 });
    let root = default_budget.spawn_root().id();
    default_budget.compose(root, |cx| {
        ui! { View(id: "commands") on Tap(registered_model_commands::callback(default_model.clone())) };
    });
    let target = default_budget.world.find_by_id("commands").unwrap();
    assert_eq!(
        dispatch(&mut default_budget.world, &tap(target), 100),
        BubbleControl::Prevent
    );
    assert_eq!(default_budget.world.get::<Marker>(target), Some(&Marker(1)));
    assert_eq!(default_model.value(), 1);

    let mut no_budget = App::headless(32, 32);
    no_budget.with_event_capacity(mirui::app::EventCapacity::new());
    let model = no_budget.add_model(EventCounter { value: 0 });
    let root = no_budget.spawn_root().id();
    no_budget.compose(root, |cx| {
        ui! { View(id: "commands") on Tap(registered_model_commands::callback(model.clone())) };
    });
    let target = no_budget.world.find_by_id("commands").unwrap();
    assert!(!no_budget.world.has::<mirui::ui::HitTarget>(target));
    assert_eq!(model.value(), 0);
}

#[event]
fn toggled_commands(
    input: event!(Toggled),
    recipient: Entity,
    markers: query!(Marker),
    commands: commands!(),
) {
    assert!(input.now);
    assert_eq!(markers.get(recipient), Some(&Marker(0)));
    commands.insert(recipient, Marker(1));
    assert_eq!(markers.get(recipient), Some(&Marker(0)));
}

#[test]
fn event_commands_apply_in_source_order_after_component_reads_end() {
    let (mut world, root) = fresh_world();
    let target = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () on Tap(|input: event!(Tap), parents: query!(Parent), markers: query!(Marker), commands: commands!()| {
                let target = input.target;
                let parent = parents.get(target).unwrap().0;
                assert_eq!(markers.get(parent).map(|marker| marker.0), Some(0));
                commands.insert(target, Marker(1));
                commands.insert(target, Marker(2));
                assert_eq!(markers.get(target).map(|marker| marker.0), Some(0));
            })
        }
    };
    world.insert(target, Marker(0));
    world.insert(target, Parent(target));

    dispatch(&mut world, &tap(target), 100);

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
                    commands!().insert(event.target, DropMarker(Rc::clone(&payload_drops)));
                    commands!().insert(event.target, Marker(1));
                    panic!("event failed");
                }
                commands!().insert(event.target, Marker(2));
            }
        }
    };
    world.insert(target, Marker(0));

    let result = catch_unwind(AssertUnwindSafe(|| {
        dispatch(&mut world, &tap(target), 100);
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
    assert!(world.get::<DropMarker>(target).is_none());
    assert_eq!(world.get::<Marker>(target), Some(&Marker(0)));

    dispatch(&mut world, &tap(target), 200);
    assert_eq!(world.get::<Marker>(target), Some(&Marker(2)));
}

#[test]
fn warmed_event_commands_do_not_allocate() {
    let (mut world, root) = fresh_world();
    let target = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () on Tap { commands!().insert(event.target, Marker(7)); }
        }
    };
    world.insert(target, Marker(0));

    dispatch(&mut world, &tap(target), 100);
    let allocations = tracked_allocations(|| {
        dispatch(&mut world, &tap(target), 200);
    });

    assert_eq!(allocations, 0);
    assert_eq!(world.get::<Marker>(target), Some(&Marker(7)));
}

#[test]
fn foreign_registered_model_binding_is_skipped_without_blocking_local_bindings() {
    let mut first = App::headless(32, 32);
    let foreign = first.add_model(EventCounter { value: 0 });
    let mut second = App::headless(32, 32);
    second.with_event_capacity(event_capacity());
    let local = second.add_model(EventCounter { value: 0 });
    let target = second.spawn_root().id();
    second.world.insert(target, Marker(0));

    mirui::__macro_support::install_entity_event(
        &mut second.world,
        target,
        Tap::EVENT,
        registered_model_commands::callback::<mirui::__macro_support::NodeContext>(foreign.clone()),
    );
    assert!(!second.world.has::<mirui::ui::HitTarget>(target));
    assert_eq!(
        dispatch(&mut second.world, &tap(target), 100),
        BubbleControl::Allow
    );
    assert_eq!(second.world.get::<Marker>(target), Some(&Marker(0)));
    assert_eq!(foreign.value(), 0);

    mirui::__macro_support::install_entity_event(
        &mut second.world,
        target,
        Tap::EVENT,
        registered_model_commands::callback::<mirui::__macro_support::NodeContext>(local.clone()),
    );
    assert_eq!(
        dispatch(&mut second.world, &tap(target), 200),
        BubbleControl::Prevent
    );
    assert_eq!(local.value(), 1);
    assert_eq!(foreign.value(), 0);
    assert_eq!(second.world.get::<Marker>(target), Some(&Marker(1)));
}

#[derive(Default)]
struct CaptureWrites(u32);

struct CaptureConfig<G> {
    game: G,
}

#[test]
fn ordinary_captures_preserve_value_semantics_and_model_access_guards() {
    let mut first = App::headless(32, 32);
    let foreign = first.add_model(EventCounter { value: 0 });
    let config = CaptureConfig {
        game: foreign.clone(),
    };
    let mut second = App::headless(32, 32);
    second.add_resource(CaptureWrites::default());
    let root = second.spawn_root().id();
    second.compose(root, |cx| {
        ui! {
            View(id: "foreign_capture") on Tap(|writes: res!(mut CaptureWrites)| {
                writes.0 += 1;
                config.game.increment();
            })
        };
    });
    let target = second.world.find_by_id("foreign_capture").unwrap();
    assert!(second.world.has::<mirui::ui::HitTarget>(target));
    let result = catch_unwind(AssertUnwindSafe(|| {
        dispatch(&mut second.world, &tap(target), 100);
    }));
    assert!(result.is_err());
    assert_eq!(second.world.resource::<CaptureWrites>().unwrap().0, 1);
    assert_eq!(foreign.value(), 0);
}

#[test]
fn warmed_model_event_and_commands_do_not_allocate() {
    let mut app = App::headless(32, 32);
    app.with_default_widgets()
        .with_event_capacity(event_capacity());
    let root = app.spawn_root().id();
    let counter = app.add_model(EventCounter { value: 0 });
    app.compose(root, model_event(counter.clone()));
    let target = app.world.find_by_id("counter_event").unwrap();
    let event = tap(target);

    assert_eq!(
        dispatch(&mut app.world, &event, 100),
        BubbleControl::Prevent
    );
    let allocations = tracked_allocations(|| {
        assert_eq!(
            dispatch(&mut app.world, &event, 200),
            BubbleControl::Prevent
        );
    });

    assert_eq!(allocations, 0);
    assert_eq!(counter.value(), 2);
    assert_eq!(app.world.get::<Marker>(target), Some(&Marker(2)));
}

#[test]
fn business_events_preserve_fields_and_deferred_commands() {
    let (mut world, root) = fresh_world();
    let recipient = world.spawn(Marker(0));
    let switch = {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            Switch () on Toggled(toggled_commands::callback(recipient))
        }
    };
    assert_eq!(
        mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(switch), 100).unwrap(),
        BubbleControl::Prevent
    );
    assert!(world.get::<Switch>(switch).unwrap().on);
    assert_eq!(world.get::<Marker>(recipient), Some(&Marker(0)));
    assert_eq!(
        mirui::__macro_support::flush_entity_events(&mut world).unwrap(),
        1
    );
    assert_eq!(world.get::<Marker>(recipient), Some(&Marker(1)));
}
