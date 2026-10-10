use super::*;
use crate::ecs::commands::{CommandCapacity, CommandQueue};
use crate::input::event::callback::{AppContext, NodeContext, PreparedEvent};
use crate::input::event::contract::WidgetSource;
use crate::input::event::gesture::Tap;
use crate::input::event::lifecycle::{Resumed, Started};
use crate::input::event::listeners::{ListenerCapacity, ListenerSelector};
use crate::test_support::tracked_allocations;
use crate::types::Fixed;
use core::any::TypeId;
use core::cell::Cell;
use core::mem::{align_of, size_of};

#[crate::component]
struct Pad {
    strength: u8,
}

struct Hit {
    old: u8,
    new: u8,
}

impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

#[derive(Default)]
struct Trace {
    values: [(u8, u8); 8],
    len: usize,
}

#[test]
fn queued_notifications_do_not_cross_retained_binding_replacement() {
    for ancestor in [false, true] {
        let mut fixture = Fixture::new(notification_capacity::<Hit>(1));
        let root = if ancestor {
            let root = fixture.world.spawn_empty();
            fixture.world.insert(fixture.node, crate::ui::Parent(root));
            root
        } else {
            fixture.node
        };
        let gate =
            crate::core::reactive::retained::RetainedEffects::new(&mut fixture.world, |_, _| true);
        fixture.world.insert(root, gate);
        assert_eq!(
            tracked_allocations(|| {
                submit(&mut fixture.world, fixture.node, Hit { old: 0, new: 1 }).unwrap();
                crate::core::reactive::retained::retire(&mut fixture.world, root);
                fixture
                    .pending
                    .flush(&fixture.listeners, &mut fixture.world, 4)
                    .unwrap();
                assert_eq!(fixture.world.resource::<Trace>().unwrap().len, 0);

                submit(&mut fixture.world, fixture.node, Hit { old: 1, new: 2 }).unwrap();
                fixture
                    .pending
                    .flush(&fixture.listeners, &mut fixture.world, 4)
                    .unwrap();
                let trace = fixture.world.resource::<Trace>().unwrap();
                assert_eq!(trace.len, 1);
                assert_eq!(trace.values[0], (1, 2));
            }),
            0
        );
    }
}

struct BindingValid(bool);

#[crate::event]
fn invalidate_binding(valid: res!(mut BindingValid)) {
    valid.0 = false;
}

#[test]
fn notification_callbacks_recheck_retained_identity_between_listeners() {
    let mut fixture = Fixture::new(notification_capacity::<Hit>(1));
    fixture.world.insert_resource(BindingValid(true));
    let gate =
        crate::core::reactive::retained::RetainedEffects::new(&mut fixture.world, |world, _| {
            world
                .resource::<BindingValid>()
                .is_some_and(|valid| valid.0)
        });
    fixture.world.insert(fixture.node, gate);
    fixture
        .listeners
        .reserve(ListenerCapacity {
            listeners: 3,
            snapshot_steps: 3,
        })
        .unwrap();
    fixture
        .listeners
        .install_entity(
            &mut fixture.world,
            fixture.node,
            ListenerSelector::new(Hit::EVENT, |(), _| true),
            invalidate_binding::callback::<Hit, NodeContext>(),
            CommandCapacity::default(),
        )
        .unwrap();
    fixture
        .listeners
        .install_entity(
            &mut fixture.world,
            fixture.node,
            ListenerSelector::new(Hit::EVENT, |(), _| true),
            record::callback::<NodeContext>(),
            CommandCapacity::default(),
        )
        .unwrap();
    submit(&mut fixture.world, fixture.node, Hit { old: 0, new: 1 }).unwrap();
    fixture
        .pending
        .flush(&fixture.listeners, &mut fixture.world, 4)
        .unwrap();
    assert!(!fixture.world.resource::<BindingValid>().unwrap().0);
    assert_eq!(fixture.world.resource::<Trace>().unwrap().len, 1);
}

impl Trace {
    fn record(&mut self, old: u8, new: u8) {
        self.values[self.len] = (old, new);
        self.len += 1;
    }
}

#[crate::event]
fn hit(
    node: entity!(),
    pads: query!(mut Pad),
    pending: res!(Rc<PendingNotifications>),
    commands: commands!(),
) {
    let pad = pads.get(node).unwrap();
    let old = pad.strength;
    pad.strength += 1;
    commands.emit_always(
        node,
        Hit {
            old,
            new: pad.strength,
        },
    );
    assert_eq!(
        pending
            .queue
            .flush(1, |_| panic!("submitted during a live parameter borrow")),
        Ok::<_, NotificationFlushError<()>>(0)
    );
    assert_eq!(pad.strength, old + 1);
}

#[crate::event]
fn record(input: event!(Hit), trace: res!(mut Trace)) {
    trace.record(input.old, input.new);
}

fn command_capacity<E>(records: usize) -> CommandCapacity
where
    E: Event<Route = EntityNotification>,
    E::Source: EntityEventSource,
{
    CommandCapacity::new().emit::<E>(records)
}

