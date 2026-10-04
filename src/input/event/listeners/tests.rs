use super::super::callback::{deferred_event_callback, event_callback};
use super::super::contract::{AppNotification, AppSource, EntityNotification, WidgetSource};
use super::super::input::Key;
use super::super::notification::NotificationCapacity;
use super::*;
use crate::input::event::gesture::DragMove;
use crate::test_support::tracked_allocations;
use crate::types::Fixed;
use core::cell::Cell;

struct RouteMarker;
impl Component for RouteMarker {}

struct RouteTrace {
    trace: Rc<RefCell<Trace>>,
    origin: Entity,
    alternate: Entity,
    old_parent: Entity,
    internal_stop: Option<Entity>,
    author_stop: Option<Entity>,
}

fn route_first(world: &mut World, node: Entity, event: &GestureEvent) -> bool {
    let trace = world.resource::<RouteTrace>().unwrap();
    assert_eq!(event.target(), trace.origin);
    trace.trace.borrow_mut().record(100 + node.id);
    let stop = trace.internal_stop == Some(node);
    if node == trace.origin {
        let (alternate, old_parent) = (trace.alternate, trace.old_parent);
        world.insert(node, Parent(alternate));
        world.remove::<RouteMarker>(old_parent);
    }
    stop
}

fn route_second(world: &mut World, node: Entity, _: &GestureEvent) -> bool {
    world
        .resource::<RouteTrace>()
        .unwrap()
        .trace
        .borrow_mut()
        .record(200 + node.id);
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
                .with_internal_gesture(route_first),
        );
        registry.insert(
            crate::ui::view::View::systems_only("second", &[])
                .with_filter::<RouteMarker>()
                .with_internal_gesture(route_second),
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
            pool.install_entity::<DragMove, RouteMarker>(
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
fn gesture_route_freezes_all_parents_and_internal_matches_before_any_body() {
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
        &[102, 202, 302, 101, 201, 301, 100, 200, 300]
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
    assert_eq!(fixture.trace.borrow().len, 9);
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
    assert_eq!(
        fixture.trace.borrow().values(),
        &[102, 202, 302, 101, 201, 301]
    );
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
        fixture.pool.install_entity::<DragMove, RouteMarker>(
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
    assert_eq!(fixture.trace.borrow().len, 9);
}

struct Widget;

#[test]
fn internal_only_gesture_routes_prepare_and_restore_reactive_owner_without_allocation() {
    fn internal(world: &mut World, _: Entity, _: &GestureEvent) -> bool {
        assert_eq!(crate::core::reactive::current_world_id(), Some(world.id()));
        true
    }
    let mut world = World::new();
    let node = world.spawn_empty();
    let mut registry = ViewRegistry::default();
    registry.insert(
        crate::ui::view::View::systems_only("internal", &[]).with_internal_gesture(internal),
    );
    world.insert_resource(registry);
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
        event_callback::<E, AppContext, (), ()>("announce", &[], move |(), event, ()| body(event))
    }
}

fn selector<E: Event<Options = ()>>() -> ListenerSelector<E> {
    ListenerSelector::new(E::EVENT, |(), _| true)
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
    let counter = app.add_model(Counter { value: 0 });
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
        record_change::callback(counter.clone()),
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
                move |(), event, target| {
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
                move |(), _, _| body.set(body.get() + 1),
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
            move |(), _, (), mut commands| {
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
            event_callback::<Key, AppContext, (), bool>("key", &[], move |(), _, ()| {
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
    };
    let released = Key {
        code: 1,
        pressed: false,
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
