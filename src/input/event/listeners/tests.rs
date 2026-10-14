use super::super::callback::{EntityContext, NodeContext, deferred_event_callback, event_callback};
use super::super::contract::{AppNotification, AppSource, EntityNotification, WidgetSource};
use super::super::input::Key;
use super::super::notification::NotificationCapacity;
use super::*;
use crate::ecs::Component;
use crate::input::event::gesture::DragMove;
use crate::input::event::gesture::Tap;
use crate::test_support::tracked_allocations;
use crate::types::Fixed;
use core::cell::Cell;

struct RouteMarker;
impl Component for RouteMarker {}

#[crate::event]
fn record_current_target(
    target: entity!(),
    input: event!(Tap),
    targets: res!(RefCell<[Option<Entity>; 2]>),
) -> bool {
    *targets.borrow_mut() = [Some(target), Some(input.target)];
    false
}

#[test]
fn prepared_listener_uses_each_invocation_target_without_repreparing() {
    let mut world = World::new();
    let first = world.spawn_empty();
    let second = world.spawn_empty();
    world.insert_resource(RefCell::new([None::<Entity>; 2]));
    let callback = record_current_target::callback::<NodeContext>()
        .prepare(&mut world, CommandCapacity::default())
        .unwrap();
    let listener = InstalledListener {
        callback,
        selector: ListenerSelector::gesture(Tap::EVENT),
    };
    let payload = Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target: first,
    };
    let event = BorrowedEvent::new(&payload);
    assert_eq!(
        tracked_allocations(|| {
            for target in [first, second, first] {
                assert_eq!(
                    listener
                        .invoke(&mut world, &event, ListenerTarget::Entity(target))
                        .unwrap(),
                    BubbleControl::Allow
                );
                assert_eq!(
                    *world
                        .resource::<RefCell<[Option<Entity>; 2]>>()
                        .unwrap()
                        .borrow(),
                    [Some(target), Some(first)]
                );
            }
        }),
        0
    );
    assert!(world.despawn(first));
    *world
        .resource::<RefCell<[Option<Entity>; 2]>>()
        .unwrap()
        .borrow_mut() = [None; 2];
    for target in [ListenerTarget::Entity(first), ListenerTarget::App] {
        listener.invoke(&mut world, &event, target).unwrap();
        assert_eq!(
            *world
                .resource::<RefCell<[Option<Entity>; 2]>>()
                .unwrap()
                .borrow(),
            [None; 2]
        );
    }
}

use crate::core::reactive::retained::RetainedEffects;
#[crate::event]
fn recycle(node: entity!(), _tap: event!(Tap), commands: commands!()) -> bool {
    commands.apply(node, (), |world, node, ()| {
        super::super::retire_subtree_interaction(world, node);
        Ok(())
    });
    false
}

#[test]
fn retained_input_snapshot_stops_after_a_handler_rebinds_the_cell() {
    let mut world = World::new();
    let cell = world.spawn_empty();
    let gate = RetainedEffects::new(&mut world, |_, _| true);
    world.insert(cell, gate);
    world.insert(cell, RouteMarker);
    world.insert_resource(TapTrace {
        trace: RefCell::new(Trace::default()),
        consume: Cell::new(None),
    });
    let mut registry = ViewRegistry::default();
    registry.insert(
        crate::ui::view::View::systems_only("recycle", &[])
            .with_filter::<RouteMarker>()
            .with_inputs(&mut world, recycle::callback()),
    );
    world.insert_resource(registry);
    crate::ui::lifecycle::attach_handlers_for(&mut world, cell);
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 2,
    })
    .unwrap();
    pool.install_entity::<Tap, EntityContext<RouteMarker>>(
        &mut world,
        cell,
        ListenerSelector::gesture(Tap::EVENT),
        record_tap::callback(1),
        CommandCapacity::default(),
    )
    .unwrap();
    let event = Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target: cell,
    };
    assert_eq!(
        tracked_allocations(|| {
            pool.snapshot_gesture(&world, &event)
                .unwrap()
                .run(&mut world)
                .unwrap();
        }),
        0
    );
    assert_eq!(world.resource::<TapTrace>().unwrap().trace.borrow().len, 0);
    world.remove_resource::<ViewRegistry>();
    pool.snapshot_gesture(&world, &event)
        .unwrap()
        .run(&mut world)
        .unwrap();
    assert_eq!(
        world
            .resource::<TapTrace>()
            .unwrap()
            .trace
            .borrow()
            .values(),
        &[1]
    );
}

struct TapTrace {
    trace: RefCell<Trace>,
    consume: Cell<Option<u32>>,
}

#[crate::event]
fn record_tap(value: u32, trace: res!(TapTrace), event: event!(Tap), node: entity!()) -> bool {
    assert!(node == event.target || value == 5);
    trace.trace.borrow_mut().record(value);
    trace.consume.get() == Some(value)
}

#[test]
fn tap_selection_freezes_origin_count_and_preserves_order_consumption_and_storage() {
    use super::super::multi_tap::{MultiTapEntry, MultiTapTracker};
    let mut world = World::new();
    let parent = world.spawn_empty();
    let leaf = world.spawn_empty();
    world.insert(parent, RouteMarker);
    world.insert(leaf, RouteMarker);
    world.insert(leaf, Parent(parent));
    world.insert_resource(TapTrace {
        trace: RefCell::new(Trace::default()),
        consume: Cell::new(None),
    });
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 5,
        snapshot_steps: 5,
    })
    .unwrap();
    let double = Tap::EVENT.configure((2,)).unwrap();
    for (node, key, value) in [
        (leaf, double, 1),
        (leaf, Tap::EVENT, 2),
        (leaf, double, 3),
        (leaf, Tap::EVENT, 4),
        (parent, double, 5),
    ] {
        pool.install_entity::<Tap, EntityContext<RouteMarker>>(
            &mut world,
            node,
            ListenerSelector::gesture(key),
            record_tap::callback(value),
            CommandCapacity::default(),
        )
        .unwrap();
    }
    let event = Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target: leaf,
    };
    for (count, consume, expected) in [
        (1, None, &[2, 4][..]),
        (2, None, &[1, 2, 3, 4, 5][..]),
        (3, None, &[2, 4][..]),
        (2, Some(2), &[1, 2][..]),
        (2, Some(1), &[1][..]),
    ] {
        world.insert_resource(MultiTapTracker {
            last: Some(MultiTapEntry {
                entity: leaf,
                time_ms: 100,
                count,
            }),
        });
        let trace = world.resource::<TapTrace>().unwrap();
        trace.trace.borrow_mut().len = 0;
        trace.consume.set(consume);
        assert_eq!(
            tracked_allocations(|| {
                let snapshot = pool.snapshot_gesture(&world, &event).unwrap();
                world.remove_resource::<MultiTapTracker>();
                let control = snapshot.run(&mut world).unwrap();
                assert_eq!(
                    control,
                    if consume.is_some() {
                        BubbleControl::Prevent
                    } else {
                        BubbleControl::Allow
                    }
                );
            }),
            0
        );
        assert_eq!(
            world
                .resource::<TapTrace>()
                .unwrap()
                .trace
                .borrow()
                .values(),
            expected
        );
    }
}

struct RouteTrace {
    trace: Rc<RefCell<Trace>>,
    origin: Entity,
    alternate: Entity,
    old_parent: Entity,
    internal_stop: Option<Entity>,
    author_stop: Option<Entity>,
}

#[crate::event]
fn route_first(
    node: entity!(),
    event: event!(DragMove),
    trace: res!(RouteTrace),
    commands: commands!(),
) -> bool {
    assert_eq!(event.target, trace.origin);
    trace.trace.borrow_mut().record(100 + node.id);
    let stop = trace.internal_stop == Some(node);
    if node == trace.origin {
        commands.insert(node, Parent(trace.alternate));
        commands.remove::<RouteMarker>(trace.old_parent);
    }
    stop
}

#[crate::event]
fn route_second(node: entity!(), _event: event!(DragMove), trace: res!(RouteTrace)) -> bool {
    trace.trace.borrow_mut().record(200 + node.id);
    false
}

#[crate::event]
fn route_author(input: event!(DragMove), node: entity!(), trace: res!(RouteTrace)) -> bool {
    assert_eq!(input.target, trace.origin);
    trace.trace.borrow_mut().record(300 + node.id);
    trace.author_stop == Some(node)
}

struct RouteFixture {
    world: World,
    pool: EventListeners,
    trace: Rc<RefCell<Trace>>,
    root: Entity,
    parent: Entity,
    leaf: Entity,
    alternate: Entity,
}

impl RouteFixture {
    fn new(snapshot_steps: usize) -> Self {
        let mut world = World::new();
        let root = world.spawn_empty();
        let parent = world.spawn_empty();
        let leaf = world.spawn_empty();
        let alternate = world.spawn_empty();
        world.insert(parent, Parent(root));
        world.insert(leaf, Parent(parent));
        let trace = Rc::new(RefCell::new(Trace::default()));
        world.insert_resource(RouteTrace {
            trace: trace.clone(),
            origin: leaf,
            alternate,
            old_parent: parent,
            internal_stop: None,
            author_stop: None,
        });
        let mut registry = ViewRegistry::default();
        registry.insert(
            crate::ui::view::View::systems_only("first", &[])
                .with_filter::<RouteMarker>()
                .with_inputs(&mut world, route_first::callback()),
        );
        registry.insert(
            crate::ui::view::View::systems_only("second", &[])
                .with_filter::<RouteMarker>()
                .with_inputs(&mut world, route_second::callback()),
        );
        world.insert_resource(registry);
        let pool = EventListeners::new(&mut world);
        pool.reserve(ListenerCapacity {
            listeners: 4,
            snapshot_steps,
        })
        .unwrap();
        // Install out of tree order; route order must still be leaf to root.
        for node in [root, alternate, parent, leaf] {
            world.insert(node, RouteMarker);
            crate::ui::lifecycle::attach_handlers_for(&mut world, node);
            pool.install_entity::<DragMove, EntityContext<RouteMarker>>(
                &mut world,
                node,
                selector(),
                route_author::callback(),
                CommandCapacity::default(),
            )
            .unwrap();
        }
        Self {
            world,
            pool,
            trace,
            root,
            parent,
            leaf,
            alternate,
        }
    }

