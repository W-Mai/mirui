use super::*;
use crate::ecs::bundle::Component;
use crate::test_support::tracked_allocations;

#[derive(Debug, PartialEq, Eq)]
struct Position(i32);

impl Component for Position {}

#[derive(Debug, PartialEq, Eq)]
struct Velocity(i32);

impl Component for Velocity {}

#[derive(Debug, PartialEq, Eq)]
struct Label(&'static str);

impl Component for Label {}

struct Active;

impl Component for Active {}

struct Hidden;

impl Component for Hidden {}

struct Marker;

impl Component for Marker {}

std::thread_local! {
    static DRIVER_CALLS: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

struct CountedDriver;

impl sealed::QueryDriver for CountedDriver {}

unsafe impl QueryDriver for CountedDriver {
    type State<'world> = <ComponentDriver<Position> as QueryDriver>::State<'world>;

    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world> {
        DRIVER_CALLS.with(|calls| calls.set((calls.get().0 + 1, calls.get().1)));
        // SAFETY: the wrapper preserves the delegated driver's contract.
        unsafe { ComponentDriver::<Position>::init(cell) }
    }

    fn len(state: &Self::State<'_>) -> usize {
        DRIVER_CALLS.with(|calls| calls.set((calls.get().0, calls.get().1 + 1)));
        ComponentDriver::<Position>::len(state)
    }

    fn contains(state: &Self::State<'_>, entity: Entity) -> bool {
        ComponentDriver::<Position>::contains(state, entity)
    }

    unsafe fn entity(state: &Self::State<'_>, index: usize) -> Entity {
        // SAFETY: the wrapper preserves the delegated driver's contract.
        unsafe { ComponentDriver::<Position>::entity(state, index) }
    }
}

#[test]
fn query_preflight_budgets_writes_once_and_skips_readonly_storage() {
    use crate::ecs::system::param::{QueryParam, fetch_system_params};
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, Position(1));
    world.watch_component_type(TypeId::of::<Position>());
    world.drain_component_changes(|_, _| {});

    type ReadParam = QueryParam<CountedDriver, Read<Position>, ()>;
    type WriteParam = QueryParam<CountedDriver, Write<Position>, ()>;
    DRIVER_CALLS.with(|calls| calls.set((0, 0)));
    // SAFETY: the sole read parameter has no conflicting accesses.
    let (rows,) = unsafe { fetch_system_params::<(ReadParam,)>(&mut world, "read") };
    assert_eq!(DRIVER_CALLS.with(Cell::get), (1, 0));
    assert_eq!(rows.get(entity), Some(&Position(1)));

    DRIVER_CALLS.with(|calls| calls.set((0, 0)));
    let rows = SystemQuery::<'_, CountedDriver, Read<Position>, ()>::new(&mut world);
    assert_eq!(DRIVER_CALLS.with(Cell::get), (1, 0));
    assert_eq!(rows.get(entity), Some(&Position(1)));

    DRIVER_CALLS.with(|calls| calls.set((0, 0)));
    // SAFETY: the sole write parameter has exclusive access to Position.
    let (mut rows,) = unsafe { fetch_system_params::<(WriteParam,)>(&mut world, "write") };
    assert_eq!(DRIVER_CALLS.with(Cell::get), (2, 1));
    assert_eq!(tracked_allocations(|| rows.get(entity).unwrap().0 += 1), 0);

    DRIVER_CALLS.with(|calls| calls.set((0, 0)));
    let mut rows = SystemQuery::<'_, CountedDriver, Write<Position>, ()>::new(&mut world);
    assert_eq!(DRIVER_CALLS.with(Cell::get), (1, 1));
    assert_eq!(tracked_allocations(|| rows.get(entity).unwrap().0 += 1), 0);
    assert_eq!(world.get::<Position>(entity), Some(&Position(3)));
    let mut changed = 0;
    world.drain_component_changes(|_, change| {
        assert_eq!(change.entity, entity);
        assert_eq!(change.type_id, TypeId::of::<Position>());
        changed += 1;
    });
    assert_eq!(changed, 1);
}

#[test]
fn paint_query_reads_track_missing_targets_and_empty_iteration_without_allocations() {
    let mut world = World::new();
    let current = world.spawn_empty();
    let other = world.spawn_empty();
    world.insert(current, Position(1));
    type Rows<'a> = SystemQuery<'a, ComponentDriver<Position>, Read<Position>, ()>;
    let allocations = tracked_allocations(|| {
        let scope = QueryReadScope::new(current);
        let rows = Rows::from_shared(&world).track_reads(&scope);
        assert_eq!(rows.get(current), Some(&Position(1)));
        assert!(!scope.is_global());
        assert!(rows.get(other).is_none());
        assert!(scope.is_global());

        type Empty<'a> = SystemQuery<'a, ComponentDriver<Marker>, Read<Marker>, ()>;
        let scope = QueryReadScope::new(current);
        let rows = Empty::from_shared(&world).track_reads(&scope);
        assert_eq!(rows.into_iter().count(), 0);
        assert!(scope.is_global());
    });
    assert_eq!(allocations, 0);
}

#[test]
fn shared_query_traits_keep_the_readonly_data_contract() {
    fn assert_shared<D: SharedQueryDriver, Q: SharedQueryData, F: SharedQueryFilter>() {
        fn assert_readonly<Q: QueryData<Mutability = ReadOnly>>() {}
        assert_readonly::<Q>();
    }
    assert_shared::<
        ComponentDriver<Position>,
        (EntityItem, Read<Position>, OptionalRead<Velocity>),
        (With<Active>, Without<Hidden>),
    >();
}

#[test]
fn query_only_dependencies_do_not_reserve_entity_change_records() {
    let mut world = World::new();
    for value in 0..32 {
        let entity = world.spawn_empty();
        world.insert(entity, Position(value));
        world.insert(entity, Velocity(value));
    }
    world.watch_query_dependency_type(TypeId::of::<Velocity>());
    type Rows<'w> = SystemQuery<'w, ComponentDriver<Position>, Write<Velocity>, ()>;
    let mut rows_written = 0;
    let allocations = tracked_allocations(|| {
        for velocity in Rows::new(&mut world) {
            velocity.0 += 1;
            rows_written += 1;
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(rows_written, 32);
    world.drain_component_changes(|_, _| panic!("query-only tracking has no entity records"));
    let mut notifications = 0;
    world.drain_query_dependency_changes(|_, kind, _| {
        assert_eq!(kind, TypeId::of::<Velocity>());
        notifications += 1;
    });
    assert_eq!(notifications, 1);
}

#[test]
fn query_dependencies_preserve_yielded_entity_records_and_prepared_budget() {
    let mut world = World::new();
    let first = world.spawn_empty();
    let second = world.spawn_empty();
    let inactive = world.spawn_empty();
    for entity in [first, second, inactive] {
        world.insert(entity, Position(1));
        world.insert(entity, Velocity(2));
    }
    world.insert(first, Active);
    world.insert(second, Active);
    world.watch_query_dependency_type(TypeId::of::<Velocity>());
    world.watch_component_type(TypeId::of::<Velocity>());
    world.drain_component_changes(|_, _| {});
    world.drain_query_dependency_changes(|_, _, _| {});
    type Rows<'w> =
        SystemQuery<'w, ComponentDriver<Position>, (EntityItem, Write<Velocity>), (With<Active>,)>;
    let _ = Rows::new(&mut world);

    let allocations = tracked_allocations(|| {
        let mut rows = Rows::new(&mut world);
        assert!(rows.get(inactive).is_none());
        for _ in 0..64 {
            let (_, velocity) = rows.get(first).expect("active row");
            velocity.0 += 1;
        }
        for (_, velocity) in rows {
            velocity.0 += 1;
        }
    });
    assert_eq!(allocations, 0);
    let mut changes = alloc::vec::Vec::new();
    world.drain_component_changes(|_, change| changes.push(change));
    assert_eq!(
        changes,
        [
            crate::ecs::world::ComponentChange {
                entity: first,
                type_id: TypeId::of::<Velocity>(),
                kind: crate::ecs::world::ComponentChangeKind::Component,
            },
            crate::ecs::world::ComponentChange {
                entity: second,
                type_id: TypeId::of::<Velocity>(),
                kind: crate::ecs::world::ComponentChangeKind::Component,
            },
        ]
    );
    let mut notifications = 0;
    world.drain_query_dependency_changes(|_, kind, _| {
        assert_eq!(kind, TypeId::of::<Velocity>());
        notifications += 1;
    });
    assert_eq!(notifications, 1);
    assert_eq!(world.get::<Velocity>(first), Some(&Velocity(67)));
    assert_eq!(world.get::<Velocity>(second), Some(&Velocity(3)));
    assert_eq!(world.get::<Velocity>(inactive), Some(&Velocity(2)));
}

#[test]
fn shared_queries_coexist_and_preserve_pointer_optional_filter_and_generation_contracts() {
    let mut world = World::new();
    let expired = world.spawn_empty();
    world.insert(expired, Position(-1));
    assert!(world.despawn(expired));
    let first = world.spawn_empty();
    assert_eq!(first.id, expired.id);
    assert_ne!(first.generation, expired.generation);
    world.insert(first, Position(1));
    world.insert(first, Velocity(10));
    world.insert(first, Active);
    let second = world.spawn_empty();
    world.insert(second, Position(2));
    world.insert(second, Label("two"));
    world.insert(second, Active);
    let hidden = world.spawn_empty();
    world.insert(hidden, Position(3));
    world.insert(hidden, Active);
    world.insert(hidden, Hidden);
    let inactive = world.spawn_empty();
    world.insert(inactive, Position(4));
    let outside_driver = world.spawn_empty();
    world.insert(outside_driver, Velocity(50));
    world.insert(outside_driver, Active);
    world.watch_component_type(TypeId::of::<Position>());
    world.drain_component_changes(|_, _| {});

    type Rows<'world> = SystemQuery<
        'world,
        ComponentDriver<Position>,
        (
            EntityItem,
            Read<Position>,
            Read<Position>,
            OptionalRead<Velocity>,
            OptionalRead<Label>,
            OptionalRead<Marker>,
        ),
        (With<Active>, Without<Hidden>),
    >;
    type Other<'world> =
        SystemQuery<'world, ComponentDriver<Active>, Read<Position>, (Without<Hidden>,)>;
    type Only<'world> = SystemQuery<'world, ComponentDriver<Active>, EntityItem, ()>;
    type Missing<'world> = SystemQuery<'world, ComponentDriver<Marker>, OptionalRead<Position>, ()>;
    {
        let rows = Rows::from_shared(&world);
        let other = Other::from_shared(&world);
        let only = Only::from_shared(&world);
        let missing = Missing::from_shared(&world);
        {
            let (entity, left, right, velocity, label, marker) = rows.get(first).unwrap();
            let concurrent = other.get(first).unwrap();
            let repeated = rows.get(first).unwrap().1;
            assert_eq!(entity, first);
            assert!(core::ptr::eq(left, right));
            assert!(core::ptr::eq(left, concurrent));
            assert!(core::ptr::eq(left, repeated));
            assert!(core::ptr::eq(left, world.get::<Position>(first).unwrap()));
            assert!(core::ptr::eq(
                velocity.unwrap(),
                world.get::<Velocity>(first).unwrap()
            ));
            assert!(label.is_none());
            assert!(marker.is_none());
        }
        let (_, _, _, velocity, label, marker) = rows.get(second).unwrap();
        assert!(velocity.is_none());
        assert!(core::ptr::eq(
            label.unwrap(),
            world.get::<Label>(second).unwrap()
        ));
        assert!(marker.is_none());
        assert!(rows.get(hidden).is_none());
        assert!(rows.get(inactive).is_none());
        assert!(rows.get(outside_driver).is_none());
        assert!(rows.get(expired).is_none());
        assert!(other.get(outside_driver).is_none());
        assert!(other.get(expired).is_none());
        assert_eq!(only.get(outside_driver), Some(outside_driver));
        assert!(missing.get(first).is_none());
        assert_eq!(missing.into_iter().count(), 0);
        let mut iter = rows.into_iter();
        assert_eq!(iter.next().map(|row| row.0), Some(first));
        assert_eq!(iter.next().map(|row| row.0), Some(second));
        assert!(iter.next().is_none());
        assert_eq!(other.get(first).unwrap(), &Position(1));
    }
    let mut changes = 0;
    world.drain_component_changes(|_, _| changes += 1);
    assert_eq!(changes, 0);
}

#[test]
fn shared_query_first_and_repeated_construction_get_and_iteration_do_not_allocate() {
    let mut world = World::new();
    let first = world.spawn_empty();
    world.insert(first, Position(1));
    world.insert(first, Velocity(10));
    world.insert(first, Active);
    let second = world.spawn_empty();
    world.insert(second, Position(2));
    world.insert(second, Active);
    type Rows<'world> = SystemQuery<
        'world,
        ComponentDriver<Position>,
        (EntityItem, Read<Position>, OptionalRead<Velocity>),
        (With<Active>, Without<Hidden>),
    >;
    for repetitions in [1, 64] {
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..repetitions {
                    let first_query = Rows::from_shared(&world);
                    let second_query = Rows::from_shared(&world);
                    {
                        let left = first_query.get(first).unwrap();
                        let right = second_query.get(first).unwrap();
                        assert!(core::ptr::eq(left.1, right.1));
                        assert!(core::ptr::eq(left.2.unwrap(), right.2.unwrap()));
                    }
                    assert!(first_query.get(second).unwrap().2.is_none());
                    assert_eq!(first_query.into_iter().count(), 2);
                    assert_eq!(second_query.into_iter().count(), 2);
                }
            }),
            0
        );
    }
}

#[crate::animate(component = GeneratedInventoryMotion)]
fn apply_generated_inventory_motion(_value: motion!()) {}

fn assert_component<T: Component>() {}

#[test]
fn framework_system_query_inventory_is_component_typed() {
    assert_component::<crate::core::timer::Timer>();
    assert_component::<crate::ui::InteractionState>();
    assert_component::<crate::ui::layout_binding::LayoutBinding>();
    assert_component::<crate::ui::layout_binding::LayoutContainer>();
    assert_component::<crate::ui::offscreen::OffscreenRender>();
    assert_component::<crate::ui::offscreen::OffscreenGeneration>();
    assert_component::<crate::ui::widgets::tabbar::TabIndicatorTween>();
    assert_component::<crate::ui::widgets::tabbar::TabBarPrev>();
    assert_component::<crate::ui::widgets::TabBar>();
    assert_component::<crate::ui::widgets::TabContent>();
    assert_component::<crate::ui::Widget>();
    assert_component::<crate::ui::Children>();
    assert_component::<crate::ui::Parent>();
    assert_component::<crate::ui::ComputedRect>();
    assert_component::<crate::ui::Style>();
    assert_component::<crate::ui::dirty::Dirty>();
    assert_component::<crate::ui::Hidden>();
    assert_component::<crate::ui::IgnoreHitTest>();
    assert_component::<crate::ui::InteractionFeedback>();
    assert_component::<crate::ui::widgets::switch::SwitchBgT>();
    assert_component::<crate::ui::widgets::switch::AnimatedThumbX>();
    assert_component::<crate::ui::widgets::switch::AnimateSwitchBgT>();
    assert_component::<crate::ui::widgets::switch::AnimateThumbX>();
    assert_component::<crate::ui::widgets::Switch>();
    assert_component::<crate::ui::widgets::LazyList>();
    assert_component::<crate::input::event::scroll::ScrollOffset>();
    assert_component::<crate::input::event::scroll::ScrollConfig>();
    assert_component::<crate::ui::widgets::WidgetTransform>();
    assert_component::<crate::input::feedback::OverlayCursor>();
    assert_component::<crate::input::feedback::OverlayRotary>();
    assert_component::<crate::ui::widgets::TextInput>();
    assert_component::<crate::ui::widgets::Text>();
    assert_component::<GeneratedInventoryMotion>();
    let _ = GeneratedInventoryMotion::system();
}

#[test]
fn whole_query_yields_flat_rows_and_mutates_in_place() {
    let mut world = World::new();
    let first = world.spawn_empty();
    world.insert(first, Position(1));
    world.insert(first, Velocity(2));
    let second = world.spawn_empty();
    world.insert(second, Position(10));
    world.insert(second, Velocity(20));

    type Rows<'w> = SystemQuery<
        'w,
        ComponentDriver<Position>,
        (EntityItem, Read<Position>, Write<Velocity>),
        (),
    >;
    let rows = Rows::new(&mut world);
    let mut seen = 0;
    for (entity, position, velocity) in rows {
        assert!(entity == first || entity == second);
        velocity.0 += position.0;
        seen += 1;
    }

    assert_eq!(seen, 2);
    assert_eq!(world.get::<Velocity>(first), Some(&Velocity(3)));
    assert_eq!(world.get::<Velocity>(second), Some(&Velocity(30)));
}

#[test]
fn get_uses_driver_required_components_filters_and_generation() {
    let mut world = World::new();
    let old = world.spawn_empty();
    world.insert(old, Position(0));
    assert!(world.despawn(old));
    let selected = world.spawn_empty();
    world.insert(selected, Position(1));
    world.insert(selected, Velocity(2));
    world.insert(selected, Active);
    let missing_driver = world.spawn_empty();
    world.insert(missing_driver, Velocity(3));
    world.insert(missing_driver, Active);
    let missing_required = world.spawn_empty();
    world.insert(missing_required, Position(4));
    world.insert(missing_required, Active);
    let inactive = world.spawn_empty();
    world.insert(inactive, Position(5));
    world.insert(inactive, Velocity(6));
    let hidden = world.spawn_empty();
    world.insert(hidden, Position(7));
    world.insert(hidden, Velocity(8));
    world.insert(hidden, Active);
    world.insert(hidden, Hidden);

    type Rows<'w> = SystemQuery<
        'w,
        ComponentDriver<Position>,
        (EntityItem, Read<Velocity>, OptionalRead<Label>),
        (With<Active>, Without<Hidden>),
    >;
    let rows = Rows::new(&mut world);
    let (entity, velocity, label) = rows.get(selected).expect("matching row");
    let (_, same_velocity, _) = rows.get(selected).expect("shared read");
    assert_eq!(entity, selected);
    assert_eq!(velocity.0, 2);
    assert!(core::ptr::eq(velocity, same_velocity));
    assert!(label.is_none());
    for excluded in [old, missing_driver, missing_required, inactive, hidden] {
        assert!(rows.get(excluded).is_none());
    }
}

#[test]
fn mutable_get_reborrows_and_coalesces_changes_without_allocating() {
    let mut world = World::new();
    let first = world.spawn_empty();
    let second = world.spawn_empty();
    for entity in [first, second] {
        world.insert(entity, Position(1));
        world.insert(entity, Velocity(2));
    }
    world.watch_component_type(TypeId::of::<Velocity>());
    world.drain_component_changes(|_, _| {});

    type Rows<'w> =
        SystemQuery<'w, ComponentDriver<Position>, (Write<Velocity>, Read<Position>), ()>;
    let _ = Rows::new(&mut world);
    let allocations = tracked_allocations(|| {
        let mut rows = Rows::new(&mut world);
        for _ in 0..64 {
            let (velocity, position) = rows.get(first).expect("first row");
            velocity.0 += position.0;
        }
        for (velocity, position) in rows {
            velocity.0 += position.0;
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(world.get::<Velocity>(first), Some(&Velocity(67)));
    assert_eq!(world.get::<Velocity>(second), Some(&Velocity(3)));
    let mut changes = alloc::vec::Vec::new();
    world.drain_component_changes(|_, change| changes.push(change.entity));
    assert_eq!(changes, [first, second]);
}

#[test]
fn mutable_rows_record_only_entities_that_are_yielded() {
    let mut world = World::new();
    let active = world.spawn_empty();
    world.insert(active, Position(1));
    world.insert(active, Velocity(2));
    world.insert(active, Active);
    let inactive = world.spawn_empty();
    world.insert(inactive, Position(3));
    world.insert(inactive, Velocity(4));
    world.watch_component_type(TypeId::of::<Velocity>());
    world.drain_component_changes(|_, _| {});

    type Rows<'w> =
        SystemQuery<'w, ComponentDriver<Position>, (EntityItem, Write<Velocity>), (With<Active>,)>;
    for (_, velocity) in Rows::new(&mut world) {
        velocity.0 += 1;
    }

    let mut changes = alloc::vec::Vec::new();
    world.drain_component_changes(|_, change| changes.push(change));
    assert_eq!(
        changes,
        [crate::ecs::world::ComponentChange {
            entity: active,
            type_id: TypeId::of::<Velocity>(),
            kind: crate::ecs::world::ComponentChangeKind::Component,
        }]
    );
    assert_eq!(world.get::<Velocity>(active), Some(&Velocity(3)));
    assert_eq!(world.get::<Velocity>(inactive), Some(&Velocity(4)));
}

#[test]
fn mutable_query_records_only_rows_requested_from_the_iterator() {
    let mut world = World::new();
    let first = world.spawn_empty();
    world.insert(first, Velocity(1));
    let second = world.spawn_empty();
    world.insert(second, Velocity(2));
    world.watch_component_type(TypeId::of::<Velocity>());
    world.drain_component_changes(|_, _| {});

    type Rows<'world> =
        SystemQuery<'world, ComponentDriver<Velocity>, (EntityItem, Write<Velocity>), ()>;

    let _ = Rows::new(&mut world);
    let mut changes = alloc::vec::Vec::new();
    world.drain_component_changes(|_, change| changes.push(change));
    assert!(changes.is_empty());

    let mut rows = Rows::new(&mut world).into_iter();
    let (yielded, _) = rows.next().expect("first mutable row");
    world.drain_component_changes(|_, change| changes.push(change));

    assert_eq!(yielded, first);
    assert_eq!(
        changes,
        [crate::ecs::world::ComponentChange {
            entity: first,
            type_id: TypeId::of::<Velocity>(),
            kind: crate::ecs::world::ComponentChangeKind::Component,
        }]
    );
    assert_ne!(yielded, second);
}

#[test]
fn mutable_query_reuses_change_queue_after_warmup() {
    let mut world = World::new();
    world.watch_component_type(TypeId::of::<Velocity>());
    for value in 0..32 {
        let entity = world.spawn_empty();
        world.insert(entity, Position(value));
        world.insert(entity, Velocity(value));
        world.drain_component_changes(|_, _| {});
    }
    type Rows<'w> = SystemQuery<'w, ComponentDriver<Position>, Write<Velocity>, ()>;

    for velocity in Rows::new(&mut world) {
        velocity.0 += 1;
    }
    world.drain_component_changes(|_, _| {});

    let allocations = tracked_allocations(|| {
        for velocity in Rows::new(&mut world) {
            velocity.0 += 1;
        }
        world.drain_component_changes(|_, _| {});
    });

    assert_eq!(allocations, 0);
}

#[test]
fn with_and_without_filters_select_rows() {
    let mut world = World::new();
    let visible = world.spawn_empty();
    world.insert(visible, Position(1));
    world.insert(visible, Active);
    let hidden = world.spawn_empty();
    world.insert(hidden, Position(2));
    world.insert(hidden, Active);
    world.insert(hidden, Hidden);
    let inactive = world.spawn_empty();
    world.insert(inactive, Position(3));

    type Rows<'w> = SystemQuery<
        'w,
        ComponentDriver<Position>,
        (EntityItem, Read<Position>),
        (With<Active>, Without<Hidden>),
    >;
    let mut rows = Rows::new(&mut world).into_iter();
    let (entity, position) = rows.next().expect("visible row");
    assert_eq!((entity, position.0), (visible, 1));
    assert!(rows.next().is_none());
}

#[test]
fn optional_read_returns_some_and_none() {
    let mut world = World::new();
    let labeled = world.spawn_empty();
    world.insert(labeled, Position(1));
    world.insert(labeled, Label("one"));
    let unlabeled = world.spawn_empty();
    world.insert(unlabeled, Position(2));

    type Rows<'w> =
        SystemQuery<'w, ComponentDriver<Position>, (EntityItem, OptionalRead<Label>), ()>;
    let mut labeled_value = None;
    let mut saw_unlabeled = false;
    for (entity, label) in Rows::new(&mut world) {
        if entity == labeled {
            labeled_value = label.map(|label| label.0);
        } else if entity == unlabeled {
            saw_unlabeled = label.is_none();
        }
    }

    assert_eq!(labeled_value, Some("one"));
    assert!(saw_unlabeled);
}

#[test]
fn missing_required_component_skips_row() {
    let mut world = World::new();
    let complete = world.spawn_empty();
    world.insert(complete, Position(1));
    world.insert(complete, Velocity(2));
    let incomplete = world.spawn_empty();
    world.insert(incomplete, Position(3));

    type Rows<'w> = SystemQuery<'w, ComponentDriver<Position>, (EntityItem, Read<Velocity>), ()>;
    let mut rows = Rows::new(&mut world).into_iter();
    assert_eq!(rows.next().map(|(entity, _)| entity), Some(complete));
    assert!(rows.next().is_none());
}

#[test]
fn missing_driver_yields_empty_query() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, Velocity(2));

    type Rows<'w> =
        SystemQuery<'w, ComponentDriver<Position>, (EntityItem, OptionalRead<Velocity>), ()>;
    assert_eq!(Rows::new(&mut world).into_iter().count(), 0);
}

#[test]
fn with_filter_can_drive_entity_only_query() {
    let mut world = World::new();
    let active = world.spawn_empty();
    world.insert(active, Active);
    let inactive = world.spawn_empty();
    world.insert(inactive, Position(2));

    type Rows<'w> = SystemQuery<'w, ComponentDriver<Active>, EntityItem, (With<Active>,)>;
    let mut rows = Rows::new(&mut world).into_iter();
    assert_eq!(rows.next(), Some(active));
    assert!(rows.next().is_none());
}

#[test]
fn entity_generation_is_preserved() {
    let mut world = World::new();
    let old = world.spawn_empty();
    world.insert(old, Position(1));
    assert!(world.despawn(old));
    let current = world.spawn_empty();
    world.insert(current, Position(2));
    assert_eq!(old.id, current.id);
    assert_ne!(old.generation, current.generation);

    type Rows<'w> = SystemQuery<'w, ComponentDriver<Position>, (EntityItem, Read<Position>), ()>;
    let mut rows = Rows::new(&mut world).into_iter();
    assert_eq!(rows.next().map(|(entity, _)| entity), Some(current));
    assert!(rows.next().is_none());
}

#[test]
fn duplicate_read_is_allowed() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, Position(4));

    type Rows<'w> =
        SystemQuery<'w, ComponentDriver<Position>, (Read<Position>, Read<Position>), ()>;
    let mut rows = Rows::new(&mut world).into_iter();
    let (left, right) = rows.next().expect("row");
    assert_eq!(left, &Position(4));
    assert!(core::ptr::eq(left, right));
}

