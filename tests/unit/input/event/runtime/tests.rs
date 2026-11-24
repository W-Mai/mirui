use super::*;
use crate::input::event::callback::NodeContext;
use crate::input::event::contract::{EntityNotification, WidgetSource};
use crate::input::event::focus::FocusState;
use crate::input::event::focus::Focusable;
use crate::input::event::gesture::{DragStart, GestureEvent, Tap};
use crate::input::event::lifecycle::{Resumed, Started};
use crate::input::event::multi_tap::{MultiTapEntry, MultiTapTracker};
use crate::test_support::tracked_allocations;
use crate::types::Fixed;
use crate::ui::delivery::{dispatch_gesture, flush, notify_app};
use crate::ui::{Hidden, Parent, UserState};

#[derive(Default)]
struct Trace {
    values: [u32; 8],
    len: usize,
}

impl Trace {
    fn push(&mut self, value: u32) {
        self.values[self.len] = value;
        self.len += 1;
    }
}

#[crate::component]
#[derive(Debug, PartialEq, Eq)]
struct Marker(u32);

#[crate::event]
fn key(input: event!(Key), trace: res!(mut Trace)) -> bool {
    trace.push(input.code);
    true
}

#[crate::event]
fn deferred(input: event!(Key), target: Entity, commands: commands!()) -> bool {
    commands.insert(target, Marker(input.code));
    true
}

#[crate::event]
fn started(trace: res!(mut Trace)) {
    trace.push(99);
}

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Trace::default());
    initialize(&mut world);
    world
}

#[crate::event]
fn tap_trace(input: event!(Tap), trace: res!(mut Trace)) -> bool {
    trace.push(input.target.id);
    false
}

fn tap(target: Entity) -> GestureEvent {
    Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target,
    }
    .into()
}

#[test]
fn gesture_cycles_fail_before_focus_tracker_or_callbacks_and_leave_runtime_reusable() {
    let mut world = world();
    let parent = world.spawn_empty();
    let target = world.spawn_empty();
    world.insert(target, Focusable);
    world.insert(target, Parent(parent));
    world.insert_resource(FocusState {
        focused: Some(parent),
    });
    world.insert_resource(MultiTapTracker {
        last: Some(MultiTapEntry {
            entity: parent,
            time_ms: 10,
            count: 2,
        }),
    });
    install_entity(
        &mut world,
        target,
        Tap::EVENT,
        tap_trace::callback::<NodeContext>(),
    )
    .unwrap();
    world.insert(parent, Parent(target));
    let events = [
        tap(target),
        DragStart {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        }
        .into(),
    ];
    for flag in 0..4 {
        match flag {
            1 => {
                world.insert(parent, Hidden);
            }
            2 => {
                world.insert(parent, UserState::Disabled);
            }
            3 => {
                world.insert(
                    parent,
                    crate::ui::branch::CachedBranchVisibility { selected: false },
                );
            }
            _ => {}
        }
        assert_eq!(
            tracked_allocations(|| {
                for event in &events {
                    assert!(matches!(
                        dispatch_gesture(&mut world, event, 100),
                        Err(ListenerError::CyclicRoute { .. })
                    ));
                    assert_eq!(
                        world.resource::<FocusState>().unwrap().focused,
                        Some(parent)
                    );
                    let previous = world.resource::<MultiTapTracker>().unwrap().last.unwrap();
                    assert_eq!(
                        (previous.entity, previous.time_ms, previous.count),
                        (parent, 10, 2)
                    );
                    assert_eq!(world.resource::<Trace>().unwrap().len, 0);
                }
            }),
            0
        );
        world.remove::<Hidden>(parent);
        world.remove::<UserState>(parent);
        world.remove::<crate::ui::branch::CachedBranchVisibility>(parent);
    }
    world.remove::<Parent>(parent);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                dispatch_gesture(&mut world, &tap(target), 200),
                Ok(super::super::BubbleControl::Allow)
            );
        }),
        0
    );
    assert_eq!(
        world.resource::<FocusState>().unwrap().focused,
        Some(target)
    );
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!(&trace.values[..trace.len], &[target.id]);
}