fn notification_capacity<E>(records: usize) -> NotificationCapacity {
    NotificationCapacity {
        records,
        payload_bytes: records * size_of::<E>(),
        payload_alignment: align_of::<E>(),
    }
}

fn install_pending(world: &mut World, capacity: NotificationCapacity) -> Rc<PendingNotifications> {
    prepare_notifications(world, capacity).unwrap();
    world
        .resource::<Rc<PendingNotifications>>()
        .unwrap()
        .clone()
}

struct Fixture {
    world: World,
    node: Entity,
    pending: Rc<PendingNotifications>,
    listeners: EventListeners,
    hit: PreparedEvent<Tap, NodeContext>,
}

impl Fixture {
    fn new(capacity: NotificationCapacity) -> Self {
        let mut world = World::new();
        let node = world.spawn(Pad { strength: 0 });
        world.watch_component_type(TypeId::of::<Pad>());
        world.insert_resource(Trace::default());
        let pending = install_pending(&mut world, capacity);
        let listeners = EventListeners::new(&mut world);
        listeners
            .reserve(ListenerCapacity {
                listeners: 2,
                snapshot_steps: 2,
            })
            .unwrap();
        listeners
            .install_entity(
                &mut world,
                node,
                ListenerSelector::new(Hit::EVENT, |(), _| true),
                record::callback::<NodeContext>(),
                CommandCapacity::default(),
            )
            .unwrap();
        let hit = hit::callback::<Tap, NodeContext>()
            .prepare(&mut world, command_capacity::<Hit>(1))
            .unwrap();
        Self {
            world,
            node,
            pending,
            listeners,
            hit,
        }
    }

    fn tap(&self) -> Tap {
        Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: self.node,
        }
    }
}

#[test]
fn typed_emission_releases_parameters_and_preserves_component_change_tracking_without_allocation() {
    let mut fixture = Fixture::new(notification_capacity::<Hit>(1));
    let tap = fixture.tap();
    for expected in [1, 2] {
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    fixture.hit.invoke(&mut fixture.world, &tap, fixture.node),
                    Ok(Some(crate::input::event::BubbleControl::Prevent))
                );
                assert_eq!(fixture.world.resource::<Trace>().unwrap().len, expected - 1);
                let mut changed = 0;
                fixture.world.drain_component_changes(|_, change| {
                    assert_eq!(change.entity, fixture.node);
                    assert_eq!(change.type_id, TypeId::of::<Pad>());
                    changed += 1;
                });
                assert_eq!(changed, 1);
                assert_eq!(
                    flush_notifications(&mut fixture.world, &fixture.listeners, 1),
                    Some(Ok(1))
                );
            }),
            0
        );
    }
    assert_eq!(fixture.world.get::<Pad>(fixture.node).unwrap().strength, 2);
    assert_eq!(
        &fixture.world.resource::<Trace>().unwrap().values[..2],
        &[(0, 1), (1, 2)]
    );
}

#[test]
fn preparation_reuses_the_service_and_preserves_pending_records_and_monotonic_capacity() {
    let mut fixture = Fixture::new(notification_capacity::<Hit>(1));
    let tap = fixture.tap();
    fixture
        .hit
        .invoke(&mut fixture.world, &tap, fixture.node)
        .unwrap();
    prepare_notifications(&mut fixture.world, notification_capacity::<Hit>(2)).unwrap();
    assert!(Rc::ptr_eq(
        &fixture.pending,
        fixture
            .world
            .resource::<Rc<PendingNotifications>>()
            .unwrap()
    ));
    let mut commands = CommandQueue::bounded(command_capacity::<Hit>(1)).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            prepare_notifications(&mut fixture.world, notification_capacity::<Hit>(0)).unwrap();
            let mut batch = commands.begin();
            batch
                .commands()
                .emit_always(fixture.node, Hit { old: 1, new: 2 });
            batch.finish(&mut fixture.world).unwrap();
            assert_eq!(
                flush_notifications(&mut fixture.world, &fixture.listeners, 2),
                Some(Ok(2))
            );
        }),
        0
    );
    let trace = fixture.world.resource::<Trace>().unwrap();
    assert_eq!(&trace.values[..trace.len], &[(0, 1), (1, 2)]);
    assert!(Rc::ptr_eq(
        &fixture.pending,
        fixture
            .world
            .resource::<Rc<PendingNotifications>>()
            .unwrap()
    ));
}

struct EmptyHit;

