use mirui::__macro_support::{
    AccessError, AppContext, EntityContext, ModelOwnerError, PreparationError,
};
use mirui::app::App;
use mirui::core::reactive::{Computed, Signal};
use mirui::ecs::CommandCapacity;
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

struct SetupResource {
    value: i32,
    drops: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Drop for SetupResource {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

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
fn read_setup_resource(counter: model!(Counter), resource: res!(SetupResource)) {
    counter.add(resource.value);
}

#[test]
fn app_resource_setup_is_immediate_and_replacement_uses_the_existing_storage() {
    let drops = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut app = App::headless(8, 8);
    let counter = app.add_model(Counter { value: 0 });
    app.add_resource(SetupResource {
        value: 2,
        drops: drops.clone(),
    })
    .add_resource(Clock(7));
    assert_eq!(app.world.resource::<SetupResource>().unwrap().value, 2);
    assert_eq!(app.world.resource::<Clock>().unwrap().0, 7);
    assert_eq!(counter.value(), 0);
    let address = app.world.resource::<SetupResource>().unwrap() as *const SetupResource;
    app.on_event(
        Started::EVENT,
        read_setup_resource::callback(counter.clone()),
    );

    assert_eq!(
        tracked_allocations(|| {
            app.add_resource(SetupResource {
                value: 5,
                drops: drops.clone(),
            });
        }),
        0
    );
    assert_eq!(drops.get(), 1);
    assert_eq!(
        app.world.resource::<SetupResource>().unwrap() as *const _,
        address
    );

    let mut other = App::headless(8, 8);
    let independent = other.add_model(Counter { value: 0 });
    other.add_resource(SetupResource {
        value: 11,
        drops: drops.clone(),
    });
    other.on_event(
        Started::EVENT,
        read_setup_resource::callback(independent.clone()),
    );
    assert!(!app.tick());
    assert!(!other.tick());
    assert_eq!(counter.value(), 5);
    assert_eq!(independent.value(), 11);
    drop(app);
    assert_eq!(drops.get(), 2);
    assert_eq!(other.world.resource::<SetupResource>().unwrap().value, 11);
    drop(other);
    assert_eq!(drops.get(), 3);
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

#[derive(Default)]
struct CaptureCalls {
    optional: u32,
    nested: u32,
    shared: u32,
}

#[event]
fn optional_increment(counter: model!(Option<Counter>), calls: res!(mut CaptureCalls)) {
    calls.optional += 1;
    if let Some(counter) = counter {
        counter.add(1);
    }
}

#[event]
fn nested_increment(counter: model!(Option<Option<Counter>>), calls: res!(mut CaptureCalls)) {
    calls.nested += 1;
    if let Some(Some(counter)) = counter {
        counter.add(2);
    }
}

#[event]
fn shared_values(
    signal: model!(Signal<i32>),
    computed: model!(Computed<i32>),
    counter: model!(Counter),
    calls: res!(mut CaptureCalls),
) {
    calls.shared += 1;
    counter.add(signal.get() + computed.get());
}

#[test]
fn app_callbacks_consume_optional_nested_and_shared_bindings_once() {
    let mut app = App::headless(8, 8);
    let counter = app.add_model(Counter { value: 0 });
    let independent = app.add_model(Counter { value: 40 });
    let signal = Signal::new(5_i32);
    // Keep the Computed app-owned while preserving the rootless Started lifecycle.
    let computed =
        mirui::__macro_support::with_world_scope(&mut app.world, || Computed::new(|| 7_i32));
    app.world.insert_resource(CaptureCalls::default());

    app.on_event(
        Started::EVENT,
        optional_increment::callback(Some(counter.clone())),
    );
    app.on_event(Started::EVENT, optional_increment::callback(None));
    app.on_event(
        Started::EVENT,
        nested_increment::callback(Some(Some(counter.clone()))),
    );
    app.on_event(Started::EVENT, nested_increment::callback(Some(None)));
    app.on_event(Started::EVENT, nested_increment::callback(None));
    app.on_event(
        Started::EVENT,
        shared_values::callback(signal, computed, counter.clone()),
    );

    for _ in 0..2 {
        assert!(!app.tick());
        assert_eq!(counter.value(), 15);
        assert_eq!(independent.value(), 40);
        let calls = app.world.resource::<CaptureCalls>().unwrap();
        assert_eq!((calls.optional, calls.nested, calls.shared), (2, 3, 1));
    }
}

#[event]
fn invalid_optional_owners(
    first: model!(Option<Counter>),
    second: model!(Option<Option<Counter>>),
) {
    let _ = (first, second);
    panic!("invalid optional binding reached its callback body");
}

#[test]
fn app_skips_mixed_foreign_and_ended_optional_bindings_and_keeps_valid_callbacks() {
    let mut app = App::headless(8, 8);
    let counter = app.add_model(Counter { value: 0 });
    let mut other = App::headless(8, 8);
    let foreign = other.add_model(Counter { value: 30 });
    let ended = {
        let mut retired = App::headless(8, 8);
        retired.add_model(Counter { value: 50 })
    };
    app.world.insert_resource(CaptureCalls::default());

    app.on_event(
        Started::EVENT,
        optional_increment::callback(Some(counter.clone())),
    );
    app.on_event(
        Started::EVENT,
        invalid_optional_owners::callback(Some(counter.clone()), Some(Some(foreign.clone()))),
    );
    app.on_event(
        Started::EVENT,
        invalid_optional_owners::callback(None, Some(Some(foreign.clone()))),
    );
    app.on_event(
        Started::EVENT,
        invalid_optional_owners::callback(Some(ended), None),
    );
    app.on_event(Started::EVENT, optional_increment::callback(None));

    assert!(!app.tick());
    assert_eq!(counter.value(), 1);
    assert_eq!(foreign.value(), 30);
    assert_eq!(app.world.resource::<CaptureCalls>().unwrap().optional, 2);
}

#[cfg(feature = "audio")]
#[event]
fn mute_audio(audio: model!(mirui::audio::AudioHandle), counter: model!(Counter)) {
    assert!(audio.set_muted(true));
    counter.add(1);
}

#[cfg(feature = "audio")]
#[test]
fn app_callback_consumes_shared_audio_with_a_registered_model() {
    static BANK: mirui::audio::AudioBank = mirui::audio::AudioBank::new(&[]);
    let mut app = App::headless(8, 8);
    app.add_plugin(mirui::app::plugins::AudioPlugin::new(
        mirui::audio::SilentAudioSink::default(),
        &BANK,
    ));
    let counter = app.add_model(Counter { value: 0 });
    let audio = app.audio().unwrap();
    app.on_event(
        Started::EVENT,
        mute_audio::callback(audio.clone(), counter.clone()),
    );

    for _ in 0..2 {
        assert!(!app.tick());
        assert_eq!(counter.value(), 1);
        assert!(audio.state().unwrap().muted);
    }
}

#[event]
fn reject_alias(left: res!(Settings), right: res!(mut Settings)) {
    let _ = (left, right);
    panic!("invalid access plan reached the body");
}

#[test]
fn generated_plan_and_all_model_owners_are_checked_at_preparation() {
    let mut app = App::headless(32, 32);
    let alias = reject_alias::callback::<Started, AppContext>()
        .prepare(&mut app.world, CommandCapacity::default())
        .err()
        .unwrap();
    assert_eq!(
        alias,
        PreparationError::Access {
            callback: "reject_alias",
            error: AccessError::Conflict {
                type_name: std::any::type_name::<Settings>()
            },
        }
    );

    let mut other = App::headless(32, 32);
    let first = app.add_model(Counter { value: 0 });
    let second = other.add_model(Counter { value: 0 });
    let owners = update_pad::callback::<EntityContext<Pad>>(first, 1, second)
        .prepare(&mut app.world, CommandCapacity::default())
        .err()
        .unwrap();
    assert_eq!(
        owners,
        PreparationError::Owner {
            callback: "update_pad",
            error: ModelOwnerError::MixedOwners,
        }
    );
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
        .prepare(&mut app.world, CommandCapacity::new().remove::<Position>(1))
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

type CallbackResult = ();

trait CallbackTypes {
    type Clock;
    const WIDTH: usize;
}

mod relative {
    use super::*;

    pub const WIDTH: usize = 2;
    pub struct Clock(pub i32);

    #[component]
    pub struct Marker;

    impl super::CallbackTypes for Marker {
        type Clock = Clock;
        const WIDTH: usize = self::WIDTH;
    }

    #[event]
    #[cfg_attr(all(), cfg(all()), doc = "A typed event callback.", inline)]
    pub fn update(
        counter: model!(super::Counter),
        amount: [u8; <self::Marker as super::CallbackTypes>::WIDTH],
        changed: event!(super::Changed),
        node: entity!(),
        clock: res!(<self::Marker as super::CallbackTypes>::Clock),
        settings: res!(mut super::Settings),
        rows: query!(mut super::Position; with self::Marker;),
    ) -> super::CallbackResult {
        counter.add(i32::from(amount[0]));
        rows.get(node).unwrap().0 += changed.0.len() as i32;
        settings.0 += clock.0;
    }
}

#[test]
fn generated_factory_preserves_authored_relative_type_and_const_paths() {
    let mut app = App::headless(32, 32);
    let counter = app.add_model(Counter { value: 0 });
    app.world.insert_resource(relative::Clock(3));
    app.world.insert_resource(Settings(0));
    let node = app.world.spawn(Pad);
    app.world.insert(node, Position(0));
    app.world.insert(node, relative::Marker);
    let handler = relative::update::callback::<EntityContext<Pad>>(counter.clone(), [2, 0])
        .prepare(&mut app.world, CommandCapacity::default())
        .unwrap();
    let changed = Changed(String::from("hi"));
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(handler.invoke(&mut app.world, &changed, node), Ok(Some(())));
        }),
        0
    );
    assert_eq!(counter.value(), 2);
    assert_eq!(app.world.get::<Position>(node), Some(&Position(2)));
    assert_eq!(app.world.resource::<Settings>().unwrap().0, 3);
}