    fn event(&self) -> DragMove {
        DragMove {
            x: Fixed::ONE,
            y: Fixed::ONE,
            dx: Fixed::ONE,
            dy: Fixed::ZERO,
            target: self.leaf,
        }
    }
}

#[test]
fn gesture_route_freezes_parents_but_skips_removed_component_instances() {
    let mut fixture = RouteFixture::new(9);
    let event = fixture.event();
    let allocations = tracked_allocations(|| {
        assert_eq!(
            fixture
                .pool
                .snapshot_gesture(&fixture.world, &event)
                .unwrap()
                .run(&mut fixture.world),
            Ok(BubbleControl::Allow)
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(
        fixture.trace.borrow().values(),
        &[102, 202, 302, 301, 100, 200, 300]
    );
    assert_eq!(
        fixture.world.get::<Parent>(fixture.leaf).unwrap().0,
        fixture.alternate
    );
    assert!(!fixture.world.has::<RouteMarker>(fixture.parent));
    fixture.trace.borrow_mut().len = 0;
    let allocations = tracked_allocations(|| {
        fixture
            .pool
            .snapshot_gesture(&fixture.world, &event)
            .unwrap()
            .run(&mut fixture.world)
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(
        fixture.trace.borrow().values(),
        &[102, 202, 302, 103, 203, 303]
    );
}

#[test]
fn gesture_route_rejects_cycles_and_capacity_before_any_partial_execution() {
    let mut fixture = RouteFixture::new(8);
    let event = fixture.event();
    let allocations = tracked_allocations(|| {
        assert!(matches!(
            fixture.pool.snapshot_gesture(&fixture.world, &event),
            Err(ListenerError::SnapshotCapacity { available: 8 })
        ));
    });
    assert_eq!(allocations, 0);
    assert!(fixture.trace.borrow().values().is_empty());
    assert!(fixture.pool.scratch.borrow().steps.is_empty());
    fixture
        .pool
        .reserve(ListenerCapacity {
            listeners: 4,
            snapshot_steps: 9,
        })
        .unwrap();
    fixture.world.insert(fixture.root, Parent(fixture.leaf));
    let allocations = tracked_allocations(|| {
        assert!(matches!(
            fixture.pool.snapshot_gesture(&fixture.world, &event),
            Err(ListenerError::CyclicRoute { .. })
        ));
    });
    assert_eq!(allocations, 0);
    assert!(fixture.trace.borrow().values().is_empty());
    assert!(fixture.pool.scratch.borrow().steps.is_empty());
    fixture.world.remove::<Parent>(fixture.root);
    fixture
        .pool
        .snapshot_gesture(&fixture.world, &event)
        .unwrap()
        .run(&mut fixture.world)
        .unwrap();
    assert_eq!(fixture.trace.borrow().len, 7);
}

#[test]
fn gesture_owner_and_busy_checks_precede_branch_cycle_validation() {
    let mut fixture = RouteFixture::new(9);
    let event = fixture.event();
    let snapshot = fixture
        .pool
        .snapshot_gesture(&fixture.world, &event)
        .unwrap();
    fixture.world.insert(fixture.root, Parent(fixture.leaf));
    assert!(matches!(
        fixture.pool.snapshot_gesture(&fixture.world, &event),
        Err(ListenerError::Busy)
    ));
    let mut foreign = World::new();
    let parent = foreign.spawn_empty();
    let child = foreign.spawn_empty();
    foreign.insert(parent, Parent(child));
    foreign.insert(child, Parent(parent));
    let mut foreign_event = event;
    foreign_event.target = child;
    assert!(matches!(
        fixture.pool.snapshot_gesture(&foreign, &foreign_event),
        Err(ListenerError::ForeignOwner)
    ));
    drop(snapshot);
    assert!(matches!(
        fixture.pool.snapshot_gesture(&fixture.world, &event),
        Err(ListenerError::CyclicRoute { .. })
    ));
    assert!(fixture.pool.scratch.borrow().steps.is_empty());
}

#[test]
fn hidden_or_stale_gesture_origins_do_not_consume_snapshot_capacity() {
    let mut fixture = RouteFixture::new(0);
    let event = fixture.event();
    fixture.world.insert(fixture.root, crate::ui::Hidden);
    let allocations = tracked_allocations(|| {
        fixture
            .pool
            .snapshot_gesture(&fixture.world, &event)
            .unwrap()
            .run(&mut fixture.world)
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert!(fixture.trace.borrow().values().is_empty());
    fixture.world.despawn(fixture.leaf);
    let recycled = fixture.world.spawn_empty();
    assert_eq!(fixture.leaf.id, recycled.id);
    assert_ne!(fixture.leaf.generation, recycled.generation);
    fixture
        .pool
        .snapshot_gesture(&fixture.world, &event)
        .unwrap()
        .run(&mut fixture.world)
        .unwrap();
    assert!(fixture.trace.borrow().values().is_empty());
}

#[test]
fn disabled_gesture_branches_skip_internal_and_author_hooks_without_using_steps() {
    for disabled in [0, 1, 2] {
        let mut fixture = RouteFixture::new(0);
        let node = [fixture.leaf, fixture.parent, fixture.root][disabled];
        fixture.world.insert(node, UserState::Disabled);
        let event = fixture.event();
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    fixture
                        .pool
                        .snapshot_gesture(&fixture.world, &event)
                        .unwrap()
                        .run(&mut fixture.world),
                    Ok(BubbleControl::Allow)
                );
            }),
            0
        );
        assert!(fixture.trace.borrow().values().is_empty());
        assert_eq!(fixture.pool.entries.borrow().listeners.len(), 4);
        fixture.world.insert(node, UserState::Errored);
        fixture
            .pool
            .reserve(ListenerCapacity {
                listeners: 4,
                snapshot_steps: 9,
            })
            .unwrap();
        assert_eq!(
            tracked_allocations(|| {
                fixture
                    .pool
                    .snapshot_gesture(&fixture.world, &event)
                    .unwrap()
                    .run(&mut fixture.world)
                    .unwrap();
            }),
            0
        );
        assert_eq!(fixture.trace.borrow().len, 7);
    }
}

#[test]
fn input_eligibility_is_frozen_with_the_route_not_rechecked_between_steps() {
    let mut fixture = RouteFixture::new(9);
    let event = fixture.event();
    let snapshot = fixture
        .pool
        .snapshot_gesture(&fixture.world, &event)
        .unwrap();
    fixture.world.insert(fixture.root, UserState::Disabled);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(snapshot.run(&mut fixture.world), Ok(BubbleControl::Allow));
        }),
        0
    );
    assert_eq!(fixture.trace.borrow().len, 7);
    fixture.trace.borrow_mut().len = 0;
    fixture.world.insert(fixture.leaf, Parent(fixture.parent));
    assert_eq!(
        tracked_allocations(|| {
            fixture
                .pool
                .snapshot_gesture(&fixture.world, &event)
                .unwrap()
                .run(&mut fixture.world)
                .unwrap();
        }),
        0
    );
    assert!(fixture.trace.borrow().values().is_empty());
}

#[test]
fn deleting_a_frozen_ancestor_does_not_dispatch_to_a_recycled_entity() {
    let mut fixture = RouteFixture::new(9);
    let event = fixture.event();
    let snapshot = fixture
        .pool
        .snapshot_gesture(&fixture.world, &event)
        .unwrap();
    fixture.world.despawn(fixture.parent);
    let recycled = fixture.world.spawn_empty();
    assert_eq!(fixture.parent.id, recycled.id);
    fixture.world.insert(recycled, RouteMarker);
    assert_eq!(snapshot.run(&mut fixture.world), Ok(BubbleControl::Allow));
    assert_eq!(
        fixture.trace.borrow().values(),
        &[102, 202, 302, 100, 200, 300]
    );
}