#[test]
fn default_emit_checks_subscriptions_at_commit_and_does_not_replay_skipped_events() {
    use crate::input::event::{EventCapacity, runtime};
    let mut world = World::new();
    world.insert_resource(Trace::default());
    let node = world.spawn(Pad { strength: 0 });
    let other = world.spawn(Pad { strength: 0 });
    runtime::configure(&mut world, EventCapacity::new()).unwrap();
    let mut queue = CommandQueue::bounded(command_capacity::<Hit>(1)).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            let mut batch = queue.begin();
            batch.commands().emit(node, Hit { old: 0, new: 1 });
            batch.finish(&mut world).unwrap();
            assert_eq!(runtime::flush(&mut world).unwrap(), 0);
        }),
        0
    );
    assert!(world.resource::<Rc<PendingNotifications>>().is_none());
    runtime::configure(
        &mut world,
        EventCapacity::new().notifications::<Hit>(1).flush_limit(1),
    )
    .unwrap();
    runtime::install_entity(
        &mut world,
        other,
        Hit::EVENT,
        record::callback::<NodeContext>(),
    )
    .unwrap();
    let mut batch = queue.begin();
    batch.commands().emit(node, Hit { old: 1, new: 2 });
    batch.finish(&mut world).unwrap();
    runtime::install_entity(
        &mut world,
        node,
        Hit::EVENT,
        record::callback::<NodeContext>(),
    )
    .unwrap();
    assert_eq!(runtime::flush(&mut world).unwrap(), 0);
    let mut batch = queue.begin();
    batch.commands().emit(node, Hit { old: 2, new: 3 });
    batch.finish(&mut world).unwrap();
    assert_eq!(runtime::flush(&mut world).unwrap(), 1);
    assert_eq!(world.resource::<Trace>().unwrap().values[0], (2, 3));
}

#[test]
fn emit_always_can_reach_a_listener_installed_before_flush_but_keeps_no_history() {
    use crate::input::event::{EventCapacity, runtime};
    let mut world = World::new();
    world.insert_resource(Trace::default());
    let node = world.spawn(Pad { strength: 0 });
    runtime::configure(
        &mut world,
        EventCapacity::new().notifications::<Hit>(1).flush_limit(1),
    )
    .unwrap();
    let mut queue = CommandQueue::bounded(command_capacity::<Hit>(1)).unwrap();
    let mut batch = queue.begin();
    batch.commands().emit_always(node, Hit { old: 0, new: 1 });
    batch.finish(&mut world).unwrap();
    assert_eq!(runtime::flush(&mut world).unwrap(), 1);
    let mut batch = queue.begin();
    batch.commands().emit_always(node, Hit { old: 1, new: 2 });
    batch.finish(&mut world).unwrap();
    runtime::install_entity(
        &mut world,
        node,
        Hit::EVENT,
        record::callback::<NodeContext>(),
    )
    .unwrap();
    assert_eq!(runtime::flush(&mut world).unwrap(), 1);
    assert_eq!(world.resource::<Trace>().unwrap().len, 1);
    assert_eq!(world.resource::<Trace>().unwrap().values[0], (1, 2));
}

#[test]
fn skipped_owning_emit_releases_its_payload_once_without_business_storage() {
    let mut world = World::new();
    let node = world.spawn(Pad { strength: 0 });
    let drops = Rc::new(Cell::new(0));
    let mut queue = CommandQueue::bounded(command_capacity::<OwnedHit>(1)).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            let mut batch = queue.begin();
            batch.commands().emit(
                node,
                OwnedHit {
                    value: 1,
                    drops: drops.clone(),
                },
            );
            assert_eq!(drops.get(), 0);
            batch.finish(&mut world).unwrap();
            assert_eq!(drops.get(), 1);
        }),
        0
    );
    assert!(world.resource::<Rc<PendingNotifications>>().is_none());
}

impl Event for EmptyHit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

#[test]
fn runtime_business_limits_do_not_borrow_app_slots_or_retained_storage() {
    let mut world = World::new();
    let node = world.spawn(Pad { strength: 0 });
    let listeners = EventListeners::new(&mut world);
    prepare_runtime_notifications(&mut world, None, false).unwrap();
    assert!(world.resource::<Rc<PendingNotifications>>().is_none());
    assert_eq!(
        enqueue_app_notice(&mut world, Started),
        Err(CommandError::Commit(CommandCommitError::MissingService {
            service: type_name::<PendingNotifications>(),
        }))
    );
    prepare_runtime_notifications(&mut world, Some(NotificationCapacity::default()), false)
        .unwrap();
    assert_eq!(
        enqueue_app_notice(&mut world, Started),
        Err(CommandError::Commit(CommandCommitError::RecordCapacity {
            available: 0
        }))
    );
    prepare_runtime_notifications(&mut world, None, true).unwrap();
    let pending = world
        .resource::<Rc<PendingNotifications>>()
        .unwrap()
        .clone();
    let mut empty = CommandQueue::bounded(command_capacity::<EmptyHit>(1)).unwrap();
    for (business, error) in [
        (
            None,
            CommandCommitError::MissingService {
                service: type_name::<PendingNotifications>(),
            },
        ),
        (
            Some(NotificationCapacity::default()),
            CommandCommitError::RecordCapacity { available: 0 },
        ),
    ] {
        prepare_runtime_notifications(&mut world, business, true).unwrap();
        assert_eq!(
            tracked_allocations(|| {
                enqueue_app_notice(&mut world, Started).unwrap();
                let mut batch = empty.begin();
                batch.commands().emit_always(node, EmptyHit);
                assert_eq!(batch.finish(&mut world), Err(CommandError::Commit(error)));
                assert_eq!(flush_notifications(&mut world, &listeners, 1), Some(Ok(1)));
            }),
            0
        );
    }
    prepare_runtime_notifications(&mut world, Some(notification_capacity::<Hit>(3)), true).unwrap();
    prepare_runtime_notifications(&mut world, Some(notification_capacity::<Hit>(1)), true).unwrap();
    let mut hits = CommandQueue::bounded(command_capacity::<Hit>(2)).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            let mut batch = hits.begin();
            batch.commands().emit_always(node, Hit { old: 0, new: 1 });
            batch.commands().emit_always(node, Hit { old: 1, new: 2 });
            assert_eq!(
                batch.finish(&mut world),
                Err(CommandError::Commit(CommandCommitError::RecordCapacity {
                    available: 1
                }))
            );
            enqueue_app_notice(&mut world, Started).unwrap();
            assert_eq!(flush_notifications(&mut world, &listeners, 2), Some(Ok(2)));
        }),
        0
    );
    prepare_runtime_notifications(&mut world, None, false).unwrap();
    let mut batch = empty.begin();
    batch.commands().emit_always(node, EmptyHit);
    assert!(matches!(
        batch.finish(&mut world),
        Err(CommandError::Commit(
            CommandCommitError::MissingService { .. }
        ))
    ));
    assert!(Rc::ptr_eq(
        &pending,
        world.resource::<Rc<PendingNotifications>>().unwrap()
    ));
}