#[test]
#[should_panic(expected = "conflicting query access")]
fn read_write_alias_is_rejected_before_fetch() {
    let mut world = World::new();
    type Rows<'w> =
        SystemQuery<'w, ComponentDriver<Position>, (Read<Position>, Write<Position>), ()>;
    let _ = Rows::new(&mut world);
}

#[test]
#[should_panic(expected = "conflicting query access")]
fn write_write_alias_is_rejected_before_fetch() {
    let mut world = World::new();
    type Rows<'w> =
        SystemQuery<'w, ComponentDriver<Position>, (Write<Position>, Write<Position>), ()>;
    let _ = Rows::new(&mut world);
}

#[test]
#[should_panic(expected = "conflicting query access")]
fn type_alias_conflict_is_rejected() {
    type PositionAlias = Position;
    let mut world = World::new();
    type Rows<'w> =
        SystemQuery<'w, ComponentDriver<Position>, (Read<Position>, Write<PositionAlias>), ()>;
    let _ = Rows::new(&mut world);
}

#[test]
fn writable_zero_sized_components_have_one_row_per_entity() {
    let mut world = World::new();
    let first = world.spawn_empty();
    world.insert(first, Marker);
    let second = world.spawn_empty();
    world.insert(second, Marker);

    type Rows<'w> = SystemQuery<'w, ComponentDriver<Marker>, (EntityItem, Write<Marker>), ()>;
    let mut rows = Rows::new(&mut world).into_iter();
    assert_eq!(rows.next().map(|(entity, _)| entity), Some(first));
    assert_eq!(rows.next().map(|(entity, _)| entity), Some(second));
    assert!(rows.next().is_none());
}