#[test]
fn gesture_focus_and_tracker_preserve_disabled_and_nearest_ancestor_semantics() {
    let mut world = world();
    let parent = world.spawn_empty();
    world.insert(parent, Focusable);
    let target = world.spawn_empty();
    world.insert(target, Parent(parent));
    world.insert_resource(FocusState {
        focused: Some(parent),
    });
    world.insert_resource(MultiTapTracker::default());
    install_entity(
        &mut world,
        target,
        Tap::EVENT,
        tap_trace::callback::<NodeContext>(),
    )
    .unwrap();
    world.insert(parent, UserState::Disabled);
    assert_eq!(
        dispatch_gesture(&mut world, &tap(target), 100),
        Ok(super::super::BubbleControl::Allow)
    );
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    assert_eq!(world.resource::<Trace>().unwrap().len, 0);
    let entry = world.resource::<MultiTapTracker>().unwrap().last.unwrap();
    assert_eq!((entry.entity, entry.time_ms, entry.count), (target, 100, 1));
    world.remove::<UserState>(parent);
    assert_eq!(
        dispatch_gesture(&mut world, &tap(target), 200),
        Ok(super::super::BubbleControl::Allow)
    );
    assert_eq!(
        world.resource::<FocusState>().unwrap().focused,
        Some(parent)
    );
    world.insert(target, Focusable);
    assert_eq!(
        dispatch_gesture(&mut world, &tap(target), 300),
        Ok(super::super::BubbleControl::Allow)
    );
    assert_eq!(
        world.resource::<FocusState>().unwrap().focused,
        Some(target)
    );
    world.insert(target, UserState::Disabled);
    assert_eq!(
        dispatch_gesture(&mut world, &tap(target), 400),
        Ok(super::super::BubbleControl::Allow)
    );
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    assert_eq!(world.resource::<Trace>().unwrap().len, 2);
    let entry = world.resource::<MultiTapTracker>().unwrap().last.unwrap();
    assert_eq!((entry.entity, entry.time_ms, entry.count), (target, 400, 4));
    world.remove::<UserState>(target);
    world.remove::<Focusable>(target);
    world.remove::<Focusable>(parent);
    world.resource_mut::<FocusState>().unwrap().focused = Some(parent);
    let drag: GestureEvent = DragStart {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target,
    }
    .into();
    dispatch_gesture(&mut world, &drag, 500).unwrap();
    assert_eq!(
        world.resource::<FocusState>().unwrap().focused,
        Some(parent)
    );
    assert_eq!(
        world
            .resource::<MultiTapTracker>()
            .unwrap()
            .last
            .unwrap()
            .time_ms,
        400
    );
    dispatch_gesture(&mut world, &tap(target), 600).unwrap();
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
}

#[test]
fn gesture_stale_generations_stop_at_the_live_prefix_without_following_reused_nodes() {
    let mut world = world();
    let parent = world.spawn_empty();
    world.insert(parent, Focusable);
    let target = world.spawn_empty();
    world.insert(target, Parent(parent));
    world.insert_resource(FocusState {
        focused: Some(parent),
    });
    world.insert_resource(MultiTapTracker::default());
    install_entity(
        &mut world,
        target,
        Tap::EVENT,
        tap_trace::callback::<NodeContext>(),
    )
    .unwrap();
    world.despawn(parent);
    let replacement = world.spawn_empty();
    world.insert(replacement, Focusable);
    assert_eq!(replacement.id, parent.id);
    assert_ne!(replacement.generation, parent.generation);
    world.insert(replacement, UserState::Disabled);
    assert_eq!(
        tracked_allocations(|| {
            for now in [100, 200] {
                assert_eq!(
                    dispatch_gesture(&mut world, &tap(target), now),
                    Ok(super::super::BubbleControl::Allow)
                );
            }
        }),
        0
    );
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!(&trace.values[..trace.len], &[target.id, target.id]);
    world.despawn(target);
    let replacement = world.spawn_empty();
    world.insert(replacement, Focusable);
    assert_eq!(replacement.id, target.id);
    world.resource_mut::<FocusState>().unwrap().focused = Some(replacement);
    dispatch_gesture(&mut world, &tap(target), 300).unwrap();
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    assert_eq!(world.resource::<Trace>().unwrap().len, 2);
    let entry = world.resource::<MultiTapTracker>().unwrap().last.unwrap();
    assert_eq!((entry.entity, entry.time_ms, entry.count), (target, 300, 3));
}