#[crate::event]
fn after_first(input: event!(Hit), node: entity!(), commands: commands!()) {
    if input.new == 1 {
        commands.emit_always(node, Hit { old: 1, new: 3 });
    }
}

#[crate::event]
fn record_app(trace: res!(mut Trace)) {
    trace.record(u8::MAX, 0);
}

#[test]
fn runtime_app_notices_share_fifo_with_business_and_callback_writes_without_allocation() {
    let mut fixture = Fixture::new(notification_capacity::<Hit>(2));
    prepare_runtime_notifications(
        &mut fixture.world,
        Some(notification_capacity::<Hit>(2)),
        true,
    )
    .unwrap();
    fixture
        .listeners
        .reserve(ListenerCapacity {
            listeners: 3,
            snapshot_steps: 2,
        })
        .unwrap();
    fixture
        .listeners
        .install_entity(
            &mut fixture.world,
            fixture.node,
            ListenerSelector::new(Hit::EVENT, |(), _| true),
            after_first::callback::<NodeContext>(),
            command_capacity::<Hit>(1),
        )
        .unwrap();
    fixture
        .listeners
        .install_app(
            &mut fixture.world,
            ListenerSelector::new(Started::EVENT, |(), _| true),
            record_app::callback::<Started, AppContext>(),
            CommandCapacity::default(),
        )
        .unwrap();
    let mut commands = CommandQueue::bounded(command_capacity::<Hit>(2)).unwrap();
    for expected in [4, 8] {
        let visits = Cell::new(0);
        assert_eq!(
            tracked_allocations(|| {
                let mut batch = commands.begin();
                for new in [1, 2] {
                    batch
                        .commands()
                        .emit_always(fixture.node, Hit { old: 0, new });
                }
                batch.finish(&mut fixture.world).unwrap();
                enqueue_app_notice(&mut fixture.world, Started).unwrap();
                assert_eq!(
                    flush_notifications_with(&mut fixture.world, &fixture.listeners, 4, |world| {
                        assert_eq!(
                            prepare_runtime_notifications(world, None, false),
                            Err(NotificationError::Busy)
                        );
                        assert_eq!(
                            prepare_runtime_notifications(
                                world,
                                Some(NotificationCapacity {
                                    records: usize::MAX,
                                    ..NotificationCapacity::default()
                                }),
                                true,
                            ),
                            Err(NotificationError::Busy)
                        );
                        visits.set(visits.get() + 1);
                        Ok(())
                    }),
                    Some(Ok(4))
                );
            }),
            0
        );
        assert_eq!(visits.get(), 5);
        let trace = fixture.world.resource::<Trace>().unwrap();
        assert_eq!(trace.len, expected);
        assert_eq!(
            &trace.values[expected - 4..expected],
            &[(0, 1), (0, 2), (u8::MAX, 0), (1, 3)]
        );
    }
}

#[test]
fn runtime_app_slot_count_is_checked_without_installing_partial_storage() {
    let mut world = World::new();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                prepare_runtime_notifications(
                    &mut world,
                    Some(NotificationCapacity {
                        records: usize::MAX,
                        ..NotificationCapacity::default()
                    }),
                    true,
                ),
                Err(NotificationError::SizeOverflow)
            );
        }),
        0
    );
    assert!(world.resource::<Rc<PendingNotifications>>().is_none());
}

