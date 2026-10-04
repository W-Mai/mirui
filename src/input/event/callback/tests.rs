use super::*;
use crate::ecs::query::system::{ComponentDriver, Read, Write};
use crate::ecs::system::param::{OptionalRes, QueryParam, Res, ResMut};
use crate::input::event::BubbleControl;
use crate::input::event::contract::{AppNotification, AppSource, EntityNotification, WidgetSource};
use crate::input::event::input::Key;
use crate::test_support::tracked_allocations;
use alloc::string::String;
use core::cell::Cell;

struct Widget;
impl Component for Widget {}

#[derive(Debug, PartialEq, Eq)]
struct Position(i32);
impl Component for Position {}

struct Clock(i32);
struct Settings(i32);
struct Missing;

#[crate::model]
struct Counter {
    value: u32,
}

#[crate::model]
impl Counter {
    fn increment(&mut self) {
        self.value += 1;
    }

    fn value(&self) -> u32 {
        self.value
    }
}

struct Changed(i32);
impl Event for Changed {
    type Route = EntityNotification;
    type Source = WidgetSource<Widget>;
    type Options = ();
}

struct Owned(String);
impl Event for Owned {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

#[test]
fn invocation_combines_typed_payload_resources_and_point_query() {
    let mut world = World::new();
    let node = world.spawn_empty();
    world.insert(node, Position(3));
    world.insert_resource(Clock(2));
    world.insert_resource(Settings(0));
    type Rows = QueryParam<ComponentDriver<Position>, Write<Position>, ()>;
    type Params = (Res<Clock>, ResMut<Settings>, OptionalRes<Missing>, Rows);
    const ACCESS: &[SystemAccess] = &[
        SystemAccess::read_resource::<Clock>(),
        SystemAccess::write_resource::<Settings>(),
        SystemAccess::read_resource::<Missing>(),
        SystemAccess::write_component::<Position>(),
    ];
    // SAFETY: ACCESS exactly describes Params; no captured Models or raw
    // World pointers are used by this body.
    let callback = unsafe {
        event_callback::<Changed, EntityContext<Widget>, Params, ()>(
            "changed",
            ACCESS,
            |(clock, settings, missing, mut rows), payload, target| {
                assert!(missing.is_none());
                settings.0 += clock.0;
                let position = rows.get(target).unwrap();
                position.0 += payload.0;
            },
        )
    }
    .prepare(&mut world);
    assert_eq!(callback.invoke(&mut world, &Changed(4), node), Some(()));
    assert_eq!(world.get::<Position>(node), Some(&Position(7)));
    assert_eq!(world.resource::<Settings>().unwrap().0, 2);
}

#[test]
fn prepare_rejects_conflicts_before_fetching_or_running_body() {
    let mut world = World::new();
    const ACCESS: &[SystemAccess] = &[
        SystemAccess::read_resource::<Missing>(),
        SystemAccess::write_resource::<Missing>(),
    ];
    type Params = (Res<Missing>, ResMut<Missing>);
    let called = Rc::new(Cell::new(false));
    let seen = Rc::clone(&called);
    // SAFETY: the deliberately conflicting plan fully describes Params.
    // Preparation must reject it before any fetch can be attempted.
    let callback = unsafe {
        event_callback::<Key, AppContext, Params, bool>("conflict", ACCESS, move |_, _, ()| {
            seen.set(true);
            true
        })
    };
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        callback.prepare(&mut world)
    }));
    let message = error.err().unwrap().downcast::<String>().unwrap();
    assert!(message.contains("event `conflict` has conflicting access"));
    assert!(!called.get());
}

#[test]
fn component_and_resource_domains_are_independent_and_shared_reads_are_valid() {
    let mut world = World::new();
    let node = world.spawn_empty();
    world.insert(node, Position(3));
    world.insert_resource(Position(9));
    type Rows = QueryParam<ComponentDriver<Position>, Read<Position>, ()>;
    type Params = (Res<Position>, Res<Position>, Rows);
    const ACCESS: &[SystemAccess] = &[
        SystemAccess::read_resource::<Position>(),
        SystemAccess::read_resource::<Position>(),
        SystemAccess::read_component::<Position>(),
    ];
    // SAFETY: all accesses are shared, including the two independent domains.
    let callback = unsafe {
        event_callback::<Changed, EntityContext<Widget>, Params, ()>(
            "shared",
            ACCESS,
            |(first, second, rows), _, target| {
                assert!(core::ptr::eq(first, second));
                assert_eq!(rows.get(target), Some(&Position(3)));
            },
        )
    }
    .prepare(&mut world);
    assert_eq!(callback.invoke(&mut world, &Changed(0), node), Some(()));
}

#[test]
fn stale_generation_is_skipped_before_required_resource_fetch() {
    let mut world = World::new();
    let old = world.spawn_empty();
    world.despawn(old);
    let replacement = world.spawn_empty();
    assert_eq!(old.id, replacement.id);
    assert_ne!(old.generation, replacement.generation);
    const ACCESS: &[SystemAccess] = &[SystemAccess::read_resource::<Missing>()];
    // SAFETY: the sole resource access matches Params; the source generation
    // check must run before trying to fetch this absent required resource.
    let callback = unsafe {
        event_callback::<Changed, EntityContext<Widget>, (Res<Missing>,), ()>(
            "stale",
            ACCESS,
            |_, _, _| panic!("stale callback invoked"),
        )
    }
    .prepare(&mut world);
    assert_eq!(callback.invoke(&mut world, &Changed(0), old), None);
}