#[test]
fn internal_and_authored_prevention_stop_the_same_frozen_route() {
    let mut fixture = RouteFixture::new(9);
    let event = fixture.event();
    fixture
        .world
        .resource_mut::<RouteTrace>()
        .unwrap()
        .internal_stop = Some(fixture.leaf);
    assert_eq!(
        fixture
            .pool
            .snapshot_gesture(&fixture.world, &event)
            .unwrap()
            .run(&mut fixture.world),
        Ok(BubbleControl::Prevent)
    );
    assert_eq!(fixture.trace.borrow().values(), &[102]);
    fixture.trace.borrow_mut().len = 0;
    fixture.world.insert(fixture.leaf, Parent(fixture.parent));
    fixture.world.insert(fixture.parent, RouteMarker);
    let trace = fixture.world.resource_mut::<RouteTrace>().unwrap();
    trace.internal_stop = None;
    trace.author_stop = Some(fixture.parent);
    let allocations = tracked_allocations(|| {
        assert_eq!(
            fixture
                .pool
                .snapshot_gesture(&fixture.world, &event)
                .unwrap()
                .run(&mut fixture.world),
            Ok(BubbleControl::Prevent)
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(fixture.trace.borrow().values(), &[102, 202, 302, 301]);
}

#[test]
fn a_failed_snapshot_reservation_does_not_expand_listener_or_step_quotas() {
    let mut fixture = RouteFixture::new(9);
    assert_eq!(
        fixture.pool.reserve(ListenerCapacity {
            listeners: 8,
            snapshot_steps: usize::MAX,
        }),
        Err(ListenerError::AllocationFailed)
    );
    assert_eq!(fixture.pool.entries.borrow().limit, 4);
    assert_eq!(fixture.pool.scratch.borrow().limit, 9);
    assert_eq!(
        fixture
            .pool
            .install_entity::<DragMove, EntityContext<RouteMarker>>(
                &mut fixture.world,
                fixture.leaf,
                selector(),
                route_author::callback(),
                CommandCapacity::default(),
            ),
        Err(ListenerError::Capacity { available: 4 })
    );
    let event = fixture.event();
    let allocations = tracked_allocations(|| {
        fixture
            .pool
            .snapshot_gesture(&fixture.world, &event)
            .unwrap()
            .run(&mut fixture.world)
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(fixture.trace.borrow().len, 7);
}

struct Widget;

#[crate::event]
fn internal(_event: event!(DragMove), owner: res!(crate::ecs::world::WorldId)) -> bool {
    assert_eq!(crate::core::reactive::current_world_id(), Some(*owner));
    true
}

#[test]
fn internal_only_gesture_routes_prepare_and_restore_reactive_owner_without_allocation() {
    let mut world = World::new();
    let node = world.spawn_empty();
    let mut registry = ViewRegistry::default();
    registry.insert(
        crate::ui::view::View::systems_only("internal", &[])
            .with_inputs(&mut world, internal::callback()),
    );
    world.insert_resource(registry);
    world.insert_resource(world.id());
    crate::ui::lifecycle::attach_handlers_for(&mut world, node);
    let before = crate::core::reactive::current_world_id();
    let pool = EventListeners::new(&mut world);
    assert_eq!(crate::core::reactive::current_world_id(), before);
    pool.reserve(ListenerCapacity {
        listeners: 0,
        snapshot_steps: 1,
    })
    .unwrap();
    let event = DragMove {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        dx: Fixed::ONE,
        dy: Fixed::ZERO,
        target: node,
    };
    let allocations = tracked_allocations(|| {
        assert_eq!(
            pool.snapshot_gesture(&world, &event)
                .unwrap()
                .run(&mut world),
            Ok(BubbleControl::Prevent)
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(crate::core::reactive::current_world_id(), before);
}
impl Component for Widget {}

struct KeyboardTrace {
    trace: Rc<RefCell<Trace>>,
    author_stop: Option<Entity>,
    internal_stop: Option<Entity>,
}

#[crate::event]
fn keyboard_author(node: entity!(), trace: res!(KeyboardTrace)) -> bool {
    trace.trace.borrow_mut().record(100 + node.id);
    trace.author_stop == Some(node)
}

#[crate::event]
fn keyboard_app(trace: res!(KeyboardTrace)) -> bool {
    trace.trace.borrow_mut().record(900);
    false
}

#[crate::event]
fn keyboard_default(node: entity!(), trace: res!(KeyboardTrace)) -> bool {
    trace.trace.borrow_mut().record(200 + node.id);
    trace.internal_stop == Some(node)
}

struct DefaultEditor;
impl Component for DefaultEditor {}

fn keyboard_view(world: &mut World) -> crate::ui::view::View {
    crate::ui::view::View::systems_only("editor", &[])
        .with_filter::<DefaultEditor>()
        .with_inputs(
            world,
            (
                keyboard_default::callback::<Key, super::super::callback::NodeContext>(),
                keyboard_default::callback::<
                    super::super::input::TextCommitted,
                    super::super::callback::NodeContext,
                >(),
            ),
        )
}

struct KeyboardFixture {
    world: World,
    pool: EventListeners,
    trace: Rc<RefCell<Trace>>,
    root: Entity,
    leaf: Entity,
}

impl KeyboardFixture {
    fn new(steps: usize) -> Self {
        let mut world = World::new();
        let root = world.spawn_empty();
        let leaf = world.spawn_empty();
        world.insert(leaf, Parent(root));
        world.insert_resource(FocusState {
            focused: Some(leaf),
        });
        let trace = Rc::new(RefCell::new(Trace::default()));
        world.insert_resource(KeyboardTrace {
            trace: trace.clone(),
            author_stop: None,
            internal_stop: None,
        });
        let mut registry = ViewRegistry::default();
        registry.insert(keyboard_view(&mut world));
        world.insert_resource(registry);
        let pool = EventListeners::new(&mut world);
        pool.reserve(ListenerCapacity {
            listeners: 6,
            snapshot_steps: steps,
        })
        .unwrap();
        for node in [root, leaf] {
            world.insert(node, DefaultEditor);
            crate::ui::lifecycle::attach_handlers_for(&mut world, node);
            pool.install_entity::<Key, EntityContext<Widget>>(
                &mut world,
                node,
                selector(),
                keyboard_author::callback(),
                CommandCapacity::default(),
            )
            .unwrap();
            pool.install_entity::<super::super::input::TextCommitted, EntityContext<Widget>>(
                &mut world,
                node,
                selector(),
                keyboard_author::callback(),
                CommandCapacity::default(),
            )
            .unwrap();
        }
        pool.install_app::<Key>(
            &mut world,
            selector(),
            keyboard_app::callback(),
            CommandCapacity::default(),
        )
        .unwrap();
        pool.install_app::<super::super::input::TextCommitted>(
            &mut world,
            selector(),
            keyboard_app::callback(),
            CommandCapacity::default(),
        )
        .unwrap();
        Self {
            world,
            pool,
            trace,
            root,
            leaf,
        }
    }

    fn dispatch<E>(&mut self, event: &E) -> Result<BubbleControl, ListenerError>
    where
        E: InputPayload + Event<Source = InputSource>,
    {
        self.pool
            .snapshot_keyboard(&self.world, event)?
            .run(&mut self.world)
    }
}

#[test]
fn keyboard_and_character_routes_order_authors_defaults_ancestors_and_app_without_allocation() {
    let mut fixture = KeyboardFixture::new(5);
    let key = Key {
        code: 7,
        pressed: false,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    let before = crate::core::reactive::current_world_id();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Allow));
        }),
        0
    );
    assert_eq!(fixture.trace.borrow().values(), &[101, 201, 100, 200, 900]);
    fixture.trace.borrow_mut().len = 0;
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                fixture.dispatch(&super::super::input::TextCommitted::new("界")),
                Ok(BubbleControl::Allow)
            );
        }),
        0
    );
    assert_eq!(fixture.trace.borrow().values(), &[101, 201, 100, 200, 900]);
    assert_eq!(crate::core::reactive::current_world_id(), before);
}

#[test]
fn keyboard_consumption_stops_before_later_defaults_and_fallback() {
    let mut fixture = KeyboardFixture::new(5);
    let key = Key {
        code: 7,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    fixture
        .world
        .resource_mut::<KeyboardTrace>()
        .unwrap()
        .author_stop = Some(fixture.leaf);
    assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Prevent));
    assert_eq!(fixture.trace.borrow().values(), &[101]);
    fixture.trace.borrow_mut().len = 0;
    let trace = fixture.world.resource_mut::<KeyboardTrace>().unwrap();
    trace.author_stop = None;
    trace.internal_stop = Some(fixture.leaf);
    assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Prevent));
    assert_eq!(fixture.trace.borrow().values(), &[101, 201]);
}

#[test]
fn keyboard_explicit_origin_does_not_read_or_change_current_focus() {
    let mut fixture = KeyboardFixture::new(5);
    let text = super::super::input::TextCommitted::new("界");
    fixture.world.resource_mut::<FocusState>().unwrap().focused = Some(fixture.root);
    assert_eq!(
        tracked_allocations(|| {
            fixture
                .pool
                .snapshot_keyboard_from(&fixture.world, &text, Some(fixture.leaf))
                .unwrap()
                .run(&mut fixture.world)
                .unwrap();
        }),
        0
    );
    assert_eq!(fixture.trace.borrow().values(), &[101, 201, 100, 200, 900]);
    assert_eq!(
        fixture.world.resource::<FocusState>().unwrap().focused,
        Some(fixture.root)
    );
    fixture.trace.borrow_mut().len = 0;
    fixture
        .pool
        .snapshot_keyboard_from(&fixture.world, &text, None)
        .unwrap()
        .run(&mut fixture.world)
        .unwrap();
    assert_eq!(fixture.trace.borrow().values(), &[900]);
    fixture.trace.borrow_mut().len = 0;
    fixture.dispatch(&text).unwrap();
    assert_eq!(fixture.trace.borrow().values(), &[100, 200, 900]);
}