#[test]
fn runtime_payload_limits_and_failed_flushes_preserve_app_slots_and_drop_owning_values_once() {
    let mut world = World::new();
    let node = world.spawn(Pad { strength: 0 });
    let listeners = EventListeners::new(&mut world);
    let drops = Rc::new(Cell::new(0));
    let mut commands = CommandQueue::bounded(command_capacity::<OwnedHit>(1)).unwrap();
    prepare_runtime_notifications(&mut world, Some(notification_capacity::<OwnedHit>(1)), true)
        .unwrap();
    for (capacity, error) in [
        (
            NotificationCapacity {
                records: 1,
                payload_bytes: 0,
                payload_alignment: align_of::<OwnedHit>(),
            },
            CommandCommitError::PayloadCapacity {
                required: size_of::<OwnedHit>(),
                available: 0,
            },
        ),
        (
            NotificationCapacity {
                records: 1,
                payload_bytes: size_of::<OwnedHit>(),
                payload_alignment: 1,
            },
            CommandCommitError::PayloadAlignment {
                required: align_of::<OwnedHit>(),
                available: 1,
            },
        ),
    ] {
        prepare_runtime_notifications(&mut world, Some(capacity), false).unwrap();
        assert_eq!(
            tracked_allocations(|| {
                enqueue_app_notice(&mut world, Started).unwrap();
                let mut batch = commands.begin();
                batch.commands().emit_always(
                    node,
                    OwnedHit {
                        value: 1,
                        drops: drops.clone(),
                    },
                );
                assert_eq!(batch.finish(&mut world), Err(CommandError::Commit(error)));
                assert_eq!(flush_notifications(&mut world, &listeners, 1), Some(Ok(1)));
            }),
            0
        );
    }
    assert_eq!(drops.get(), 2);
    prepare_runtime_notifications(
        &mut world,
        Some(notification_capacity::<OwnedHit>(1)),
        false,
    )
    .unwrap();
    assert_eq!(
        prepare_runtime_notifications(
            &mut world,
            Some(NotificationCapacity {
                records: 1,
                payload_bytes: size_of::<OwnedHit>(),
                payload_alignment: 3
            }),
            false,
        ),
        Err(NotificationError::InvalidLayout)
    );
    assert_eq!(
        tracked_allocations(|| {
            let mut batch = commands.begin();
            batch.commands().emit_always(
                node,
                OwnedHit {
                    value: 3,
                    drops: drops.clone(),
                },
            );
            batch.finish(&mut world).unwrap();
            enqueue_app_notice(&mut world, Started).unwrap();
            assert_eq!(
                flush_notifications(&mut world, &listeners, 0),
                Some(Err(NotificationFlushError::Storage(
                    NotificationError::FlushLimit { limit: 0 }
                )))
            );
            assert_eq!(drops.get(), 3);
            enqueue_app_notice(&mut world, Resumed).unwrap();
            let mut batch = commands.begin();
            batch.commands().emit_always(
                node,
                OwnedHit {
                    value: 4,
                    drops: drops.clone(),
                },
            );
            batch.finish(&mut world).unwrap();
            assert_eq!(flush_notifications(&mut world, &listeners, 2), Some(Ok(2)));
        }),
        0
    );
    assert_eq!(drops.get(), 4);
}

#[test]
fn failed_structural_preparation_drops_pending_payloads_and_reuses_the_app_slot_without_allocation()
{
    let mut world = World::new();
    let node = world.spawn(Pad { strength: 0 });
    let listeners = EventListeners::new(&mut world);
    let drops = Rc::new(Cell::new(0));
    let mut commands = CommandQueue::bounded(command_capacity::<OwnedHit>(1)).unwrap();
    prepare_runtime_notifications(&mut world, Some(notification_capacity::<OwnedHit>(1)), true)
        .unwrap();
    for expected in [2, 4] {
        let attempts = Cell::new(0);
        assert_eq!(
            tracked_allocations(|| {
                let mut batch = commands.begin();
                batch.commands().emit_always(
                    node,
                    OwnedHit {
                        value: 1,
                        drops: drops.clone(),
                    },
                );
                batch.finish(&mut world).unwrap();
                enqueue_app_notice(&mut world, Started).unwrap();
                assert_eq!(
                    flush_notifications_with(&mut world, &listeners, 2, |_| {
                        attempts.set(attempts.get() + 1);
                        Err(ListenerError::Busy)
                    }),
                    Some(Err(NotificationFlushError::Listener(ListenerError::Busy)))
                );
                assert_eq!(drops.get(), expected - 1);
                assert_eq!(attempts.get(), 1);
                enqueue_app_notice(&mut world, Resumed).unwrap();
                let mut batch = commands.begin();
                batch.commands().emit_always(
                    node,
                    OwnedHit {
                        value: 2,
                        drops: drops.clone(),
                    },
                );
                batch.finish(&mut world).unwrap();
                assert_eq!(flush_notifications(&mut world, &listeners, 2), Some(Ok(2)));
                assert_eq!(drops.get(), expected);
            }),
            0
        );
    }
    let attempts = Cell::new(0);
    assert_eq!(
        flush_notifications_with(&mut world, &listeners, 0, |_| {
            attempts.set(attempts.get() + 1);
            Ok(())
        }),
        Some(Ok(0))
    );
    assert_eq!(attempts.get(), 1);
}

#[test]
fn flushing_an_absent_service_does_not_install_storage_or_allocate() {
    let mut world = World::new();
    let listeners = EventListeners::new(&mut world);
    for _ in 0..2 {
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(flush_notifications(&mut world, &listeners, 1), None);
            }),
            0
        );
    }
    assert!(world.resource::<Rc<PendingNotifications>>().is_none());
}