#[test]
fn installed_owner_is_checked_before_body_execution() {
    let mut first = World::new();
    let mut second = World::new();
    // SAFETY: the empty plan exactly describes the empty parameter set.
    let callback = unsafe {
        event_callback::<Key, AppContext, (), bool>("owner", &[], |(), _, ()| {
            panic!("cross-owner body invoked")
        })
    }
    .prepare(&mut first);
    let payload = Key {
        code: 1,
        pressed: true,
    };
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        callback.invoke(&mut second, &payload, ())
    }));
    let message = error.err().unwrap().downcast::<String>().unwrap();
    assert!(message.contains("event callback belongs to a different App"));
}

#[test]
fn app_payload_is_borrowed_without_clone_or_entity() {
    let mut world = World::new();
    let payload = Owned(String::from("owned"));
    let address = payload.0.as_ptr() as usize;
    // SAFETY: the empty plan matches Params and no Models are captured.
    let callback = unsafe {
        event_callback::<Owned, AppContext, (), ()>("owned", &[], move |(), payload, ()| {
            assert_eq!(payload.0.as_ptr() as usize, address);
        })
    }
    .prepare(&mut world);
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..64 {
                let snapshot = callback.clone();
                assert_eq!(snapshot.invoke(&mut world, &payload, ()), Some(()));
            }
        }),
        0
    );
    assert_eq!(payload.0, "owned");
}

#[test]
fn input_callback_normalizes_consume_results() {
    let mut world = World::new();
    // SAFETY: the empty plan matches Params and no Models are captured.
    let callback = unsafe {
        event_callback::<Key, AppContext, (), bool>("key", &[], |(), input, ()| input.pressed)
    }
    .prepare(&mut world);
    let mut payload = Key {
        code: 1,
        pressed: true,
    };
    assert_eq!(
        callback.invoke(&mut world, &payload, ()),
        Some(BubbleControl::Prevent)
    );
    payload.pressed = false;
    assert_eq!(
        callback.invoke(&mut world, &payload, ()),
        Some(BubbleControl::Allow)
    );
}

#[test]
fn model_capture_uses_the_invocation_owner_and_preserves_instance_identity() {
    let mut app = crate::app::App::headless(32, 32);
    let first = app.add_model(Counter { value: 1 });
    let second = app.add_model(Counter { value: 8 });
    let captured = first.clone();
    // SAFETY: Params is empty, the sole captured Model is declared below,
    // and the body does not acquire World through another path.
    let callback = unsafe {
        event_callback::<Key, AppContext, (), ()>("model", &[], move |(), _, ()| {
            captured.increment();
        })
    }
    .require_model(&first)
    .prepare(&mut app.world);
    let payload = Key {
        code: 1,
        pressed: true,
    };
    assert_eq!(
        callback.invoke(&mut app.world, &payload, ()),
        Some(BubbleControl::Prevent)
    );
    assert_eq!(first.value(), 2);
    assert_eq!(second.value(), 8);
}

#[test]
fn model_capture_rejects_mixed_owners_and_wrong_installation_owner() {
    let mut first_app = crate::app::App::headless(32, 32);
    let mut second_app = crate::app::App::headless(32, 32);
    let first = first_app.add_model(Counter { value: 1 });
    let second = second_app.add_model(Counter { value: 2 });
    let build = || {
        // SAFETY: the body captures no Models or World and Params is empty.
        unsafe { event_callback::<Key, AppContext, (), ()>("owner", &[], |(), _, ()| ()) }
            .require_model(&first)
    };
    let mixed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build().require_model(&second)
    }));
    assert!(mixed.is_err());
    let install = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build().prepare(&mut second_app.world)
    }));
    assert!(install.is_err());
}

#[test]
fn query_reborrow_and_repeated_prepared_invocation_do_not_allocate() {
    let mut world = World::new();
    let node = world.spawn_empty();
    world.insert(node, Position(0));
    type Rows = QueryParam<ComponentDriver<Position>, Write<Position>, ()>;
    const ACCESS: &[SystemAccess] = &[SystemAccess::write_component::<Position>()];
    // SAFETY: the sole mutable query matches ACCESS; no other World access
    // occurs while the fetched row lives.
    let callback = unsafe {
        event_callback::<Changed, EntityContext<Widget>, (Rows,), ()>(
            "allocation",
            ACCESS,
            |(mut rows,), payload, target| {
                rows.get(target).unwrap().0 += payload.0;
            },
        )
    }
    .prepare(&mut world);
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..64 {
                let snapshot = callback.clone();
                assert_eq!(snapshot.invoke(&mut world, &Changed(1), node), Some(()));
            }
        }),
        0
    );
    assert_eq!(world.get::<Position>(node), Some(&Position(64)));
}

#[test]
fn panic_releases_invocation_borrows_and_preserves_prepared_callback() {
    let mut world = World::new();
    world.insert_resource(Settings(0));
    const ACCESS: &[SystemAccess] = &[SystemAccess::write_resource::<Settings>()];
    // SAFETY: the sole mutable resource matches ACCESS.
    let callback = unsafe {
        event_callback::<Key, AppContext, (ResMut<Settings>,), bool>(
            "panic",
            ACCESS,
            |(settings,), payload, ()| {
                settings.0 += 1;
                assert!(!payload.pressed, "body failure");
                false
            },
        )
    }
    .prepare(&mut world);
    let mut payload = Key {
        code: 1,
        pressed: true,
    };
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        callback.invoke(&mut world, &payload, ())
    }));
    assert!(error.is_err());
    payload.pressed = false;
    assert_eq!(
        callback.invoke(&mut world, &payload, ()),
        Some(BubbleControl::Allow)
    );
    assert_eq!(world.resource::<Settings>().unwrap().0, 2);
}