#[test]
fn first_successful_binding_freezes_configuration_but_failed_binding_does_not() {
    let mut world = world();
    configure(&mut world, EventCapacity::new()).unwrap();
    let target = world.spawn(Marker(0));
    assert_eq!(
        install_app(&mut world, Key::EVENT, deferred::callback(target)),
        Err(RuntimeInstallError::MissingCommandCapacity)
    );
    let capacity = EventCapacity::new().commands(CommandCapacity::new().insert::<Marker>(1));
    configure(&mut world, capacity).unwrap();
    install_app(&mut world, Key::EVENT, deferred::callback(target)).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            configure(&mut world, capacity).unwrap();
            assert_eq!(
                configure(
                    &mut world,
                    EventCapacity::new().commands(CommandCapacity::new().insert::<Marker>(2))
                ),
                Err(EventCapacityError::FrozenConfiguration)
            );
        }),
        0
    );
    assert_eq!(
        dispatch_input(
            &mut world,
            None,
            &InputEvent::Key {
                physical: None,
                code: 7,
                pressed: true,
                modifiers: Some(Default::default()),
                repeat: Some(false),
            },
            32,
            32,
        ),
        Ok(true)
    );
    assert_eq!(world.get::<Marker>(target), Some(&Marker(7)));
}

#[test]
fn plain_app_input_needs_neither_root_nor_business_capacity() {
    let mut world = world();
    install_app(&mut world, Key::EVENT, key::callback()).unwrap();
    prepare(&world).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            configure(&mut world, EventCapacity::default()).unwrap();
            let runtime = runtime(&world).unwrap();
            assert!(
                super::super::emission::flush_notifications(&mut world, &runtime.listeners, 0,)
                    .is_none()
            );
            for code in [1, 2] {
                assert_eq!(
                    dispatch_input(
                        &mut world,
                        None,
                        &InputEvent::Key {
                            physical: None,
                            code,
                            pressed: true,
                            modifiers: Some(Default::default()),
                            repeat: Some(false),
                        },
                        0,
                        0,
                    ),
                    Ok(true)
                );
            }
            assert_eq!(
                dispatch_input(
                    &mut world,
                    None,
                    &InputEvent::PointerDown {
                        kind: crate::input::event::input::PointerKind::Mouse,
                        id: 0,
                        x: Fixed::ZERO,
                        y: Fixed::ZERO,
                    },
                    0,
                    0,
                ),
                Ok(false)
            );
        }),
        0
    );
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!(&trace.values[..trace.len], &[1, 2]);
}

#[test]
fn structural_unit_slot_is_prepared_during_installation_and_reused_without_growth() {
    let mut world = world();
    install_app(&mut world, Started::EVENT, started::callback()).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..2 {
                assert_eq!(notify_app(&mut world, Started), Ok(1));
            }
        }),
        0
    );
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!(&trace.values[..trace.len], &[99, 99]);
}

#[test]
fn unmatched_app_notices_do_not_allocate_or_occupy_the_structural_slot() {
    let mut world = world();
    install_app(&mut world, Key::EVENT, key::callback()).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(notify_app(&mut world, Started), Ok(0));
        }),
        0
    );
    install_app(&mut world, Started::EVENT, started::callback()).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(notify_app(&mut world, Resumed), Ok(0));
            assert_eq!(notify_app(&mut world, Started), Ok(1));
        }),
        0
    );
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!(&trace.values[..trace.len], &[99]);
}

#[crate::event]
fn editing(node: entity!(), _key: event!(Key), trace: res!(mut Trace)) -> bool {
    trace.push(40 + node.id);
    false
}

#[test]
fn structural_flush_prepares_without_notification_service_and_preserves_route_order() {
    let mut world = world();
    install_app(&mut world, Key::EVENT, key::callback()).unwrap();
    let parent = world.spawn_empty();
    let child = world.spawn_empty();
    world.insert(parent, Marker(0));
    world.insert(child, Marker(0));
    let mut registry = ViewRegistry::default();
    registry.insert(
        crate::ui::view::View::systems_only("editor", &[])
            .with_filter::<Marker>()
            .with_inputs(&mut world, editing::callback()),
    );
    world.insert_resource(registry);
    world.insert(child, crate::ui::Parent(parent));
    crate::ui::lifecycle::attach_handlers_for(&mut world, parent);
    crate::ui::lifecycle::attach_handlers_for(&mut world, child);
    world.insert_resource(FocusState {
        focused: Some(child),
    });
    let input = InputEvent::Key {
        physical: None,
        code: 7,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    assert_eq!(
        dispatch_input(&mut world, None, &input, 0, 0),
        Err(ListenerError::SnapshotCapacity { available: 1 })
    );
    assert_eq!(world.resource::<Trace>().unwrap().len, 0);
    assert_eq!(flush(&mut world), Ok(0));
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(dispatch_input(&mut world, None, &input, 0, 0), Ok(true));
        }),
        0
    );
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!(
        &trace.values[..trace.len],
        &[40 + child.id, 40 + parent.id, 7]
    );
}