#[crate::event]
fn prepare_while_flushing(input: event!(Hit), node: entity!(), commands: commands!()) {
    if input.old == 0 {
        commands.apply_resource((), |world, ()| {
            assert_eq!(
                prepare_notifications(world, notification_capacity::<Hit>(2)),
                Err(NotificationError::Busy)
            );
        });
        commands.emit_always(
            node,
            Hit {
                old: input.new,
                new: input.new + 10,
            },
        );
    }
}

#[test]
fn active_preparation_returns_busy_without_replacing_the_service_or_reordering_notifications() {
    let mut fixture = Fixture::new(notification_capacity::<Hit>(2));
    fixture
        .listeners
        .install_entity(
            &mut fixture.world,
            fixture.node,
            ListenerSelector::new(Hit::EVENT, |(), _| true),
            prepare_while_flushing::callback::<NodeContext>(),
            CommandCapacity::new()
                .resource_value::<()>(1)
                .emit::<Hit>(1),
        )
        .unwrap();
    let mut commands = CommandQueue::bounded(command_capacity::<Hit>(2)).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            let mut batch = commands.begin();
            for new in [1, 2] {
                batch
                    .commands()
                    .emit_always(fixture.node, Hit { old: 0, new });
            }
            batch.finish(&mut fixture.world).unwrap();
            assert_eq!(
                flush_notifications(&mut fixture.world, &fixture.listeners, 4),
                Some(Ok(4))
            );
        }),
        0
    );
    assert!(Rc::ptr_eq(
        &fixture.pending,
        fixture
            .world
            .resource::<Rc<PendingNotifications>>()
            .unwrap()
    ));
    let trace = fixture.world.resource::<Trace>().unwrap();
    assert_eq!(
        &trace.values[..trace.len],
        &[(0, 1), (0, 2), (1, 11), (2, 12)]
    );
}

#[crate::event]
fn chain(input: event!(Hit), node: entity!(), commands: commands!()) {
    if input.new < 3 {
        commands.emit_always(
            node,
            Hit {
                old: input.new,
                new: input.new + 1,
            },
        );
    }
}

#[test]
fn notifications_emitted_by_callbacks_keep_fifo_and_the_active_payload_stable() {
    let mut fixture = Fixture::new(notification_capacity::<Hit>(1));
    fixture
        .listeners
        .install_entity(
            &mut fixture.world,
            fixture.node,
            ListenerSelector::new(Hit::EVENT, |(), _| true),
            chain::callback::<NodeContext>(),
            command_capacity::<Hit>(1),
        )
        .unwrap();
    let tap = fixture.tap();
    assert_eq!(
        tracked_allocations(|| {
            fixture
                .hit
                .invoke(&mut fixture.world, &tap, fixture.node)
                .unwrap();
            assert_eq!(
                flush_notifications(&mut fixture.world, &fixture.listeners, 3),
                Some(Ok(3))
            );
        }),
        0
    );
    let trace = fixture.world.resource::<Trace>().unwrap();
    assert_eq!(&trace.values[..trace.len], &[(0, 1), (1, 2), (2, 3)]);
}

#[repr(align(64))]
struct OwnedHit {
    value: u8,
    drops: Rc<Cell<usize>>,
}

impl Drop for OwnedHit {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl Event for OwnedHit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}

#[crate::event]
fn record_owned(input: event!(OwnedHit), trace: res!(mut Trace)) {
    assert_eq!(
        core::ptr::from_ref(input).addr() % align_of::<OwnedHit>(),
        0
    );
    trace.record(0, input.value);
}

#[test]
fn retired_notification_payloads_are_dropped_once_and_storage_is_reused() {
    let mut fixture = Fixture::new(notification_capacity::<OwnedHit>(1));
    let gate =
        crate::core::reactive::retained::RetainedEffects::new(&mut fixture.world, |_, _| true);
    fixture.world.insert(fixture.node, gate);
    fixture
        .listeners
        .install_entity(
            &mut fixture.world,
            fixture.node,
            ListenerSelector::new(OwnedHit::EVENT, |(), _| true),
            record_owned::callback::<NodeContext>(),
            CommandCapacity::default(),
        )
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    assert_eq!(
        tracked_allocations(|| {
            for expected in 1..=2 {
                submit(
                    &mut fixture.world,
                    fixture.node,
                    OwnedHit {
                        value: expected,
                        drops: drops.clone(),
                    },
                )
                .unwrap();
                if expected == 1 {
                    crate::core::reactive::retained::retire(&mut fixture.world, fixture.node);
                }
                fixture
                    .pending
                    .flush(&fixture.listeners, &mut fixture.world, 4)
                    .unwrap();
                assert_eq!(drops.get(), usize::from(expected));
                assert_eq!(
                    fixture.world.resource::<Trace>().unwrap().len,
                    usize::from(expected - 1)
                );
            }
        }),
        0
    );
    assert_eq!(fixture.world.resource::<Trace>().unwrap().values[0], (0, 2));
}

