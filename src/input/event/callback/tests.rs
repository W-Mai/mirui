use super::*;
use crate::core::model::ModelHandle;
use crate::core::reactive::{Computed, Signal};
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
            |(clock, settings, missing, mut rows), payload, target, _scope| {
                assert!(missing.is_none());
                settings.0 += clock.0;
                let position = rows.get(target).unwrap();
                position.0 += payload.0;
            },
        )
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    assert_eq!(callback.invoke(&mut world, &Changed(4), node), Ok(Some(())));
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
        event_callback::<Key, AppContext, Params, bool>(
            "conflict",
            ACCESS,
            move |_, _, (), _scope| {
                seen.set(true);
                true
            },
        )
    };
    let error = callback
        .prepare(&mut world, CommandCapacity::default())
        .err()
        .unwrap();
    assert_eq!(
        error,
        PreparationError::Access {
            callback: "conflict",
            error: AccessError::Conflict {
                type_name: core::any::type_name::<Missing>()
            },
        }
    );
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
            |(first, second, rows), _, target, _scope| {
                assert!(core::ptr::eq(first, second));
                assert_eq!(rows.get(target), Some(&Position(3)));
            },
        )
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    assert_eq!(callback.invoke(&mut world, &Changed(0), node), Ok(Some(())));
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
            |_, _, _, _scope| panic!("stale callback invoked"),
        )
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    assert_eq!(callback.invoke(&mut world, &Changed(0), old), Ok(None));
}

#[test]
fn installed_owner_is_checked_before_body_execution() {
    let mut first = World::new();
    let mut second = World::new();
    // SAFETY: the empty plan exactly describes the empty parameter set.
    let callback = unsafe {
        event_callback::<Key, AppContext, (), bool>("owner", &[], |(), _, (), _scope| {
            panic!("cross-owner body invoked")
        })
    }
    .prepare(&mut first, CommandCapacity::default())
    .unwrap();
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
        event_callback::<Owned, AppContext, (), ()>("owned", &[], move |(), payload, (), _scope| {
            assert_eq!(payload.0.as_ptr() as usize, address);
        })
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..64 {
                let snapshot = callback.clone();
                assert_eq!(snapshot.invoke(&mut world, &payload, ()), Ok(Some(())));
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
        event_callback::<Key, AppContext, (), bool>("key", &[], |(), input, (), _scope| {
            input.pressed
        })
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    let mut payload = Key {
        code: 1,
        pressed: true,
    };
    assert_eq!(
        callback.invoke(&mut world, &payload, ()),
        Ok(Some(BubbleControl::Prevent))
    );
    payload.pressed = false;
    assert_eq!(
        callback.invoke(&mut world, &payload, ()),
        Ok(Some(BubbleControl::Allow))
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
        event_callback::<Key, AppContext, (), ()>("model", &[], move |(), _, (), _scope| {
            captured.increment();
        })
    }
    .require_binding(&first)
    .prepare(&mut app.world, CommandCapacity::default())
    .unwrap();
    let payload = Key {
        code: 1,
        pressed: true,
    };
    assert_eq!(
        callback.invoke(&mut app.world, &payload, ()),
        Ok(Some(BubbleControl::Prevent))
    );
    assert_eq!(first.value(), 2);
    assert_eq!(second.value(), 8);
}

#[test]
fn shared_capture_preparation_and_first_repeated_invocation_do_not_allocate() {
    let mut app = crate::app::App::headless(32, 32);
    let model = app.add_model(Counter { value: 0 });
    let nested = Some(Some(model.clone()));
    let missing: Option<CounterHandle> = None;
    let signal = Signal::new(5_u32);
    let computed = {
        let _owner = crate::core::reactive::OwnerGuard::enter(&mut app.world);
        Computed::new(|| 7_u32)
    };
    let captured = nested.clone();
    let captured_signal = signal.clone();
    let captured_computed = computed.clone();
    // SAFETY: Params is empty and every captured binding is declared before
    // preparation. The body only uses its shared binding facades.
    let callback = unsafe {
        event_callback::<Key, AppContext, (), ()>("shared", &[], move |(), _, (), _scope| {
            assert_eq!(captured_signal.get(), 5);
            assert_eq!(captured_computed.get(), 7);
            captured.as_ref().unwrap().as_ref().unwrap().increment();
        })
    };
    let mut prepared = None;
    assert_eq!(
        tracked_allocations(|| {
            prepared = Some(
                callback
                    .require_binding(&nested)
                    .require_binding(&missing)
                    .require_binding(&signal)
                    .require_binding(&computed)
                    .prepare(&mut app.world, CommandCapacity::new())
                    .unwrap(),
            );
        }),
        0
    );
    let prepared = prepared.unwrap();
    let payload = Key {
        code: 1,
        pressed: true,
    };
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                prepared.invoke(&mut app.world, &payload, ()),
                Ok(Some(BubbleControl::Prevent))
            );
        }),
        0
    );
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..64 {
                assert_eq!(
                    prepared.invoke(&mut app.world, &payload, ()),
                    Ok(Some(BubbleControl::Prevent))
                );
            }
        }),
        0
    );
    assert_eq!(model.value(), 65);
}