#[test]
fn mutable_driver_rows_keep_distinct_slots_exclusive() {
    let mut world = World::new();
    let first = world.spawn_empty();
    world.insert(first, Velocity(1));
    let second = world.spawn_empty();
    world.insert(second, Velocity(2));

    type Rows<'world> =
        SystemQuery<'world, ComponentDriver<Velocity>, (EntityItem, Write<Velocity>), ()>;
    let mut rows = Rows::new(&mut world).into_iter();
    let (first_entity, first_value) = rows.next().expect("first row");
    let (second_entity, second_value) = rows.next().expect("second row");
    first_value.0 += 10;
    second_value.0 += 20;
    assert!(rows.next().is_none());

    assert_eq!((first_entity, first_value.0), (first, 11));
    assert_eq!((second_entity, second_value.0), (second, 22));
}

#[test]
fn warmed_query_construction_and_iteration_allocate_nothing() {
    let mut world = World::new();
    for value in 1..=8 {
        let entity = world.spawn_empty();
        world.insert(entity, Position(value));
        world.insert(entity, Velocity(value * 10));
        world.insert(entity, Active);
        if value % 2 == 0 {
            world.insert(entity, Label("label"));
        }
        if value == 8 {
            world.insert(entity, Hidden);
        }
    }

    type Rows<'world> = SystemQuery<
        'world,
        ComponentDriver<Position>,
        (
            EntityItem,
            Read<Position>,
            Write<Velocity>,
            OptionalRead<Label>,
        ),
        (With<Active>, Without<Hidden>),
    >;

    let mut visited = 0;
    let allocations = tracked_allocations(|| {
        for (_entity, position, velocity, label) in Rows::new(&mut world) {
            velocity.0 += position.0;
            visited += 1 + usize::from(label.is_some());
        }
    });

    assert_eq!(allocations, 0);
    assert_eq!(visited, 10);
}