#[test]
fn planned_input_preparation_preserves_busy_and_capacity_errors() {
    use super::super::view_input::InputState;
    let mut world = world();
    let view = crate::ui::view::View::systems_only("editor", &[])
        .with_filter::<Marker>()
        .with_inputs(&mut world, editing::callback());
    let inputs = view.inputs().unwrap().clone();
    let mut registry = ViewRegistry::default();
    registry.insert(view);
    world.insert_resource(registry);
    assert_eq!(prepare(&world), Ok(()));
    let borrowed = inputs.borrow_mut();
    assert_eq!(prepare(&world), Err(ListenerError::Busy));
    drop(borrowed);

    for _ in 0..2 {
        let entity = world.spawn_empty();
        world.insert(entity, Marker(0));
    }
    let original = core::mem::replace(&mut *inputs.borrow_mut(), InputState::Failed);
    assert_eq!(prepare(&world), Ok(()));
    *inputs.borrow_mut() =
        InputState::Pending(|_| panic!("capacity preparation must not install callbacks"));
    assert_eq!(prepare(&world), Ok(()));
    *inputs.borrow_mut() = original;
    {
        let InputState::Ready(prepared) = &mut *inputs.borrow_mut() else {
            panic!("prepared inputs");
        };
        prepared.max_dispatch = usize::MAX;
    }
    assert_eq!(prepare(&world), Err(ListenerError::SizeOverflow));
}

#[test]
fn mandatory_suspend_cancellation_precedes_busy_route_preparation() {
    let mut world = world();
    install_app(&mut world, Started::EVENT, started::callback()).unwrap();
    world.insert_resource(super::super::PointerCursor {
        kind: crate::surface::PointerKind::Mouse,
        id: 0,
        present: true,
        hover_capable: true,
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        down: true,
        event_seq: 5,
    });
    let runtime = runtime(&world).unwrap();
    let snapshot = runtime
        .listeners
        .snapshot(&world, &Started, ListenerTarget::App)
        .unwrap();
    assert_eq!(prepare(&world), Err(ListenerError::Busy));
    super::super::dispatch_input(&mut world, None, &InputEvent::AppSuspend, 0, 0, 0);
    let cursor = world.resource::<super::super::PointerCursor>().unwrap();
    assert!(!cursor.down);
    assert_eq!(cursor.event_seq, 6);
    drop(snapshot);
    assert_eq!(prepare(&world), Ok(()));
}

#[crate::component]
struct Pad;

struct Hit(u32);

impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

#[crate::event]
fn hit(input: event!(Hit), trace: res!(mut Trace)) {
    trace.push(input.0);
}

#[test]
fn default_storage_is_lazy_and_later_binding_preserves_business_capacity() {
    let mut world = world();
    let target = world.spawn(Pad);
    install_app(&mut world, Key::EVENT, key::callback()).unwrap();
    assert!(!has_business_storage(&world));
    install_app(&mut world, Started::EVENT, started::callback()).unwrap();
    assert!(!has_business_storage(&world));
    install_entity(
        &mut world,
        target,
        Hit::EVENT,
        hit::callback::<NodeContext>(),
    )
    .unwrap();
    assert!(has_business_storage(&world));
    install_entity(
        &mut world,
        target,
        Tap::EVENT,
        tap_trace::callback::<NodeContext>(),
    )
    .unwrap();
    install_app(&mut world, Resumed::EVENT, started::callback()).unwrap();
    assert!(has_business_storage(&world));
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..2 {
                world.resource_mut::<Trace>().unwrap().len = 0;
                for n in 0..8 {
                    crate::input::event::emission::submit_if_listened(&mut world, target, Hit(n))
                        .unwrap();
                }
                assert!(matches!(
                    crate::input::event::emission::submit_if_listened(&mut world, target, Hit(9)),
                    Err(CommandError::Commit(
                        crate::ecs::CommandCommitError::RecordCapacity { available: 8 }
                    ))
                ));
                assert_eq!(flush(&mut world), Ok(8));
                let trace = world.resource::<Trace>().unwrap();
                assert_eq!(&trace.values[..trace.len], &[0, 1, 2, 3, 4, 5, 6, 7]);
            }
        }),
        0
    );
}