struct Replacement(Rc<PendingNotifications>);

#[crate::event]
fn replace_pending(
    input: event!(OwnedHit),
    node: entity!(),
    replacement: res!(Replacement),
    commands: commands!(),
) {
    commands.apply_resource(replacement.0.clone(), |world, pending| {
        world.insert_resource(pending);
    });
    commands.emit_always(
        node,
        Hit {
            old: 0,
            new: input.value,
        },
    );
}

#[test]
fn forced_service_replacement_keeps_active_payloads_alive_and_redirects_later_submissions() {
    let mut world = World::new();
    let node = world.spawn(Pad { strength: 0 });
    world.insert_resource(Trace::default());
    let old = install_pending(&mut world, notification_capacity::<OwnedHit>(2));
    let old_weak = Rc::downgrade(&old);
    let replacement = Rc::new(
        PendingNotifications::new(&world, Some(notification_capacity::<Hit>(2)), false).unwrap(),
    );
    world.insert_resource(Replacement(replacement.clone()));
    let listeners = EventListeners::new(&mut world);
    listeners
        .reserve(ListenerCapacity {
            listeners: 3,
            snapshot_steps: 2,
        })
        .unwrap();
    listeners
        .install_entity(
            &mut world,
            node,
            ListenerSelector::new(OwnedHit::EVENT, |(), _| true),
            replace_pending::callback::<NodeContext>(),
            CommandCapacity::new()
                .resource_value::<Rc<PendingNotifications>>(1)
                .emit::<Hit>(1),
        )
        .unwrap();
    listeners
        .install_entity(
            &mut world,
            node,
            ListenerSelector::new(OwnedHit::EVENT, |(), _| true),
            record_owned::callback::<NodeContext>(),
            CommandCapacity::default(),
        )
        .unwrap();
    listeners
        .install_entity(
            &mut world,
            node,
            ListenerSelector::new(Hit::EVENT, |(), _| true),
            record::callback::<NodeContext>(),
            CommandCapacity::default(),
        )
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    let mut queue = CommandQueue::bounded(command_capacity::<OwnedHit>(2)).unwrap();
    let mut batch = queue.begin();
    for value in [1, 2] {
        batch.commands().emit_always(
            node,
            OwnedHit {
                value,
                drops: drops.clone(),
            },
        );
    }
    batch.finish(&mut world).unwrap();
    drop(old);
    assert_eq!(
        tracked_allocations(|| {
            let active = world
                .resource::<Rc<PendingNotifications>>()
                .unwrap()
                .clone();
            assert_eq!(active.flush(&listeners, &mut world, 2), Ok(2));
            assert_eq!(drops.get(), 2);
            assert!(old_weak.upgrade().is_some());
            assert!(Rc::ptr_eq(
                world.resource::<Rc<PendingNotifications>>().unwrap(),
                &replacement
            ));
            drop(active);
            assert!(old_weak.upgrade().is_none());
            assert_eq!(replacement.flush(&listeners, &mut world, 2), Ok(2));
        }),
        0
    );
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!(
        &trace.values[..trace.len],
        &[(0, 1), (0, 2), (0, 1), (0, 2)]
    );
}

#[test]
fn commit_failure_drops_owning_payloads_once_preserves_completed_commands_and_reuses_storage() {
    let mut world = World::new();
    let node = world.spawn(Pad { strength: 0 });
    world.insert_resource(Trace::default());
    let pending = install_pending(&mut world, notification_capacity::<OwnedHit>(1));
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    pool.install_entity(
        &mut world,
        node,
        ListenerSelector::new(OwnedHit::EVENT, |(), _| true),
        record_owned::callback::<NodeContext>(),
        CommandCapacity::default(),
    )
    .unwrap();
    let drops = Rc::new(Cell::new(0));
    let mut commands = CommandQueue::bounded(command_capacity::<OwnedHit>(3)).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            let mut batch = commands.begin();
            for value in [1, 2, 3] {
                batch.commands().emit_always(
                    node,
                    OwnedHit {
                        value,
                        drops: drops.clone(),
                    },
                );
            }
            assert_eq!(
                batch.finish(&mut world),
                Err(CommandError::Commit(CommandCommitError::RecordCapacity {
                    available: 1
                }))
            );
            assert_eq!(drops.get(), 2);
            assert_eq!(pending.flush(&pool, &mut world, 1), Ok(1));
            assert_eq!(drops.get(), 3);
            let mut batch = commands.begin();
            batch.commands().emit_always(
                node,
                OwnedHit {
                    value: 4,
                    drops: drops.clone(),
                },
            );
            batch.finish(&mut world).unwrap();
            assert_eq!(pending.flush(&pool, &mut world, 1), Ok(1));
        }),
        0
    );
    assert_eq!(drops.get(), 4);
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!(&trace.values[..trace.len], &[(0, 1), (0, 4)]);
}

