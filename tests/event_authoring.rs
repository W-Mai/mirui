#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use mirui::app::App;
use mirui::ecs::{Component, Entity, World};
use mirui::input::event::BubbleControl;
use mirui::input::event::contract::Event;
use mirui::input::event::focus::{FocusChanged, FocusSelectors, Focusable};
use mirui::input::event::gesture::{GestureEvent, Tap};
use mirui::input::event::hover::{HoverChanged, HoverSelectors};
use mirui::input::event::press::{PressCancelled, PressSelectors, Released};
use mirui::types::Fixed;
use mirui::ui::widgets::switch::{Switch, Toggled};
use mirui::{compose, event, model, ui};
use tracking_allocator::tracked_allocations;

type TestApp =
    App<mirui::surface::framebuf::FramebufSurface<fn(&[u8], mirui::types::PhysicalRect)>>;

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

#[event]
fn count_focus(event: event!(FocusChanged), counter: model!(EventCounter)) {
    if event.focused {
        counter.increment();
    }
}

#[compose]
fn focus_panel(
    block: model!(EventCounter),
    closure: model!(EventCounter),
    function: model!(EventCounter),
) {
    ui! {
        Column () {
            View (id: "focus_block") [Focusable] on FocusChanged {
                if event.focused { block.increment(); }
            }
            View (id: "focus_closure") [Focusable] on FocusChanged(move |event: &FocusChanged| {
                if event.focused { closure.increment(); }
            })
            View (id: "focus_function") [Focusable] on FocusChanged(count_focus::callback(function))
        }
    };
}

#[test]
fn focus_notifications_use_all_three_authoring_forms_without_pointer_feedback() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let counters = core::array::from_fn::<_, 3, _>(|_| app.add_model(EventCounter { value: 0 }));
    app.compose(
        root,
        focus_panel(
            counters[0].clone(),
            counters[1].clone(),
            counters[2].clone(),
        ),
    );
    for (index, id) in ["focus_block", "focus_closure", "focus_function"]
        .into_iter()
        .enumerate()
    {
        let node = app.world.find_by_id(id).unwrap();
        assert!(!app.world.has::<mirui::ui::HitTarget>(node));
        dispatch(&mut app.world, &tap(node), index as u32);
        assert_eq!(counters[index].value(), 1);
    }
    assert!(counters.iter().all(|counter| counter.value() == 1));
}

#[event]
fn count_hover(event: event!(HoverChanged), counter: model!(EventCounter)) {
    if event.hovered {
        counter.increment();
    }
}

#[compose]
fn hover_panel(
    block: model!(EventCounter),
    closure: model!(EventCounter),
    function: model!(EventCounter),
) {
    ui! {
        Column (width: 32, height: 32, id: "hover_block") {
            Column (width: 32, height: 32, id: "hover_closure") {
                View (width: 32, height: 32, id: "hover_function")
                    on HoverChanged(count_hover::callback(function))
                    on Tap {}
            } on HoverChanged(move |event: &HoverChanged| {
                if event.hovered { closure.increment(); }
            })
        } on HoverChanged {
            if event.hovered { block.increment(); }
        }
    };
}

#[test]
fn hover_notifications_use_all_authoring_forms_and_preserve_container_hit_semantics() {
    use mirui::input::event::input::{InputEvent, PointerKind};
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let counters = core::array::from_fn::<_, 3, _>(|_| app.add_model(EventCounter { value: 0 }));
    app.compose(
        root,
        hover_panel(
            counters[0].clone(),
            counters[1].clone(),
            counters[2].clone(),
        ),
    );
    app.render().unwrap();
    for id in ["hover_block", "hover_closure"] {
        let node = app.world.find_by_id(id).unwrap();
        assert!(!app.world.has::<mirui::ui::HitTarget>(node));
    }
    mirui::input::event::dispatch_input(
        &mut app.world,
        Some(root),
        &InputEvent::PointerMove {
            kind: PointerKind::Mouse,
            id: 0,
            x: 10.into(),
            y: 10.into(),
        },
        0,
        32,
        32,
    );
    mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
    assert!(counters.iter().all(|counter| counter.value() == 1));
}

struct DropMarker(Rc<Cell<u32>>);