#[test]
fn model_capture_rejects_mixed_owners_and_wrong_installation_owner() {
    let mut first_app = crate::app::App::headless(32, 32);
    let mut second_app = crate::app::App::headless(32, 32);
    let first = first_app.add_model(Counter { value: 1 });
    let second = second_app.add_model(Counter { value: 2 });
    let build = || {
        // SAFETY: the body captures no Models or World and Params is empty.
        unsafe { event_callback::<Key, AppContext, (), ()>("owner", &[], |(), _, (), _scope| ()) }
            .require_binding(&Some(Some(first.clone())))
    };
    let mixed = build().require_binding(&Some(second.clone()));
    assert_eq!(
        mixed
            .prepare(&mut first_app.world, CommandCapacity::default())
            .err()
            .unwrap(),
        PreparationError::Owner {
            callback: "owner",
            error: ModelOwnerError::MixedOwners
        },
    );
    assert_eq!(
        build()
            .prepare(&mut second_app.world, CommandCapacity::default())
            .err()
            .unwrap(),
        PreparationError::Owner {
            callback: "owner",
            error: ModelOwnerError::ForeignOwner
        },
    );
}

#[test]
fn ended_model_capture_returns_an_owner_error_without_allocating_or_running_body() {
    for retain_cell in [false, true] {
        let (model, retained) = {
            let mut app = crate::app::App::headless(32, 32);
            let model = app.add_model(Counter { value: 1 });
            let retained = retain_cell.then(|| model.cell().upgrade().unwrap());
            (model, retained)
        };
        let mut world = World::new();
        let nested = Some(Some(model));
        // SAFETY: the body has no borrowed parameters or raw World access.
        let callback = unsafe {
            event_callback::<Key, AppContext, (), ()>("ended", &[], |(), _, (), _scope| {
                panic!("ended model callback reached its body")
            })
        };
        assert_eq!(
            tracked_allocations(|| {
                let error = callback
                    .require_binding(&nested)
                    .prepare(&mut world, CommandCapacity::new())
                    .err()
                    .unwrap();
                assert_eq!(
                    error,
                    PreparationError::Owner {
                        callback: "ended",
                        error: ModelOwnerError::RegistrationEnded,
                    }
                );
            }),
            0
        );
        drop(retained);
    }
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
            |(mut rows,), payload, target, _scope| {
                rows.get(target).unwrap().0 += payload.0;
            },
        )
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..64 {
                let snapshot = callback.clone();
                assert_eq!(snapshot.invoke(&mut world, &Changed(1), node), Ok(Some(())));
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
            |(settings,), payload, (), _scope| {
                settings.0 += 1;
                assert!(!payload.pressed, "body failure");
                false
            },
        )
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
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
        Ok(Some(BubbleControl::Allow))
    );
    assert_eq!(world.resource::<Settings>().unwrap().0, 2);
}