#[test]
fn keyboard_selection_freezes_focus_and_parents_but_skips_retired_defaults() {
    let mut fixture = KeyboardFixture::new(5);
    let key = Key {
        code: 7,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    let snapshot = fixture
        .pool
        .snapshot_keyboard(&fixture.world, &key)
        .unwrap();
    fixture.world.remove::<Parent>(fixture.leaf);
    fixture.world.remove::<DefaultEditor>(fixture.root);
    fixture.world.resource_mut::<FocusState>().unwrap().focused = Some(fixture.root);
    assert_eq!(snapshot.run(&mut fixture.world), Ok(BubbleControl::Allow));
    assert_eq!(fixture.trace.borrow().values(), &[101, 201, 100, 900]);
    fixture.trace.borrow_mut().len = 0;
    assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Allow));
    assert_eq!(fixture.trace.borrow().values(), &[100, 900]);
}

#[test]
fn keyboard_suppressed_or_stale_focus_only_reaches_app_fallback() {
    let mut fixture = KeyboardFixture::new(1);
    let key = Key {
        code: 7,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    fixture.world.insert(fixture.root, UserState::Disabled);
    assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Allow));
    assert_eq!(fixture.trace.borrow().values(), &[900]);
    fixture.trace.borrow_mut().len = 0;
    fixture.world.remove::<UserState>(fixture.root);
    fixture.world.insert(fixture.root, crate::ui::Hidden);
    assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Allow));
    assert_eq!(fixture.trace.borrow().values(), &[900]);
    fixture.trace.borrow_mut().len = 0;
    fixture.world.despawn(fixture.leaf);
    let replacement = fixture.world.spawn_empty();
    assert_eq!(replacement.id, fixture.leaf.id);
    fixture.world.insert(replacement, DefaultEditor);
    assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Allow));
    assert_eq!(fixture.trace.borrow().values(), &[900]);
    fixture.trace.borrow_mut().len = 0;
    fixture.world.resource_mut::<FocusState>().unwrap().focused = None;
    assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Allow));
    assert_eq!(fixture.trace.borrow().values(), &[900]);
}

#[test]
fn keyboard_capacity_and_cycles_fail_before_any_body_and_release_scratch() {
    let mut fixture = KeyboardFixture::new(4);
    let key = Key {
        code: 7,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    assert_eq!(
        tracked_allocations(|| {
            assert!(matches!(
                fixture.dispatch(&key),
                Err(ListenerError::SnapshotCapacity { available: 4 })
            ));
        }),
        0
    );
    assert_eq!(fixture.trace.borrow().len, 0);
    fixture.world.insert(fixture.root, Parent(fixture.leaf));
    assert_eq!(
        tracked_allocations(|| {
            assert!(matches!(
                fixture.dispatch(&key),
                Err(ListenerError::CyclicRoute { .. })
            ));
        }),
        0
    );
    assert_eq!(fixture.trace.borrow().len, 0);
    fixture.world.remove::<Parent>(fixture.root);
    fixture
        .pool
        .reserve(ListenerCapacity {
            listeners: 6,
            snapshot_steps: 5,
        })
        .unwrap();
    assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Allow));
    assert_eq!(fixture.trace.borrow().values(), &[101, 201, 100, 200, 900]);
}

#[test]
fn keyboard_snapshot_skips_destroyed_generations_without_retargeting_replacements() {
    let mut fixture = KeyboardFixture::new(5);
    let key = Key {
        code: 7,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    let snapshot = fixture
        .pool
        .snapshot_keyboard(&fixture.world, &key)
        .unwrap();
    fixture.world.despawn(fixture.leaf);
    let replacement = fixture.world.spawn_empty();
    fixture.world.insert(replacement, DefaultEditor);
    assert_eq!(snapshot.run(&mut fixture.world), Ok(BubbleControl::Allow));
    assert_eq!(fixture.trace.borrow().values(), &[100, 200, 900]);
}

#[test]
fn unchanged_editor_actions_are_accepted_without_reaching_app_fallback() {
    use crate::ui::widgets::TextEditing;
    let mut fixture = KeyboardFixture::new(5);
    fixture.world.insert(fixture.leaf, TextEditing::default());
    let view = crate::ui::widgets::text_editing::view();
    fixture.world.remove::<DefaultEditor>(fixture.leaf);
    view.auto_attach()
        .unwrap()
        .invoke(&mut fixture.world, fixture.leaf);
    super::super::view_input::fixture_inputs(view, &mut fixture.world);
    crate::ui::lifecycle::attach_handlers_for(&mut fixture.world, fixture.leaf);
    let changes = Rc::new(Cell::new(0));
    let called = changes.clone();
    // SAFETY: the only capture is an independent test counter, not a Model.
    let changed = unsafe {
        TextEditing::Changed
            .__bind_plain::<super::super::callback::NodeContext, ()>("changed", move |_, _scope| {
                called.set(called.get() + 1)
            })
    };
    super::super::binding::install_entity_event(
        &mut fixture.world,
        fixture.leaf,
        TextEditing::Changed,
        changed,
    );
    for code in [
        super::super::input::KEY_BACKSPACE,
        super::super::input::KEY_DELETE,
        super::super::input::KEY_LEFT,
        super::super::input::KEY_RIGHT,
        super::super::input::KEY_HOME,
        super::super::input::KEY_END,
    ] {
        fixture.trace.borrow_mut().len = 0;
        let key = Key {
            code,
            pressed: true,
            modifiers: Some(Default::default()),
            repeat: Some(false),
        };
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(fixture.dispatch(&key), Ok(BubbleControl::Prevent));
            }),
            0
        );
        assert_eq!(fixture.trace.borrow().values(), &[101]);
    }
    assert_eq!(changes.get(), 0);
    assert_eq!(
        fixture
            .world
            .get::<TextEditing>(fixture.leaf)
            .unwrap()
            .len(),
        0
    );
}

struct Changed(u32);
impl Event for Changed {
    type Route = EntityNotification;
    type Source = WidgetSource<Widget>;
    type Options = ();
}

struct Announced(u32);
impl Event for Announced {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

struct Owned(Rc<Cell<usize>>);
impl Drop for Owned {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
impl Event for Owned {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

#[derive(Default)]
struct Trace {
    values: [u32; 16],
    len: usize,
}
impl Trace {
    fn record(&mut self, value: u32) {
        self.values[self.len] = value;
        self.len += 1;
    }
    fn values(&self) -> &[u32] {
        &self.values[..self.len]
    }
}

fn app_callback<E>(body: impl Fn(&E) + 'static) -> EventCallback<E, AppContext>
where
    E: Event<Route = AppNotification, Source = AppSource, Options = ()>,
{
    // SAFETY: this body fetches no World parameters and captures no Models.
    unsafe {
        event_callback::<E, AppContext, (), ()>("announce", &[], move |(), event, (), _scope| {
            body(event)
        })
    }
}

fn selector<E: Event<Options = ()>>() -> ListenerSelector<E> {
    ListenerSelector::new(E::EVENT, |(), _| true)
}

struct ScrollTrace {
    trace: RefCell<Trace>,
    wheel: Cell<Option<super::super::input::Wheel>>,
    rotary: Cell<Option<super::super::input::Rotary>>,
}

#[crate::event]
fn wheel_fallback(
    tag: u32,
    consume: bool,
    input: event!(super::super::input::Wheel),
    trace: res!(ScrollTrace),
) -> bool {
    trace.trace.borrow_mut().record(tag);
    trace.wheel.set(Some(*input));
    consume
}

#[crate::event]
fn rotary_fallback(input: event!(super::super::input::Rotary), trace: res!(ScrollTrace)) -> bool {
    trace.trace.borrow_mut().record(900);
    trace.rotary.set(Some(*input));
    false
}

struct ScrollFixture {
    world: World,
    root: Entity,
    pool: EventListeners,
}

impl ScrollFixture {
    fn new(steps: usize) -> Self {
        use crate::input::event::scroll::{
            ScrollConfig, ScrollDragState, ScrollMotion, ScrollOffset,
        };
        use crate::types::{Dimension, Viewport};
        use crate::ui::layout::LayoutStyle;

        let mut world = World::new();
        let root = world.spawn(crate::ui::Widget);
        world.insert(
            root,
            crate::ui::Style {
                layout: LayoutStyle {
                    width: Dimension::px(100),
                    height: Dimension::px(100),
                    ..LayoutStyle::default()
                },
                ..crate::ui::Style::default()
            },
        );
        world.insert(root, ScrollOffset::default());
        world.insert(
            root,
            ScrollConfig {
                content_height: Fixed::from_int(300),
                elastic: false,
                ..ScrollConfig::default()
            },
        );
        world.insert_resource(ScrollDragState {
            last_resolved_target: Some(root),
            ..ScrollDragState::default()
        });
        world.insert_resource(ScrollMotion::default());
        world.insert_resource(ScrollTrace {
            trace: RefCell::new(Trace::default()),
            wheel: Cell::new(None),
            rotary: Cell::new(None),
        });
        crate::ui::render_system::try_update_layout(
            &mut world,
            root,
            &Viewport::new(100, 100, Fixed::ONE),
        )
        .unwrap();
        let pool = EventListeners::new(&mut world);
        pool.reserve(ListenerCapacity {
            listeners: 4,
            snapshot_steps: steps,
        })
        .unwrap();
        for (tag, consume) in [(100, false), (200, true), (300, false)] {
            pool.install_app(
                &mut world,
                selector(),
                wheel_fallback::callback(tag, consume),
                CommandCapacity::default(),
            )
            .unwrap();
        }
        pool.install_app(
            &mut world,
            selector(),
            rotary_fallback::callback(),
            CommandCapacity::default(),
        )
        .unwrap();
        Self { world, root, pool }
    }

    fn wheel(
        &mut self,
        input: &super::super::input::Wheel,
    ) -> Result<BubbleControl, ListenerError> {
        self.pool
            .dispatch_scroll(&mut self.world, self.root, input, 100, 100)
    }

    fn rotary(
        &mut self,
        input: &super::super::input::Rotary,
    ) -> Result<BubbleControl, ListenerError> {
        self.pool
            .dispatch_scroll(&mut self.world, self.root, input, 100, 100)
    }
}

#[test]
fn scroll_claim_precedes_app_and_does_not_require_fallback_capacity() {
    use crate::input::event::input::{Rotary, Wheel};
    use crate::input::event::scroll::{ScrollMotion, ScrollOffset};

    let mut fixture = ScrollFixture::new(0);
    fixture.world.invalidate_visual(fixture.root);
    let wheel = Wheel {
        dx: Fixed::ZERO,
        dy: Fixed::from_int(-20),
        x: 20.into(),
        y: 20.into(),
    };
    let rotary = Rotary { id: 7, delta: -2 };
    let owner = crate::core::reactive::current_world_id();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(fixture.wheel(&wheel), Ok(BubbleControl::Prevent));
            assert_eq!(fixture.rotary(&rotary), Ok(BubbleControl::Prevent));
        }),
        0
    );
    assert!(
        fixture
            .world
            .resource::<ScrollTrace>()
            .unwrap()
            .trace
            .borrow()
            .values()
            .is_empty()
    );
    assert_eq!(
        fixture.world.get::<ScrollOffset>(fixture.root).unwrap().y,
        60.into()
    );
    assert_eq!(crate::core::reactive::current_world_id(), owner);
    fixture
        .world
        .get_mut::<ScrollOffset>(fixture.root)
        .unwrap()
        .y = 200.into();
    fixture.world.insert_resource(ScrollMotion::default());
    assert_eq!(fixture.wheel(&wheel), Ok(BubbleControl::Prevent));
    assert!(
        fixture
            .world
            .resource::<ScrollTrace>()
            .unwrap()
            .wheel
            .get()
            .is_none()
    );
}

#[test]
fn unaccepted_scroll_preserves_payload_and_consumes_in_app_installation_order() {
    use crate::input::event::input::{Rotary, Wheel};
    use crate::input::event::scroll::{ScrollDragState, ScrollOffset};

    let mut fixture = ScrollFixture::new(3);
    fixture.world.remove::<ScrollOffset>(fixture.root);
    fixture
        .world
        .resource_mut::<ScrollDragState>()
        .unwrap()
        .last_resolved_target = None;
    let wheel = Wheel {
        dx: (-2).into(),
        dy: 3.into(),
        x: 42.into(),
        y: 57.into(),
    };
    let rotary = Rotary { id: 9, delta: -3 };
    for _ in 0..2 {
        fixture
            .world
            .resource::<ScrollTrace>()
            .unwrap()
            .trace
            .borrow_mut()
            .len = 0;
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(fixture.wheel(&wheel), Ok(BubbleControl::Prevent));
                assert_eq!(fixture.rotary(&rotary), Ok(BubbleControl::Allow));
            }),
            0
        );
        let trace = fixture.world.resource::<ScrollTrace>().unwrap();
        assert_eq!(trace.trace.borrow().values(), &[100, 200, 900]);
        assert_eq!(trace.wheel.get(), Some(wheel));
        assert_eq!(trace.rotary.get(), Some(rotary));
    }
}