#[test]
fn default_commands_install_without_configuration_and_reuse_bounded_storage() {
    let mut world = world();
    let target = world.spawn(Marker(0));
    install_app(&mut world, Key::EVENT, deferred::callback(target)).unwrap();
    assert!(has_business_storage(&world));
    assert_eq!(
        tracked_allocations(|| {
            for code in [3, 7] {
                assert_eq!(
                    dispatch_input(
                        &mut world,
                        None,
                        &InputEvent::Key {
                            physical: None,
                            code,
                            pressed: true,
                            modifiers: Some(Default::default()),
                            repeat: Some(false),
                        },
                        0,
                        0
                    ),
                    Ok(true)
                );
                assert_eq!(world.get::<Marker>(target), Some(&Marker(code)));
                assert_eq!(flush(&mut world), Ok(0));
            }
        }),
        0
    );
    let capacity = runtime(&world)
        .unwrap()
        .configuration
        .get()
        .limits()
        .commands
        .unwrap();
    let mut queue = crate::ecs::commands::CommandQueue::bounded(capacity).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..2 {
                let mut batch = queue.begin();
                for _ in 0..=capacity.record_count() {
                    batch.commands().insert(target, Marker(1));
                }
                assert_eq!(
                    batch.finish(&mut world),
                    Err(CommandError::RecordCapacity {
                        available: capacity.record_count(),
                    })
                );
                assert_eq!(world.get::<Marker>(target), Some(&Marker(7)));
            }
        }),
        0
    );
}

#[test]
fn explicit_small_notification_capacity_is_not_replaced_by_preset() {
    let mut world = world();
    let target = world.spawn(Pad);
    configure(
        &mut world,
        EventCapacity::new().notifications::<Hit>(1).flush_limit(1),
    )
    .unwrap();
    install_entity(
        &mut world,
        target,
        Hit::EVENT,
        hit::callback::<NodeContext>(),
    )
    .unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..2 {
                crate::input::event::emission::submit_if_listened(&mut world, target, Hit(1))
                    .unwrap();
                assert!(matches!(
                    crate::input::event::emission::submit_if_listened(&mut world, target, Hit(2)),
                    Err(CommandError::Commit(
                        crate::ecs::CommandCommitError::RecordCapacity { available: 1 }
                    ))
                ));
                assert_eq!(flush(&mut world), Ok(1));
            }
        }),
        0
    );
}

#[crate::event]
fn feedback(input: event!(Hit), node: entity!(), commands: commands!()) {
    commands.emit(node, Hit(input.0 + 1));
}

#[test]
fn default_feedback_limit_discards_pending_writes_and_storage_remains_reusable() {
    let mut world = world();
    let target = world.spawn(Pad);
    install_entity(
        &mut world,
        target,
        Hit::EVENT,
        feedback::callback::<NodeContext>(),
    )
    .unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..2 {
                crate::input::event::emission::submit_if_listened(&mut world, target, Hit(0))
                    .unwrap();
                assert_eq!(
                    flush(&mut world),
                    Err(RuntimeRunError::Notification(
                        NotificationFlushError::Storage(NotificationError::FlushLimit {
                            limit: 16
                        })
                    ))
                );
                assert_eq!(flush(&mut world), Ok(0));
            }
        }),
        0
    );
}

struct Oversized([u8; 1024]);

#[repr(align(64))]
struct Aligned(u8);

macro_rules! custom_events {
        ($($event:ty),+ $(,)?) => {
            $(impl Event for $event {
                type Route = EntityNotification;
                type Source = WidgetSource<Pad>;
                type Options = ();
            })+
        };
    }
custom_events!(Oversized, Aligned);