#[test]
fn deferred_body_reads_live_parameters_before_ordered_commit_without_allocating() {
    let mut world = World::new();
    world.insert_resource(Settings(0));
    const ACCESS: &[SystemAccess] = &[SystemAccess::write_resource::<Settings>()];
    // SAFETY: ACCESS describes the only live mutable resource. Queued updates
    // do not borrow it until the body has released its parameter.
    let callback = unsafe {
        deferred_event_callback::<Key, AppContext, (ResMut<Settings>,), bool>(
            "deferred",
            ACCESS,
            |(settings,), _, (), _scope, mut commands| {
                settings.0 = 1;
                commands.apply_resource(4i32, |world, value| {
                    world.resource_mut::<Settings>().unwrap().0 += value;
                });
                commands.apply_resource(3i32, |world, value| {
                    world.resource_mut::<Settings>().unwrap().0 *= value;
                });
                assert_eq!(settings.0, 1);
                true
            },
        )
    }
    .prepare(&mut world, CommandCapacity::new().resource_value::<i32>(2))
    .unwrap();
    let payload = Key {
        code: 1,
        pressed: true,
    };
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..32 {
                let snapshot = callback.clone();
                assert_eq!(
                    snapshot.invoke(&mut world, &payload, ()),
                    Ok(Some(BubbleControl::Prevent))
                );
            }
        }),
        0
    );
    assert_eq!(world.resource::<Settings>().unwrap().0, 15);
}

#[test]
fn deferred_component_removal_runs_after_query_reborrow_ends() {
    let mut world = World::new();
    let node = world.spawn_empty();
    world.insert(node, Position(7));
    type Rows = QueryParam<ComponentDriver<Position>, Read<Position>, ()>;
    const ACCESS: &[SystemAccess] = &[SystemAccess::read_component::<Position>()];
    // SAFETY: the only live query read matches ACCESS; component removal is
    // deferred and cannot invalidate it while the authored body runs.
    let callback = unsafe {
        deferred_event_callback::<Changed, EntityContext<Widget>, (Rows,), ()>(
            "remove",
            ACCESS,
            |(rows,), _, target, _scope, mut commands| {
                let position = rows.get(target).unwrap();
                commands.remove::<Position>(target);
                assert_eq!(position.0, 7);
                assert_eq!(rows.get(target), Some(&Position(7)));
            },
        )
    }
    .prepare(&mut world, CommandCapacity::new().remove::<Position>(1))
    .unwrap();
    assert_eq!(callback.invoke(&mut world, &Changed(0), node), Ok(Some(())));
    assert!(!world.has::<Position>(node));
}

#[test]
fn deferred_capacity_error_does_not_commit_partial_commands_or_rollback_resource_values() {
    let mut world = World::new();
    world.insert_resource(Settings(0));
    const ACCESS: &[SystemAccess] = &[SystemAccess::write_resource::<Settings>()];
    // SAFETY: the declared resource is only borrowed during the body; commands
    // cannot access it before the body returns.
    let callback = unsafe {
        deferred_event_callback::<Key, AppContext, (ResMut<Settings>,), bool>(
            "capacity",
            ACCESS,
            |(settings,), payload, (), _scope, mut commands| {
                settings.0 += 1;
                commands.apply_resource(100i32, |world, value| {
                    world.resource_mut::<Settings>().unwrap().0 += value;
                });
                if payload.pressed {
                    commands.remove_resource::<Settings>();
                }
                false
            },
        )
    }
    .prepare(&mut world, CommandCapacity::new().resource_value::<i32>(1))
    .unwrap();
    let mut payload = Key {
        code: 1,
        pressed: true,
    };
    assert_eq!(
        callback.invoke(&mut world, &payload, ()),
        Err(CommandError::RecordCapacity { available: 1 })
    );
    assert_eq!(world.resource::<Settings>().unwrap().0, 1);
    payload.pressed = false;
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                callback.invoke(&mut world, &payload, ()),
                Ok(Some(BubbleControl::Allow))
            );
        }),
        0
    );
    assert_eq!(world.resource::<Settings>().unwrap().0, 102);
}