#[test]
fn scroll_fallback_capacity_failure_runs_no_partial_listener_and_reuses_scratch() {
    use crate::input::event::input::Wheel;
    let mut fixture = ScrollFixture::new(2);
    let wheel = Wheel {
        dx: Fixed::ONE,
        dy: Fixed::ZERO,
        x: 20.into(),
        y: 20.into(),
    };
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                fixture.wheel(&wheel),
                Err(ListenerError::SnapshotCapacity { available: 2 })
            );
        }),
        0
    );
    assert!(fixture.pool.scratch.borrow().steps.is_empty());
    assert!(
        fixture
            .world
            .resource::<ScrollTrace>()
            .unwrap()
            .trace
            .borrow()
            .values()
            .is_empty()
    );
    fixture
        .pool
        .reserve(ListenerCapacity {
            listeners: 4,
            snapshot_steps: 3,
        })
        .unwrap();
    assert_eq!(fixture.wheel(&wheel), Ok(BubbleControl::Prevent));
    assert_eq!(
        fixture
            .world
            .resource::<ScrollTrace>()
            .unwrap()
            .trace
            .borrow()
            .values(),
        &[100, 200]
    );
}

#[test]
fn scroll_route_rejects_busy_or_foreign_owner_before_mutating_scroll() {
    use crate::input::event::input::Wheel;
    use crate::input::event::scroll::{ScrollMotion, ScrollOffset};
    let mut fixture = ScrollFixture::new(3);
    let wheel = Wheel {
        dx: Fixed::ZERO,
        dy: -Fixed::ONE,
        x: 20.into(),
        y: 20.into(),
    };
    let held = fixture
        .pool
        .snapshot(&fixture.world, &wheel, ListenerTarget::App)
        .unwrap();
    assert_eq!(
        fixture
            .pool
            .dispatch_scroll(&mut fixture.world, fixture.root, &wheel, 100, 100),
        Err(ListenerError::Busy)
    );
    assert!(
        fixture
            .world
            .resource::<ScrollMotion>()
            .unwrap()
            .target_entity
            .is_none()
    );
    drop(held);
    let mut foreign = ScrollFixture::new(3);
    assert_eq!(
        fixture
            .pool
            .dispatch_scroll(&mut foreign.world, foreign.root, &wheel, 100, 100),
        Err(ListenerError::ForeignOwner)
    );
    assert!(
        foreign
            .world
            .resource::<ScrollMotion>()
            .unwrap()
            .target_entity
            .is_none()
    );
    assert_eq!(
        foreign.world.get::<ScrollOffset>(foreign.root).unwrap().y,
        Fixed::ZERO
    );
    assert_eq!(fixture.wheel(&wheel), Ok(BubbleControl::Prevent));
}

#[test]
fn retired_rotary_target_reaches_app_without_dirtying_recycled_entity() {
    use crate::input::event::input::Rotary;
    use crate::input::event::scroll::{ScrollDragState, ScrollOffset};
    let mut fixture = ScrollFixture::new(3);
    let target = fixture.world.spawn(ScrollOffset::default());
    fixture
        .world
        .resource_mut::<ScrollDragState>()
        .unwrap()
        .last_resolved_target = Some(target);
    assert!(fixture.world.despawn(target));
    let replacement = fixture.world.spawn(ScrollOffset::default());
    assert_eq!(replacement.id, target.id);
    assert_ne!(replacement.generation, target.generation);
    let rotary = Rotary { id: 3, delta: -8 };
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(fixture.rotary(&rotary), Ok(BubbleControl::Allow));
        }),
        0
    );
    assert_eq!(
        fixture
            .world
            .resource::<ScrollTrace>()
            .unwrap()
            .rotary
            .get(),
        Some(rotary)
    );
    assert_eq!(
        fixture.world.get::<ScrollOffset>(replacement).unwrap().y,
        Fixed::ZERO
    );
    assert!(!fixture.world.has::<crate::ui::dirty::Dirty>(replacement));
}

#[crate::model]
struct Counter {
    #[observe]
    value: u32,
}

#[crate::model]
impl Counter {
    fn add(&mut self, value: u32) {
        self.value += value;
    }
}

struct Position(u32);
impl Component for Position {}

#[crate::event]
fn record_change(
    counter: model!(Counter),
    event: event!(Changed),
    node: entity!(),
    positions: query!(mut Position),
) {
    counter.add(event.0);
    positions.get(node).unwrap().0 += event.0;
}