#[test]
fn oversized_payloads_fail_before_freezing_and_explicit_layout_allows_installation() {
    let mut world = world();
    let target = world.spawn(Pad);
    let capacity = EventCapacity::default()
        .resolve()
        .unwrap()
        .notifications
        .unwrap();
    assert_eq!(
        install_entity(
            &mut world,
            target,
            Oversized::EVENT,
            Oversized::EVENT.__bind_callback::<NodeContext, _>("large", |value: &Oversized| {
                assert_eq!(value.0[0], 5);
            })
        ),
        Err(RuntimeInstallError::NotificationPayload {
            event: type_name::<Oversized>(),
            error: NotificationError::PayloadCapacity {
                required: core::mem::size_of::<Oversized>(),
                available: capacity.payload_bytes,
            },
        })
    );
    assert_eq!(
        install_entity(
            &mut world,
            target,
            Aligned::EVENT,
            Aligned::EVENT.__bind_callback::<NodeContext, _>("aligned", |value: &Aligned| {
                assert_eq!(value.0, 7);
            })
        ),
        Err(RuntimeInstallError::NotificationPayload {
            event: type_name::<Aligned>(),
            error: NotificationError::PayloadAlignment {
                required: 64,
                available: capacity.payload_alignment,
            },
        })
    );
    assert!(!has_business_storage(&world));
    configure(
        &mut world,
        EventCapacity::new()
            .notifications::<Oversized>(1)
            .notifications::<Aligned>(1)
            .flush_limit(2),
    )
    .unwrap();
    install_entity(
        &mut world,
        target,
        Oversized::EVENT,
        Oversized::EVENT.__bind_callback::<NodeContext, _>("large", |value: &Oversized| {
            assert_eq!(value.0[0], 5);
        }),
    )
    .unwrap();
    install_entity(
        &mut world,
        target,
        Aligned::EVENT,
        Aligned::EVENT.__bind_callback::<NodeContext, _>("aligned", |value: &Aligned| {
            assert_eq!(value.0, 7);
        }),
    )
    .unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..2 {
                crate::input::event::emission::submit_if_listened(
                    &mut world,
                    target,
                    Oversized([5; 1024]),
                )
                .unwrap();
                crate::input::event::emission::submit_if_listened(&mut world, target, Aligned(7))
                    .unwrap();
                assert_eq!(flush(&mut world), Ok(2));
            }
        }),
        0
    );
}

#[test]
fn app_notice_joins_pending_business_notifications_in_the_same_fifo() {
    let mut world = world();
    let target = world.spawn(Pad);
    configure(
        &mut world,
        EventCapacity::new().notifications::<Hit>(1).flush_limit(1),
    )
    .unwrap();
    install_app(&mut world, Started::EVENT, started::callback()).unwrap();
    let runtime = runtime(&world).unwrap();
    runtime.prepare(&world, 1).unwrap();
    runtime
        .listeners
        .install_entity(
            &mut world,
            target,
            ListenerSelector::new(Hit::EVENT, |(), _| true),
            hit::callback::<NodeContext>(),
            CommandCapacity::new(),
        )
        .unwrap();
    let mut commands =
        crate::ecs::commands::CommandQueue::bounded(CommandCapacity::new().emit::<Hit>(1)).unwrap();
    let mut batch = commands.begin();
    batch.commands().emit(target, Hit(7));
    batch.finish(&mut world).unwrap();
    assert_eq!(notify_app(&mut world, Started), Ok(2));
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!(&trace.values[..trace.len], &[7, 99]);
}

#[test]
fn invalid_configuration_and_foreign_owner_do_not_change_live_configuration() {
    let mut world = world();
    let capacity = EventCapacity::new().commands(CommandCapacity::new().insert::<Marker>(1));
    configure(&mut world, capacity).unwrap();
    let runtime = runtime(&world).unwrap();
    let previous = runtime.configuration.get();
    assert_eq!(
        configure(&mut world, EventCapacity::new().notifications::<Hit>(1)),
        Err(EventCapacityError::MissingFlushLimit)
    );
    assert_eq!(runtime.configuration.get(), previous);
    let mut other = World::new();
    other.insert_resource(runtime.clone());
    assert_eq!(
        configure(&mut other, capacity),
        Err(EventCapacityError::ForeignOwner)
    );
    assert_eq!(prepare(&other), Err(ListenerError::ForeignOwner));
    assert_eq!(runtime.configuration.get(), previous);
}