#[test]
fn deferred_snapshot_shares_storage_and_reports_busy_without_panicking() {
    let mut world = World::new();
    // SAFETY: the empty plan matches Params; the body captures no Models.
    let callback = unsafe {
        deferred_event_callback::<Key, AppContext, (), bool>("busy", &[], |(), _, (), _scope, _| {
            false
        })
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    let snapshot = callback.clone();
    let CallbackRun::Deferred {
        commands: queue, ..
    } = &callback.run
    else {
        panic!("deferred callback expected");
    };
    let CallbackRun::Deferred {
        commands: snapshot_queue,
        ..
    } = &snapshot.run
    else {
        panic!("deferred callback expected");
    };
    assert!(Rc::ptr_eq(queue, snapshot_queue));
    let guard = queue.borrow_mut();
    let payload = Key {
        code: 1,
        pressed: true,
    };
    assert_eq!(
        snapshot.invoke(&mut world, &payload, ()),
        Err(CommandError::Busy)
    );
    drop(guard);
    assert_eq!(
        snapshot.invoke(&mut world, &payload, ()),
        Ok(Some(BubbleControl::Allow))
    );
    // Plain callbacks do not allocate or retain command storage.
    let plain = unsafe {
        event_callback::<Key, AppContext, (), bool>("plain", &[], |(), _, (), _scope| false)
    }
    .prepare(&mut world, CommandCapacity::default())
    .unwrap();
    assert!(matches!(plain.run, CallbackRun::Plain(_)));
}

struct OwnedCommand(Rc<Cell<usize>>);
impl Drop for OwnedCommand {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn deferred_body_and_apply_panic_drop_pending_values_and_leave_executor_reusable() {
    for fail_in_body in [true, false] {
        let mut world = World::new();
        let drops = Rc::new(Cell::new(0));
        let captured = Rc::clone(&drops);
        // SAFETY: there are no borrowed parameters. Apply receives World only
        // after the callback body has returned; payloads are always consumed.
        let callback = unsafe {
            deferred_event_callback::<Key, AppContext, (), bool>(
                "unwind",
                &[],
                move |(), payload, (), _scope, mut commands| {
                    if !payload.pressed {
                        return false;
                    }
                    commands.apply_resource(OwnedCommand(captured.clone()), |_, value| {
                        drop(value);
                        panic!("apply failure");
                    });
                    commands.apply_resource(OwnedCommand(captured.clone()), |_, value| drop(value));
                    assert!(!fail_in_body, "body failure");
                    true
                },
            )
        }
        .prepare(
            &mut world,
            CommandCapacity::new().resource_value::<OwnedCommand>(2),
        )
        .unwrap();
        let mut payload = Key {
            code: 1,
            pressed: true,
        };
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback.invoke(&mut world, &payload, ())
        }));
        assert!(error.is_err());
        assert_eq!(drops.get(), 2);
        payload.pressed = false;
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    callback.invoke(&mut world, &payload, ()),
                    Ok(Some(BubbleControl::Allow))
                );
            }),
            0
        );
    }
}

#[test]
fn invalid_command_budget_is_rejected_during_installation() {
    let mut world = World::new();
    // SAFETY: the empty plan matches Params and the body captures no Models.
    let callback = unsafe {
        deferred_event_callback::<Key, AppContext, (), bool>(
            "budget",
            &[],
            |(), _, (), _scope, _| false,
        )
    };
    assert!(matches!(
        callback.prepare(&mut world, CommandCapacity::new().despawn(usize::MAX)),
        Err(PreparationError::Command {
            error: CommandError::SizeOverflow,
            ..
        })
    ));
}