#[test]
fn named_factory_receives_owned_notification_through_the_installed_pool() {
    let mut app = crate::app::App::headless(8, 8);
    let counter = app.add_model(Counter { value: 0 }).finish();
    let node = app.world.spawn_empty();
    app.world.insert(node, Widget);
    app.world.insert(node, Position(2));
    let pool = EventListeners::new(&mut app.world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    pool.install_entity(
        &mut app.world,
        node,
        selector(),
        record_change::callback::<EntityContext<Widget>>(counter.clone()),
        CommandCapacity::default(),
    )
    .unwrap();
    let queue = NotificationQueue::default();
    queue
        .reserve(NotificationCapacity {
            records: 1,
            payload_bytes: size_of::<Changed>(),
            payload_alignment: core::mem::align_of::<Changed>(),
        })
        .unwrap();
    let allocations = tracked_allocations(|| {
        queue.push_entity(node, Changed(5)).ok().unwrap();
        assert_eq!(pool.flush_notifications(&mut app.world, &queue, 1), Ok(1));
        assert_eq!(counter.value(), 5);
        assert_eq!(app.world.get::<Position>(node).unwrap().0, 7);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn disabled_or_hidden_widgets_still_receive_queued_business_notifications() {
    let mut app = crate::app::App::headless(8, 8);
    let counter = app.add_model(Counter { value: 0 }).finish();
    let node = app.world.spawn_empty();
    app.world.insert(node, Widget);
    app.world.insert(node, Position(0));
    let pool = EventListeners::new(&mut app.world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    pool.install_entity(
        &mut app.world,
        node,
        selector(),
        record_change::callback::<EntityContext<Widget>>(counter.clone()),
        CommandCapacity::default(),
    )
    .unwrap();
    let queue = NotificationQueue::default();
    queue
        .reserve(NotificationCapacity {
            records: 1,
            payload_bytes: size_of::<Changed>(),
            payload_alignment: core::mem::align_of::<Changed>(),
        })
        .unwrap();
    for hidden in [false, true] {
        app.world.insert(node, UserState::Disabled);
        if hidden {
            app.world.insert(node, crate::ui::Hidden);
        }
        assert_eq!(
            tracked_allocations(|| {
                queue.push_entity(node, Changed(1)).ok().unwrap();
                assert_eq!(pool.flush_notifications(&mut app.world, &queue, 1), Ok(1));
            }),
            0
        );
    }
    assert_eq!(counter.value(), 2);
    assert_eq!(app.world.get::<Position>(node).unwrap().0, 2);
}

#[test]
fn erased_delivery_preserves_high_alignment_without_requiring_zst_bytes() {
    #[repr(align(128))]
    struct Pulse;
    impl Event for Pulse {
        type Route = AppNotification;
        type Source = AppSource;
        type Options = ();
    }
    let mut world = World::new();
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    let called = Rc::new(Cell::new(0));
    let seen = called.clone();
    pool.install_app(
        &mut world,
        selector(),
        app_callback(move |pulse: &Pulse| {
            assert_eq!(
                core::ptr::from_ref(pulse).addr() % core::mem::align_of::<Pulse>(),
                0
            );
            seen.set(seen.get() + 1);
        }),
        CommandCapacity::default(),
    )
    .unwrap();
    let queue = NotificationQueue::default();
    queue
        .reserve(NotificationCapacity {
            records: 1,
            payload_bytes: 0,
            payload_alignment: 128,
        })
        .unwrap();
    let allocations = tracked_allocations(|| {
        queue.push_app(Pulse).ok().unwrap();
        assert_eq!(pool.flush_notifications(&mut world, &queue, 1), Ok(1));
    });
    assert_eq!(allocations, 0);
    assert_eq!(called.get(), 1);
}

#[test]
fn notification_fifo_and_fanout_use_the_prepared_executor_without_allocation() {
    let mut world = World::new();
    let node = world.spawn_empty();
    world.insert(node, Widget);
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 3,
        snapshot_steps: 3,
    })
    .unwrap();
    let queue = Rc::new(NotificationQueue::default());
    queue
        .reserve(NotificationCapacity {
            records: 3,
            payload_bytes: 12,
            payload_alignment: 4,
        })
        .unwrap();
    let trace = Rc::new(RefCell::new(Trace::default()));
    for base in [100, 200] {
        let trace = trace.clone();
        let queue = queue.clone();
        // SAFETY: this body has no World parameters or captured Models.
        let callback = unsafe {
            event_callback::<Changed, EntityContext<Widget>, (), ()>(
                "changed",
                &[],
                move |(), event, target, _scope| {
                    assert_eq!(target, node);
                    trace.borrow_mut().record(base + event.0);
                    if base == 100 && event.0 == 1 {
                        queue.push_entity(node, Changed(9)).ok().unwrap();
                    }
                },
            )
        };
        pool.install_entity(
            &mut world,
            node,
            selector(),
            callback,
            CommandCapacity::default(),
        )
        .unwrap();
    }
    let seen = trace.clone();
    pool.install_app(
        &mut world,
        selector(),
        app_callback(move |event: &Announced| seen.borrow_mut().record(300 + event.0)),
        CommandCapacity::default(),
    )
    .unwrap();
    let allocations = tracked_allocations(|| {
        queue.push_entity(node, Changed(1)).ok().unwrap();
        queue.push_app(Announced(2)).ok().unwrap();
        queue.push_entity(node, Changed(3)).ok().unwrap();
        assert_eq!(pool.flush_notifications(&mut world, &queue, 4), Ok(4));
    });
    assert_eq!(allocations, 0);
    assert_eq!(
        trace.borrow().values(),
        &[101, 201, 302, 103, 203, 109, 209]
    );
}

#[test]
fn entry_changes_do_not_change_a_current_snapshot_or_reuse_listener_identity() {
    let mut world = World::new();
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 3,
        snapshot_steps: 3,
    })
    .unwrap();
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut handles = [None, None];
    for (index, value) in [1, 2].into_iter().enumerate() {
        let trace = trace.clone();
        handles[index] = Some(
            pool.install_app(
                &mut world,
                selector(),
                app_callback(move |_: &Announced| trace.borrow_mut().record(value)),
                CommandCapacity::default(),
            )
            .unwrap(),
        );
    }
    let event = Announced(0);
    let snapshot = pool.snapshot(&world, &event, ListenerTarget::App).unwrap();
    let old = handles[0].unwrap();
    assert_eq!(pool.remove(old), Ok(true));
    let seen = trace.clone();
    let new = pool
        .install_app(
            &mut world,
            selector(),
            app_callback(move |_: &Announced| seen.borrow_mut().record(3)),
            CommandCapacity::default(),
        )
        .unwrap();
    assert_ne!(old, new);
    assert_eq!(pool.remove(old), Ok(false));
    let allocations = tracked_allocations(|| {
        assert_eq!(snapshot.run(&mut world), Ok(BubbleControl::Allow));
        pool.snapshot(&world, &event, ListenerTarget::App)
            .unwrap()
            .run(&mut world)
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(trace.borrow().values(), &[1, 2, 2, 3]);
}

#[test]
fn callback_removal_takes_effect_on_the_following_notification() {
    let mut world = World::new();
    let pool = Rc::new(EventListeners::new(&mut world));
    pool.reserve(ListenerCapacity {
        listeners: 2,
        snapshot_steps: 2,
    })
    .unwrap();
    let remove = Rc::new(Cell::new(None));
    let seen = Rc::new(Cell::new(0));
    let weak = Rc::downgrade(&pool);
    let id = remove.clone();
    let first = pool
        .install_app(
            &mut world,
            selector(),
            app_callback(move |_: &Announced| {
                assert!(weak.upgrade().unwrap().remove(id.take().unwrap()).unwrap());
            }),
            CommandCapacity::default(),
        )
        .unwrap();
    let counter = seen.clone();
    let other = pool
        .install_app(
            &mut world,
            selector(),
            app_callback(move |_: &Announced| counter.set(counter.get() + 1)),
            CommandCapacity::default(),
        )
        .unwrap();
    remove.set(Some(other));
    pool.snapshot(&world, &Announced(0), ListenerTarget::App)
        .unwrap()
        .run(&mut world)
        .unwrap();
    assert_eq!(seen.get(), 1);
    pool.remove(first).unwrap();
    pool.snapshot(&world, &Announced(0), ListenerTarget::App)
        .unwrap()
        .run(&mut world)
        .unwrap();
    assert_eq!(seen.get(), 1);
}

#[test]
fn retiring_entities_releases_slots_without_rewriting_frozen_snapshots() {
    let mut world = World::new();
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    let node = world.spawn_empty();
    world.insert(node, Widget);
    let drops = Rc::new(Cell::new(0));
    let captured = Owned(drops.clone());
    let seen = Rc::new(Cell::new(0));
    let calls = seen.clone();
    // SAFETY: the callback declares no World access or captured Model.
    let callback = unsafe {
        event_callback::<Changed, EntityContext<Widget>, (), ()>(
            "retired",
            &[],
            move |(), _, _, _scope| {
                let _ = &captured;
                calls.set(calls.get() + 1);
            },
        )
    };
    let old = pool
        .install_entity(
            &mut world,
            node,
            selector(),
            callback,
            CommandCapacity::default(),
        )
        .unwrap();
    let event = Changed(1);
    let snapshot = pool
        .snapshot(&world, &event, ListenerTarget::Entity(node))
        .unwrap();
    assert!(world.despawn(node));
    let reused = world.spawn_empty();
    assert_eq!(reused.id, node.id);
    assert_ne!(reused.generation, node.generation);
    world.insert(reused, Widget);

    let calls = seen.clone();
    // SAFETY: the callback declares no World access or captured Model.
    let callback = unsafe {
        event_callback::<Changed, EntityContext<Widget>, (), ()>(
            "replacement",
            &[],
            move |(), _, _, _scope| calls.set(calls.get() + 10),
        )
    };
    let new = pool
        .install_entity(
            &mut world,
            reused,
            selector(),
            callback,
            CommandCapacity::default(),
        )
        .expect("retired listeners cannot consume the bounded installation quota");
    assert_ne!(new, old);
    assert_eq!(pool.remove(old), Ok(false));
    assert_eq!(
        drops.get(),
        0,
        "the frozen snapshot still owns its callback"
    );
    let allocations = tracked_allocations(|| {
        snapshot.run(&mut world).unwrap();
        pool.snapshot(&world, &event, ListenerTarget::Entity(reused))
            .unwrap()
            .run(&mut world)
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(seen.get(), 10);
    assert_eq!(drops.get(), 1);
}

#[test]
fn fresh_selection_reclaims_retired_callbacks_and_preserves_live_installation_order() {
    let mut world = World::new();
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 3,
        snapshot_steps: 2,
    })
    .unwrap();
    let node = world.spawn_empty();
    world.insert(node, Widget);
    let drops = Rc::new(Cell::new(0));
    let trace = Rc::new(RefCell::new(Trace::default()));
    let first_trace = trace.clone();
    let first = pool
        .install_app(
            &mut world,
            selector(),
            app_callback(move |_: &Announced| first_trace.borrow_mut().record(1)),
            CommandCapacity::default(),
        )
        .unwrap();
    let captured = Owned(drops.clone());
    // SAFETY: the callback declares no World access or captured Model.
    let callback = unsafe {
        event_callback::<Changed, EntityContext<Widget>, (), ()>(
            "retired",
            &[],
            move |(), _, _, _scope| {
                let _ = &captured;
            },
        )
    };
    let retired = pool
        .install_entity(
            &mut world,
            node,
            selector(),
            callback,
            CommandCapacity::default(),
        )
        .unwrap();
    let last_trace = trace.clone();
    let last = pool
        .install_app(
            &mut world,
            selector(),
            app_callback(move |_: &Announced| last_trace.borrow_mut().record(2)),
            CommandCapacity::default(),
        )
        .unwrap();
    let storage = pool.entries.borrow().listeners.as_ptr();
    assert!(world.despawn(node));
    let allocations = tracked_allocations(|| {
        pool.snapshot(&world, &Announced(0), ListenerTarget::App)
            .unwrap()
            .run(&mut world)
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(drops.get(), 1);
    assert_eq!(trace.borrow().values(), &[1, 2]);
    let entries = pool.entries.borrow();
    assert_eq!(entries.listeners.as_ptr(), storage);
    assert_eq!(entries.listeners.len(), 2);
    assert_eq!(entries.listeners[0].id, first);
    assert_eq!(entries.listeners[1].id, last);
    drop(entries);
    assert_eq!(pool.remove(retired), Ok(false));
}

#[test]
fn retirement_drops_captures_outside_the_entry_borrow_and_restores_scratch_on_unwind() {
    struct Capture {
        pool: alloc::rc::Weak<EventListeners>,
        other: ListenerId,
        drops: Rc<Cell<usize>>,
        panic_once: Rc<Cell<bool>>,
    }
    impl Drop for Capture {
        fn drop(&mut self) {
            assert_eq!(self.pool.upgrade().unwrap().remove(self.other), Ok(true));
            self.drops.set(self.drops.get() + 1);
            if self.panic_once.replace(false) {
                panic!("captured value destructor");
            }
        }
    }
    let mut world = World::new();
    let pool = Rc::new(EventListeners::new(&mut world));
    pool.reserve(ListenerCapacity {
        listeners: 2,
        snapshot_steps: 2,
    })
    .unwrap();
    let other = pool
        .install_app(
            &mut world,
            selector(),
            app_callback(|_: &Announced| {}),
            CommandCapacity::default(),
        )
        .unwrap();
    let node = world.spawn_empty();
    world.insert(node, Widget);
    let drops = Rc::new(Cell::new(0));
    let captured = Capture {
        pool: Rc::downgrade(&pool),
        other,
        drops: drops.clone(),
        panic_once: Rc::new(Cell::new(true)),
    };
    // SAFETY: the callback declares no World access or captured Model.
    let callback = unsafe {
        event_callback::<Changed, EntityContext<Widget>, (), ()>(
            "retired",
            &[],
            move |(), _, _, _scope| {
                let _ = &captured;
            },
        )
    };
    pool.install_entity(
        &mut world,
        node,
        selector(),
        callback,
        CommandCapacity::default(),
    )
    .unwrap();
    assert!(world.despawn(node));
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = pool.snapshot(&world, &Announced(0), ListenerTarget::App);
    }));
    assert!(failure.is_err());
    assert_eq!(drops.get(), 1);
    assert!(pool.entries.borrow().listeners.is_empty());
    let allocations = tracked_allocations(|| {
        pool.snapshot(&world, &Announced(0), ListenerTarget::App)
            .unwrap()
            .run(&mut world)
            .unwrap();
    });
    assert_eq!(allocations, 0);
}

#[test]
fn owner_and_busy_checks_do_not_retire_captures_before_rejecting_selection() {
    let mut world = World::new();
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    let node = world.spawn_empty();
    world.insert(node, Widget);
    let drops = Rc::new(Cell::new(0));
    let captured = Owned(drops.clone());
    // SAFETY: the callback declares no World access or captured Model.
    let callback = unsafe {
        event_callback::<Changed, EntityContext<Widget>, (), ()>(
            "retired",
            &[],
            move |(), _, _, _scope| {
                let _ = &captured;
            },
        )
    };
    let id = pool
        .install_entity(
            &mut world,
            node,
            selector(),
            callback,
            CommandCapacity::default(),
        )
        .unwrap();
    let held = pool
        .snapshot(&world, &Announced(0), ListenerTarget::App)
        .unwrap();
    assert!(world.despawn(node));
    let foreign = World::new();
    let allocations = tracked_allocations(|| {
        assert!(matches!(
            pool.snapshot(&foreign, &Announced(0), ListenerTarget::App),
            Err(ListenerError::ForeignOwner)
        ));
        assert!(matches!(
            pool.snapshot(&world, &Announced(0), ListenerTarget::App),
            Err(ListenerError::Busy)
        ));
    });
    assert_eq!(allocations, 0);
    assert_eq!(drops.get(), 0);
    assert_eq!(pool.entries.borrow().listeners[0].id, id);
    drop(held);
    let allocations = tracked_allocations(|| {
        pool.snapshot(&world, &Announced(0), ListenerTarget::App)
            .unwrap()
            .run(&mut world)
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(drops.get(), 1);
}

#[test]
fn capacity_owner_and_reentry_fail_before_dispatch_or_registration() {
    let mut world = World::new();
    let mut other = World::new();
    let pool = EventListeners::new(&mut world);
    assert_eq!(
        pool.install_app(
            &mut world,
            selector(),
            app_callback(|_: &Announced| {}),
            CommandCapacity::default()
        ),
        Err(ListenerError::Capacity { available: 0 })
    );
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    assert_eq!(
        pool.install_app(
            &mut other,
            selector(),
            app_callback(|_: &Announced| {}),
            CommandCapacity::default()
        ),
        Err(ListenerError::ForeignOwner)
    );
    let id = pool
        .install_app(
            &mut world,
            selector(),
            app_callback(|_: &Announced| {}),
            CommandCapacity::default(),
        )
        .unwrap();
    let snapshot = pool
        .snapshot(&world, &Announced(0), ListenerTarget::App)
        .unwrap();
    assert_eq!(
        pool.reserve(ListenerCapacity {
            listeners: 2,
            snapshot_steps: 2
        }),
        Err(ListenerError::Busy)
    );
    assert!(matches!(
        pool.snapshot(&world, &Announced(0), ListenerTarget::App),
        Err(ListenerError::Busy)
    ));
    assert_eq!(snapshot.run(&mut other), Err(ListenerError::ForeignOwner));
    pool.reserve(ListenerCapacity {
        listeners: 2,
        snapshot_steps: 2,
    })
    .unwrap();
    let foreign = EventListeners::new(&mut other);
    assert_eq!(foreign.remove(id), Err(ListenerError::ForeignOwner));
    assert_eq!(
        pool.reserve(ListenerCapacity {
            listeners: usize::MAX,
            snapshot_steps: usize::MAX
        }),
        Err(ListenerError::AllocationFailed)
    );
    assert_eq!(pool.entries.borrow().limit, 2);
    pool.snapshot(&world, &Announced(0), ListenerTarget::App)
        .unwrap()
        .run(&mut world)
        .unwrap();
    pool.entries.borrow_mut().next = u64::MAX;
    assert_eq!(
        pool.install_app(
            &mut world,
            selector(),
            app_callback(|_: &Announced| {}),
            CommandCapacity::default()
        ),
        Err(ListenerError::IdentityExhausted)
    );
}

#[test]
fn widget_installation_checks_source_and_execution_checks_generation() {
    let mut world = World::new();
    let node = world.spawn_empty();
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    let seen = Rc::new(Cell::new(0));
    let callback = || {
        let body = seen.clone();
        // SAFETY: this body fetches no World parameters and captures no Models.
        unsafe {
            event_callback::<Changed, EntityContext<Widget>, (), ()>(
                "changed",
                &[],
                move |(), _, _, _scope| body.set(body.get() + 1),
            )
        }
    };
    assert!(
        matches!(pool.install_entity(&mut world, node, selector(), callback(), CommandCapacity::default()), Err(ListenerError::MissingSource { entity }) if entity == node)
    );
    world.insert(node, Widget);
    pool.install_entity(
        &mut world,
        node,
        selector(),
        callback(),
        CommandCapacity::default(),
    )
    .unwrap();
    let snapshot = pool
        .snapshot(&world, &Changed(1), ListenerTarget::Entity(node))
        .unwrap();
    world.remove::<Widget>(node);
    snapshot.run(&mut world).unwrap();
    assert_eq!(
        seen.get(),
        1,
        "source removal does not alter a frozen match"
    );
    assert!(world.despawn(node));
    let reused = world.spawn_empty();
    world.insert(reused, Widget);
    assert_eq!(node.id, reused.id);
    assert_ne!(node.generation, reused.generation);
    pool.snapshot(&world, &Changed(1), ListenerTarget::Entity(node))
        .unwrap()
        .run(&mut world)
        .unwrap();
    pool.snapshot(&world, &Changed(1), ListenerTarget::Entity(reused))
        .unwrap()
        .run(&mut world)
        .unwrap();
    assert_eq!(seen.get(), 1);
}

#[test]
fn attached_sources_share_typed_execution_and_instance_identity() {
    let mut app = crate::app::App::headless(8, 8);
    let counter = app.add_model(Counter { value: 0 }).finish();
    let first = app.world.spawn_empty();
    let second = app.world.spawn_empty();
    for node in [first, second] {
        app.world.insert(node, RouteMarker);
        app.world.insert(node, Widget);
        app.world.insert(node, Position(0));
    }
    let pool = EventListeners::new(&mut app.world);
    pool.reserve(ListenerCapacity {
        listeners: 3,
        snapshot_steps: 2,
    })
    .unwrap();
    pool.install_entity(
        &mut app.world,
        first,
        selector(),
        record_change::callback::<EntityContext<Widget>>(counter.clone()),
        CommandCapacity::default(),
    )
    .unwrap();
    pool.install_entity(
        &mut app.world,
        first,
        selector(),
        record_change::callback::<NodeContext>(counter.clone()),
        CommandCapacity::default(),
    )
    .unwrap();
    pool.install_entity(
        &mut app.world,
        second,
        selector(),
        record_change::callback::<NodeContext>(counter.clone()),
        CommandCapacity::default(),
    )
    .unwrap();
    let queue = NotificationQueue::default();
    queue
        .reserve(NotificationCapacity {
            records: 2,
            payload_bytes: 2 * size_of::<Changed>(),
            payload_alignment: core::mem::align_of::<Changed>(),
        })
        .unwrap();
    for value in [3, 5] {
        assert_eq!(
            tracked_allocations(|| {
                queue.push_entity(first, Changed(value)).ok().unwrap();
                queue.push_entity(second, Changed(value + 1)).ok().unwrap();
                assert_eq!(pool.flush_notifications(&mut app.world, &queue, 2), Ok(2));
            }),
            0
        );
    }
    assert_eq!(counter.value(), 26);
    assert_eq!(app.world.get::<Position>(first).unwrap().0, 16);
    assert_eq!(app.world.get::<Position>(second).unwrap().0, 10);
}

#[test]
fn attached_source_validation_rejects_missing_foreign_and_stale_nodes_without_using_slots() {
    let mut world = World::new();
    let node = world.spawn(RouteMarker);
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    let callback = || {
        // SAFETY: the body has no World access or captured Models.
        unsafe {
            event_callback::<Changed, NodeContext, (), ()>("attached", &[], |(), _, _, _scope| ())
        }
    };
    let install = |world: &mut World, node| {
        pool.install_entity(
            world,
            node,
            selector(),
            callback(),
            CommandCapacity::default(),
        )
    };
    assert_eq!(
        install(&mut world, node),
        Err(ListenerError::MissingSource { entity: node })
    );
    let mut foreign = World::new();
    let foreign_node = foreign.spawn(Widget);
    assert_eq!(
        install(&mut foreign, foreign_node),
        Err(ListenerError::ForeignOwner)
    );
    assert!(world.despawn(node));
    let replacement = world.spawn(Widget);
    assert_eq!(replacement.id, node.id);
    assert_ne!(replacement.generation, node.generation);
    assert_eq!(
        install(&mut world, node),
        Err(ListenerError::MissingSource { entity: node })
    );
    let installed = install(&mut world, replacement).unwrap();
    assert_eq!(
        install(&mut world, replacement),
        Err(ListenerError::Capacity { available: 1 })
    );
    assert!(pool.remove(installed).unwrap());
    assert!(install(&mut world, replacement).is_ok());
}

#[test]
fn attached_snapshot_preserves_frozen_matches_and_skips_retired_generations() {
    let mut world = World::new();
    let node = world.spawn(Widget);
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    let seen = Rc::new(Cell::new(0));
    let body = seen.clone();
    // SAFETY: the body has no World access or captured Models.
    let callback = unsafe {
        event_callback::<Changed, NodeContext, (), ()>("attached", &[], move |(), _, _, _scope| {
            body.set(body.get() + 1);
        })
    };
    pool.install_entity(
        &mut world,
        node,
        selector(),
        callback,
        CommandCapacity::default(),
    )
    .unwrap();
    let event = Changed(1);
    let snapshot = pool
        .snapshot(&world, &event, ListenerTarget::Entity(node))
        .unwrap();
    world.remove::<Widget>(node);
    assert_eq!(
        tracked_allocations(|| {
            snapshot.run(&mut world).unwrap();
        }),
        0
    );
    assert_eq!(seen.get(), 1);
    let snapshot = pool
        .snapshot(&world, &event, ListenerTarget::Entity(node))
        .unwrap();
    assert!(world.despawn(node));
    let replacement = world.spawn(Widget);
    assert_eq!(replacement.id, node.id);
    assert_eq!(
        tracked_allocations(|| {
            snapshot.run(&mut world).unwrap();
        }),
        0
    );
    assert_eq!(seen.get(), 1);
}

#[test]
fn listener_command_error_cleans_notifications_and_keeps_both_storages_reusable() {
    let mut world = World::new();
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    let called = Rc::new(Cell::new(0));
    let seen = called.clone();
    // SAFETY: this body only uses its deferred command facade; it has no
    // World parameters or captured Models.
    let callback = unsafe {
        deferred_event_callback::<Owned, AppContext, (), ()>(
            "fail",
            &[],
            move |(), _, (), _scope, mut commands| {
                seen.set(seen.get() + 1);
                commands.insert_resource(7_u32);
            },
        )
    };
    let id = pool
        .install_app(&mut world, selector(), callback, CommandCapacity::default())
        .unwrap();
    let queue = NotificationQueue::default();
    queue
        .reserve(NotificationCapacity {
            records: 2,
            payload_bytes: 2 * size_of::<Owned>(),
            payload_alignment: core::mem::align_of::<Owned>(),
        })
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    for _ in 0..2 {
        queue.push_app(Owned(drops.clone())).ok().unwrap();
    }
    let allocations = tracked_allocations(|| {
        assert_eq!(
            pool.flush_notifications(&mut world, &queue, 2),
            Err(NotificationFlushError::Listener(ListenerError::Command(
                CommandError::RecordCapacity { available: 0 }
            )))
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(drops.get(), 2);
    assert_eq!(called.get(), 1);
    assert!(world.resource::<u32>().is_none());
    pool.remove(id).unwrap();
    pool.install_app(
        &mut world,
        selector(),
        app_callback(|_: &Owned| {}),
        CommandCapacity::default(),
    )
    .unwrap();
    queue.push_app(Owned(drops.clone())).ok().unwrap();
    assert_eq!(pool.flush_notifications(&mut world, &queue, 1), Ok(1));
    assert_eq!(drops.get(), 3);
}

#[test]
fn snapshot_unwind_releases_borrows_and_notification_payloads() {
    let mut world = World::new();
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 1,
        snapshot_steps: 1,
    })
    .unwrap();
    let id = pool
        .install_app(
            &mut world,
            selector(),
            app_callback(|_: &Owned| panic!("listener")),
            CommandCapacity::default(),
        )
        .unwrap();
    let queue = NotificationQueue::default();
    queue
        .reserve(NotificationCapacity {
            records: 2,
            payload_bytes: 2 * size_of::<Owned>(),
            payload_alignment: core::mem::align_of::<Owned>(),
        })
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    for _ in 0..2 {
        queue.push_app(Owned(drops.clone())).ok().unwrap();
    }
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || pool.flush_notifications(&mut world, &queue, 2)
        ))
        .is_err()
    );
    assert_eq!(drops.get(), 2);
    assert!(pool.scratch.borrow().steps.is_empty());
    pool.remove(id).unwrap();
    pool.install_app(
        &mut world,
        selector(),
        app_callback(|_: &Owned| {}),
        CommandCapacity::default(),
    )
    .unwrap();
    queue.push_app(Owned(drops.clone())).ok().unwrap();
    assert_eq!(pool.flush_notifications(&mut world, &queue, 1), Ok(1));
}

#[test]
fn consumable_input_uses_the_same_pool_and_stops_in_installation_order() {
    let mut world = World::new();
    let pool = EventListeners::new(&mut world);
    pool.reserve(ListenerCapacity {
        listeners: 2,
        snapshot_steps: 2,
    })
    .unwrap();
    let seen = Rc::new(Cell::new(0));
    for index in [1, 2] {
        let seen = seen.clone();
        // SAFETY: this callback fetches no parameters and captures no Models.
        let callback = unsafe {
            event_callback::<Key, AppContext, (), bool>("key", &[], move |(), _, (), _scope| {
                seen.set(index);
                true
            })
        };
        pool.install_app(
            &mut world,
            ListenerSelector::new(Key::EVENT, |(), event| event.pressed),
            callback,
            CommandCapacity::default(),
        )
        .unwrap();
    }
    let pressed = Key {
        code: 1,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    let released = Key {
        code: 1,
        pressed: false,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    let allocations = tracked_allocations(|| {
        assert_eq!(
            pool.snapshot(&world, &released, ListenerTarget::App)
                .unwrap()
                .run(&mut world),
            Ok(BubbleControl::Allow)
        );
        assert_eq!(seen.get(), 0);
        assert_eq!(
            pool.snapshot(&world, &pressed, ListenerTarget::App)
                .unwrap()
                .run(&mut world),
            Ok(BubbleControl::Prevent)
        );
        assert_eq!(seen.get(), 1);
    });
    assert_eq!(allocations, 0);
    assert_eq!(
        pool.flush_notifications(&mut world, &NotificationQueue::default(), 0),
        Ok(0)
    );
}