#[test]
fn source_removal_is_checked_at_commit_without_rolling_back_completed_commands() {
    let mut world = World::new();
    let node = world.spawn(Pad { strength: 0 });
    let pending = install_pending(&mut world, notification_capacity::<OwnedHit>(1));
    let drops = Rc::new(Cell::new(0));
    let mut queue = CommandQueue::bounded(command_capacity::<OwnedHit>(3)).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            let mut batch = queue.begin();
            batch.commands().remove::<Pad>(node);
            batch.commands().emit_always(
                node,
                OwnedHit {
                    value: 1,
                    drops: drops.clone(),
                },
            );
            batch.commands().emit_always(
                node,
                OwnedHit {
                    value: 2,
                    drops: drops.clone(),
                },
            );
            assert_eq!(
                batch.finish(&mut world),
                Err(CommandError::Commit(CommandCommitError::MissingSource {
                    entity: node,
                    source: type_name::<WidgetSource<Pad>>(),
                }))
            );
        }),
        0
    );
    assert!(!world.has::<Pad>(node));
    assert_eq!(drops.get(), 2);
    assert_eq!(
        pending
            .queue
            .flush(1, |_| panic!("missing source was queued")),
        Ok::<_, NotificationFlushError<()>>(0)
    );
}

#[test]
fn stale_targets_and_missing_or_foreign_services_return_commit_errors() {
    let mut world = World::new();
    let node = world.spawn(Pad { strength: 0 });
    let mut queue = CommandQueue::bounded(command_capacity::<Hit>(1)).unwrap();
    let mut batch = queue.begin();
    batch.commands().emit_always(node, Hit { old: 0, new: 1 });
    assert_eq!(
        batch.finish(&mut world),
        Err(CommandError::Commit(CommandCommitError::MissingService {
            service: type_name::<PendingNotifications>()
        }))
    );
    let other = World::new();
    let foreign = Rc::new(
        PendingNotifications::new(&other, Some(notification_capacity::<Hit>(1)), false).unwrap(),
    );
    world.insert_resource(foreign);
    let mut batch = queue.begin();
    batch.commands().emit_always(node, Hit { old: 0, new: 1 });
    assert_eq!(
        batch.finish(&mut world),
        Err(CommandError::Commit(CommandCommitError::ForeignOwner {
            service: type_name::<PendingNotifications>()
        }))
    );
    world.remove_resource::<Rc<PendingNotifications>>();
    let pending = install_pending(&mut world, notification_capacity::<Hit>(1));
    let mut batch = queue.begin();
    batch.commands().despawn(node);
    batch.commands().emit_always(node, Hit { old: 0, new: 1 });
    assert_eq!(
        batch.finish(&mut world),
        Err(CommandError::RecordCapacity { available: 1 })
    );
    assert!(world.is_alive(node));
    world.despawn(node);
    let replacement = world.spawn(Pad { strength: 0 });
    assert_eq!(replacement.id, node.id);
    let mut batch = queue.begin();
    batch.commands().emit_always(node, Hit { old: 0, new: 1 });
    assert_eq!(
        batch.finish(&mut world),
        Err(CommandError::Commit(CommandCommitError::MissingTarget {
            entity: node
        }))
    );
    assert_eq!(
        pending
            .queue
            .flush(1, |_| panic!("stale source was queued")),
        Ok::<_, NotificationFlushError<()>>(0)
    );
}

#[test]
fn executor_propagates_commit_capacity_errors_without_rolling_back_component_updates() {
    let mut fixture = Fixture::new(notification_capacity::<Hit>(0));
    let tap = fixture.tap();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                fixture.hit.invoke(&mut fixture.world, &tap, fixture.node),
                Err(CommandError::Commit(CommandCommitError::RecordCapacity {
                    available: 0
                }))
            );
        }),
        0
    );
    assert_eq!(fixture.world.get::<Pad>(fixture.node).unwrap().strength, 1);
    assert_eq!(fixture.world.resource::<Trace>().unwrap().len, 0);
}

#[test]
fn submission_byte_and_alignment_limits_remain_distinct_from_recording_limits() {
    let drops = Rc::new(Cell::new(0));
    let mut queue = CommandQueue::bounded(command_capacity::<OwnedHit>(1)).unwrap();
    for (capacity, error) in [
        (
            NotificationCapacity {
                records: 1,
                payload_bytes: 0,
                payload_alignment: align_of::<OwnedHit>(),
            },
            CommandCommitError::PayloadCapacity {
                required: size_of::<OwnedHit>(),
                available: 0,
            },
        ),
        (
            NotificationCapacity {
                records: 1,
                payload_bytes: size_of::<OwnedHit>(),
                payload_alignment: 1,
            },
            CommandCommitError::PayloadAlignment {
                required: align_of::<OwnedHit>(),
                available: 1,
            },
        ),
    ] {
        let mut world = World::new();
        let node = world.spawn(Pad { strength: 0 });
        install_pending(&mut world, capacity);
        assert_eq!(
            tracked_allocations(|| {
                let mut batch = queue.begin();
                batch.commands().emit_always(
                    node,
                    OwnedHit {
                        value: 1,
                        drops: drops.clone(),
                    },
                );
                assert_eq!(batch.finish(&mut world), Err(CommandError::Commit(error)));
            }),
            0
        );
    }
    assert_eq!(drops.get(), 2);
}