#[event]
fn count_cancel(event: event!(PressCancelled), counter: model!(EventCounter)) {
    assert_eq!(
        event.reason,
        mirui::input::event::press::PressCancelReason::Gesture
    );
    counter.increment();
}

#[compose]
fn press_panel(down: model!(EventCounter), up: model!(EventCounter), cancel: model!(EventCounter)) {
    ui! {
        View (width: 32, height: 32, id: "press_panel")
            on Tap {}
            on Pressed { down.increment(); }
            on Released(move |_event: &Released| { up.increment(); })
            on PressCancelled(count_cancel::callback(cancel))
    };
}

#[test]
fn press_notifications_support_inline_closure_and_typed_callbacks() {
    use mirui::input::event::input::{InputEvent, PointerKind};
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let counters = core::array::from_fn::<_, 3, _>(|_| app.add_model(EventCounter { value: 0 }));
    app.compose(
        root,
        press_panel(
            counters[0].clone(),
            counters[1].clone(),
            counters[2].clone(),
        ),
    );
    app.render().unwrap();
    for (now, input) in [
        (
            0,
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: 10.into(),
                y: 10.into(),
            },
        ),
        (
            10,
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x: 10.into(),
                y: 10.into(),
            },
        ),
        (
            20,
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: 10.into(),
                y: 10.into(),
            },
        ),
        (
            30,
            InputEvent::PointerMove {
                kind: PointerKind::Mouse,
                id: 0,
                x: 30.into(),
                y: 10.into(),
            },
        ),
        (
            40,
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x: 30.into(),
                y: 10.into(),
            },
        ),
    ] {
        mirui::input::event::dispatch_input(&mut app.world, Some(root), &input, now, 32, 32);
        mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
    }
    assert_eq!(
        counters.each_ref().map(|counter| counter.value()),
        [2, 1, 1]
    );
}

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
                .insert::<DropMarker>(1)
                .set::<mirui::ui::property::prop::SwitchOn>(1)
                .emit::<Toggled>(1),
        )
        .notifications::<Toggled>(1)
        .flush_limit(1)
}

fn fresh_app() -> (TestApp, Entity) {
    let mut app = App::headless(32, 32);
    app.with_default_widgets()
        .with_event_capacity(event_capacity());
    let root = app.spawn_root().id();
    (app, root)
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
            Column () {
                View (id: "function") on Tap(propagate_tap)
                View (id: "local") on Tap(registered_model_commands::callback(local.clone()))
                View (id: "foreign") on Tap(registered_model_commands::callback(foreign.clone()))
                View (id: "alias") on Tap(aliased_resources::callback())
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
        ui! {
            View (id: "commands") on Tap(registered_model_commands::callback(default_model.clone()))
        };
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
        ui! {
            View (id: "commands") on Tap(registered_model_commands::callback(model.clone()))
        };
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
    let (mut app, root) = fresh_app();
    let target = app.compose(root, |cx| {
        ui! {
            View () [
                Marker(0),
            ] on Tap(|
                input: event!(Tap),
                parents: query!(Parent),
                markers: query!(Marker),
                commands: commands!()|
            {
                let target = input.target;
                let parent = parents.get(target).unwrap().0;
                assert_eq!(markers.get(parent).map(| marker | marker.0), Some(0));
                commands.insert(target, Marker(1));
                commands.insert(target, Marker(2));
                assert_eq!(markers.get(target).map(| marker | marker.0), Some(0));
            })
        }
    });
    let world = &mut app.world;
    world.insert(target, Parent(target));

    dispatch(world, &tap(target), 100);

    assert_eq!(world.get::<Marker>(target), Some(&Marker(2)));
}

#[test]
fn panicking_event_discards_commands_and_reuses_its_queue() {
    let (mut app, root) = fresh_app();
    let drops = Rc::new(Cell::new(0));
    let payload_drops = Rc::clone(&drops);
    let should_panic = Rc::new(Cell::new(true));
    let fail_once = Rc::clone(&should_panic);
    let target = app.compose(root, |cx| {
        ui! {
            View () [
                Marker(0),
            ] on Tap {
                if fail_once.replace(false) {
                    commands!().insert(event.target, DropMarker(Rc::clone(&payload_drops)));
                    commands!().insert(event.target, Marker(1));
                    panic!("event failed");
                }
                commands!().insert(event.target, Marker(2));
            }
        }
    });
    let world = &mut app.world;

    let result = catch_unwind(AssertUnwindSafe(|| {
        dispatch(world, &tap(target), 100);
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
    assert!(world.get::<DropMarker>(target).is_none());
    assert_eq!(world.get::<Marker>(target), Some(&Marker(0)));

    dispatch(world, &tap(target), 200);
    assert_eq!(world.get::<Marker>(target), Some(&Marker(2)));
}

#[test]
fn warmed_event_commands_do_not_allocate() {
    let (mut app, root) = fresh_app();
    let target = app.compose(root, |cx| {
        ui! {
            View () [
                Marker(0),
            ] on Tap { commands!().insert(event.target, Marker(7)); }
        }
    });
    let world = &mut app.world;

    dispatch(world, &tap(target), 100);
    let allocations = tracked_allocations(|| {
        dispatch(world, &tap(target), 200);
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
            View (id: "foreign_capture") on Tap(|writes: res!(mut CaptureWrites)| {
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
    let (mut app, root) = fresh_app();
    let recipient = app.compose(root, |tree| tree.spawn(Marker(0)));
    let switch = app.compose(root, |cx| {
        ui! {
            Switch () on Toggled(toggled_commands::callback(recipient))
        }
    });
    let world = &mut app.world;
    assert_eq!(
        mirui::__macro_support::dispatch_entity_gesture(world, &tap(switch), 100).unwrap(),
        BubbleControl::Prevent
    );
    assert!(world.get::<Switch>(switch).unwrap().on);
    assert_eq!(world.get::<Marker>(recipient), Some(&Marker(0)));
    assert_eq!(
        mirui::__macro_support::flush_entity_events(world).unwrap(),
        1
    );
    assert_eq!(world.get::<Marker>(recipient), Some(&Marker(1)));
}

mod third_party_notifications {
    use super::tracking_allocator::tracked_allocations;
    use mirui::input::event::contract::{EntityNotification, Event, EventKey, WidgetSource};
    use mirui::prelude::*;
    use mirui::render::DrawCommand;
    use mirui::surface::framebuf::FramebufSurface;
    use std::cell::Cell;
    use std::rc::Rc;

    #[component]
    struct Pad {
        strength: u8,
    }

    struct PadHit {
        old: u8,
        new: u8,
    }

    impl Event for PadHit {
        type Route = EntityNotification;
        type Source = WidgetSource<Pad>;
        type Options = ();
    }

    impl Pad {
        #[allow(non_upper_case_globals)]
        const Hit: EventKey<PadHit> = PadHit::EVENT;
    }

    #[model]
    struct Receipts {
        entries: [(u8, u8, u8); 9],
        len: usize,
    }

    #[model]
    impl Receipts {
        fn record(&mut self, entrance: u8, old: u8, new: u8) {
            self.entries[self.len] = (entrance, old, new);
            self.len += 1;
        }

        fn snapshot(&self) -> ([(u8, u8, u8); 9], usize) {
            (self.entries, self.len)
        }
    }

    #[model]
    struct Drive {
        pending: Option<(Entity, u8)>,
    }

    #[model]
    impl Drive {
        fn request(&mut self, node: Entity, strength: u8) {
            assert!(self.pending.is_none());
            self.pending = Some((node, strength));
        }

        fn pending(&self) -> Option<(Entity, u8)> {
            self.pending
        }

        fn clear_request(&mut self) {
            self.pending = None;
        }
    }

    #[event]
    fn record_hit(
        receipts: model!(Receipts),
        input: event!(PadHit),
        node: entity!(),
        pads: query!(Pad),
    ) {
        assert_eq!(
            pads.get(node).expect("notification source").strength,
            input.new
        );
        receipts.record(2, input.old, input.new);
    }

    #[system]
    fn update_pad(drive: model!(Drive), pads: query!(mut Pad), commands: commands!()) {
        let Some((node, new)) = drive.pending() else {
            return;
        };
        drive.clear_request();
        let hit = {
            let pad = pads.get(node).expect("requested Pad");
            let old = pad.strength;
            pad.strength = new;
            PadHit { old, new }
        };
        commands.emit(node, hit);
    }

    struct PaintProbe(Rc<Cell<usize>>);

    #[view(component = Pad)]
    fn paint_pad(pad: component!(), probe: res!(PaintProbe), paint: paint!()) {
        probe.0.set(probe.0.get() + 1);
        paint.draw(&DrawCommand::Fill {
            area: paint.rect(),
            transform: paint.transform(),
            quad: paint.quad(),
            color: Color::rgb(pad.strength, 0, 0),
            radius: Fixed::ZERO,
            opa: 255,
        });
        paint.set_background_handled(true);
    }

    #[test]
    fn third_party_notifications_repaint_through_app_and_all_on_forms() {
        let pixel = Rc::new(Cell::new([0_u8; 4]));
        let paints = Rc::new(Cell::new(0_usize));
        let flushes = Rc::new(Cell::new(0_usize));
        let pixel_on_flush = pixel.clone();
        let flushes_on_flush = flushes.clone();
        let backend = FramebufSurface::new(8, 8, move |pixels, _| {
            let offset = (4 * 8 + 4) * 4;
            pixel_on_flush.set([
                pixels[offset],
                pixels[offset + 1],
                pixels[offset + 2],
                pixels[offset + 3],
            ]);
            flushes_on_flush.set(flushes_on_flush.get() + 1);
        });
        let mut app = App::new(backend);
        app.with_event_capacity(
            EventCapacity::new()
                .notifications::<PadHit>(1)
                .flush_limit(1),
        );
        app.with_default_widgets().with_widget(paint_pad::view());
        app.add_resource(PaintProbe(paints.clone()));
        let receipts = app.add_model(Receipts {
            entries: [(0, 0, 0); 9],
            len: 0,
        });
        let drive = app.add_model(Drive { pending: None });
        let block_receipts = receipts.clone();
        let closure_receipts = receipts.clone();
        let root = app.spawn_root().id();
        let node = app.compose(root, |cx| {
            ui! {
                View (width: 8, height: 8) [
                    Pad { strength: 1 },
                ] on Pad::Hit { block_receipts.record(0, event.old, event.new); } on Pad::Hit(|input: event!(PadHit), node: entity!(), pads: query!(Pad)| {
                    assert_eq!(pads.get(node).expect("notification source").strength, input.new);
                    closure_receipts.record(1, input.old, input.new);
                }) on Pad::Hit(record_hit::callback(receipts.clone()))
            }
        });
        app.add_system(update_pad::system(drive.clone()));

        assert!(!app.tick());
        assert_eq!(pixel.get(), [1, 0, 0, 255]);
        assert_eq!(receipts.snapshot(), ([(0, 0, 0); 9], 0));
        assert_eq!((paints.get(), flushes.get()), (1, 1));

        assert!(!app.tick());
        assert_eq!(receipts.snapshot(), ([(0, 0, 0); 9], 0));
        assert_eq!((paints.get(), flushes.get()), (1, 1));

        drive.request(node, 7);
        assert!(!app.tick());
        let mut expected = [(0, 0, 0); 9];
        expected[..3].copy_from_slice(&[(0, 1, 7), (1, 1, 7), (2, 1, 7)]);
        assert_eq!(receipts.snapshot(), (expected, 3));
        assert_eq!(pixel.get(), [7, 0, 0, 255]);
        assert_eq!((paints.get(), flushes.get()), (2, 2));

        for (round, (old, new)) in [(7, 11), (11, 19)].into_iter().enumerate() {
            let allocations = tracked_allocations(|| {
                drive.request(node, new);
                assert!(!app.tick());
            });
            assert_eq!(allocations, 0);
            let offset = (round + 1) * 3;
            expected[offset..offset + 3].copy_from_slice(&[
                (0, old, new),
                (1, old, new),
                (2, old, new),
            ]);
            assert_eq!(receipts.snapshot(), (expected, offset + 3));
            assert_eq!(pixel.get(), [new, 0, 0, 255]);
            assert_eq!((paints.get(), flushes.get()), (round + 3, round + 3));
        }

        assert!(!app.tick());
        assert_eq!(receipts.snapshot(), (expected, 9));
        assert_eq!(pixel.get(), [19, 0, 0, 255]);
        assert_eq!((paints.get(), flushes.get()), (4, 4));
    }
}
