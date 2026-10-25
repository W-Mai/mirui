use super::*;
use crate::ui::view::{View, ViewRegistry};

#[crate::component]
struct Source(u32);
#[crate::component]
struct Capability;
#[derive(Default)]
struct Trace {
    sources: u32,
    capabilities: u32,
}

fn source_attach(scope: &mut crate::ui::UiScope<'_>) {
    let node = scope.parent();
    let world = scope.world_mut();
    world.resource_mut::<Trace>().unwrap().sources += 1;
    if !world.has::<Capability>(node) {
        world.insert(node, Capability);
    }
}

fn capability_attach(scope: &mut crate::ui::UiScope<'_>) {
    scope
        .world_mut()
        .resource_mut::<Trace>()
        .unwrap()
        .capabilities += 1;
}

fn source_view() -> View {
    View::systems_only("source", &[])
        .with_filter::<Source>()
        .with_attach(source_attach)
}

fn capability_view() -> View {
    View::systems_only("capability", &[])
        .with_filter::<Capability>()
        .with_attach(capability_attach)
}

fn world(reverse: bool) -> World {
    let mut world = World::new();
    let mut registry = ViewRegistry::default();
    if reverse {
        registry.insert(capability_view());
        registry.insert(source_view());
    } else {
        registry.insert(source_view());
        registry.insert(capability_view());
    }
    world.insert_resource(registry);
    world.insert_resource(Trace::default());
    world
}

#[test]
fn event_executor_does_not_initialize_pending_widgets() {
    let mut world = world(false);
    let node = world.spawn_empty();
    attach_handlers_for(&mut world, node);
    world.insert(node, Source(1));
    crate::input::event::runtime::initialize(&mut world);

    crate::input::event::runtime::flush(&mut world, |_| {}).unwrap();
    assert!(!world.has::<Capability>(node));
    assert_eq!(world.resource::<Trace>().unwrap().sources, 0);

    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(world.has::<Capability>(node));
    assert_eq!(world.resource::<Trace>().unwrap().sources, 1);
}

struct InitializeStep(u8);

impl crate::input::event::contract::Event for InitializeStep {
    type Route = crate::input::event::contract::EntityNotification;
    type Source = crate::input::event::contract::WidgetSource<crate::ui::Widget>;
    type Options = ();
}

#[crate::event]
fn initialize_step(
    event: event!(InitializeStep),
    node: entity!(),
    capabilities: query!(Capability),
    commands: commands!(),
) {
    match event.0 {
        0 => commands.insert(node, Source(1)),
        _ => assert!(capabilities.get(node).is_some()),
    }
}

#[test]
fn notification_delivery_initializes_changes_before_the_next_notification() {
    use crate::input::event::{contract::Event, emission, runtime};
    let mut world = world(true);
    let node = world.spawn(crate::ui::Widget);
    attach_handlers_for(&mut world, node);
    runtime::install_entity(
        &mut world,
        node,
        InitializeStep::EVENT,
        initialize_step::callback::<crate::input::event::callback::NodeContext>(),
    )
    .unwrap();
    emission::submit(&mut world, node, InitializeStep(0)).unwrap();
    emission::submit(&mut world, node, InitializeStep(1)).unwrap();

    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 2);
    assert_eq!(world.resource::<Trace>().unwrap().sources, 1);
    assert_eq!(world.resource::<Trace>().unwrap().capabilities, 1);
}

#[test]
fn attachment_effects_settle_before_delivery_preparation_returns() {
    use crate::core::reactive::{Effect, Signal, with_world, with_world_scope};
    #[crate::component]
    struct Wake;
    fn wake(scope: &mut crate::ui::UiScope<'_>) {
        scope
            .world_mut()
            .resource::<Signal<bool>>()
            .unwrap()
            .set(true);
    }
    let mut world = world(false);
    world.resource_mut::<ViewRegistry>().unwrap().insert(
        View::systems_only("wake", &[])
            .with_filter::<Wake>()
            .with_attach(wake),
    );
    let node = world.spawn_empty();
    attach_handlers_for(&mut world, node);
    let signal = Signal::new(false);
    world.insert_resource(signal.clone());
    let _effect = with_world_scope(&mut world, || {
        Effect::new(move || {
            if signal.get() {
                with_world(|world| world.insert(node, Capability));
            }
        })
    });
    world.insert(node, Wake);

    prepare(&mut world);

    assert!(world.has::<Capability>(node));
    assert_eq!(world.resource::<Trace>().unwrap().capabilities, 1);
}

#[test]
fn component_chains_initialize_once_independently_of_registration_order() {
    for reverse in [false, true] {
        let mut world = world(reverse);
        let node = world.spawn(Source(1));
        attach_handlers_for(&mut world, node);
        prepare(&mut world);
        let trace = world.resource::<Trace>().unwrap();
        assert_eq!((trace.sources, trace.capabilities), (1, 1));
        let guard = view_input_guard(&world, node, if reverse { 0 } else { 1 }).unwrap();
        assert!(guard.valid(&world, node));
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                world.get_mut::<Source>(node).unwrap().0 += 1;
                attach_handlers_for(&mut world, node);
                prepare(&mut world);
            }),
            0
        );
        let trace = world.resource::<Trace>().unwrap();
        assert_eq!((trace.sources, trace.capabilities), (1, 1));
    }
}

#[test]
fn replacements_invalidate_old_input_even_after_the_new_instance_is_ready() {
    let mut world = world(true);
    let node = world.spawn(Source(1));
    attach_handlers_for(&mut world, node);
    prepare(&mut world);
    let old = view_input_guard(&world, node, 0).unwrap();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            world.insert(node, Capability);
            assert!(!old.valid(&world, node));
            prepare(&mut world);
        }),
        0
    );
    assert!(!old.valid(&world, node));
    assert!(
        view_input_guard(&world, node, 0)
            .unwrap()
            .valid(&world, node)
    );
    world.remove::<Capability>(node);
    world.insert(node, Capability);
    prepare(&mut world);
    assert_eq!(world.resource::<Trace>().unwrap().capabilities, 3);
    world.despawn(node);
    let replacement = world.spawn(Source(2));
    attach_handlers_for(&mut world, replacement);
    assert!(!old.valid(&world, replacement));
    assert_eq!(world.resource::<Trace>().unwrap().capabilities, 4);
}

#[test]
fn late_view_registration_initializes_existing_constructed_nodes() {
    let mut world = world(false);
    world.insert_resource(ViewRegistry::default());
    let node = world.spawn(Source(1));
    attach_handlers_for(&mut world, node);
    world
        .resource_mut::<ViewRegistry>()
        .unwrap()
        .insert(source_view());
    prepare(&mut world);
    assert_eq!(world.resource::<Trace>().unwrap().sources, 1);
    world
        .resource_mut::<ViewRegistry>()
        .unwrap()
        .insert(capability_view());
    prepare(&mut world);
    assert_eq!(world.resource::<Trace>().unwrap().capabilities, 1);
    let old = view_input_guard(&world, node, 1).unwrap();
    let mut registry = ViewRegistry::default();
    registry.insert(source_view());
    registry.insert(capability_view());
    world.insert_resource(registry);
    prepare(&mut world);
    assert!(!old.valid(&world, node));
    assert_eq!(world.resource::<Trace>().unwrap().capabilities, 2);
}

#[test]
fn prepared_registration_keeps_reconciliation_live_without_allocations() {
    struct Reconciliations(usize);
    fn reconcile(world: &mut World, _: Entity) -> bool {
        world.resource_mut::<Reconciliations>().unwrap().0 += 1;
        false
    }
    let mut world = world(false);
    world.insert_resource(Reconciliations(0));
    world.resource_mut::<ViewRegistry>().unwrap().insert(
        View::systems_only("reconciler", &[])
            .with_filter::<Source>()
            .with_reconcile(reconcile),
    );
    let node = world.spawn(Source(1));
    attach_handlers_for(&mut world, node);
    prepare(&mut world);
    let before = world.resource::<Reconciliations>().unwrap().0;
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..8 {
                prepare(&mut world);
            }
        }),
        0
    );
    assert_eq!(world.resource::<Reconciliations>().unwrap().0, before + 8);
}

#[test]
fn pending_attachment_refreshes_the_registry_before_reconciliation() {
    fn replace(scope: &mut crate::ui::UiScope<'_>) {
        let node = scope.parent();
        let world = scope.world_mut();
        if world.get::<Source>(node).unwrap().0 == 2 {
            world.insert_resource(ViewRegistry::default());
        }
    }
    fn reconcile(world: &mut World, node: Entity) -> bool {
        assert_eq!(world.get::<Source>(node).unwrap().0, 1);
        false
    }
    let mut world = World::new();
    let mut registry = ViewRegistry::default();
    registry.insert(
        View::systems_only("replace", &[])
            .with_filter::<Source>()
            .with_attach(replace)
            .with_reconcile(reconcile),
    );
    world.insert_resource(registry);
    let node = world.spawn(Source(1));
    attach_handlers_for(&mut world, node);
    prepare(&mut world);

    world.insert(node, Source(2));
    prepare(&mut world);
    assert!(world.resource::<ViewRegistry>().unwrap().is_empty());
}

#[test]
fn incomplete_nodes_wait_for_the_construction_boundary() {
    let mut world = world(true);
    let first = world.spawn(Source(1));
    attach_handlers_for(&mut world, first);
    let unfinished = world.spawn(Source(2));
    prepare(&mut world);
    assert!(!world.has::<Capability>(unfinished));
    attach_handlers_for(&mut world, unfinished);
    assert!(world.has::<Capability>(unfinished));
    assert_eq!(world.resource::<Trace>().unwrap().sources, 2);
}

#[test]
fn replacement_cycles_stop_without_exposing_uninitialized_input() {
    fn replace(scope: &mut crate::ui::UiScope<'_>) {
        let node = scope.parent();
        let world = scope.world_mut();
        world.resource_mut::<Trace>().unwrap().sources += 1;
        world.insert(node, Source(0));
    }
    let mut world = world(false);
    let mut registry = ViewRegistry::default();
    registry.insert(
        View::systems_only("cycle", &[])
            .with_filter::<Source>()
            .with_attach(replace),
    );
    world.insert_resource(registry);
    let node = world.spawn(Source(1));
    attach_handlers_for(&mut world, node);
    assert!(view_input_guard(&world, node, 0).is_none());
    let calls = world.resource::<Trace>().unwrap().sources;
    assert!(calls > 0 && calls <= 1024);
    prepare(&mut world);
    assert_eq!(world.resource::<Trace>().unwrap().sources, calls);
}

#[test]
fn panic_releases_active_initialization_without_retrying_the_failed_instance() {
    fn fail(_: &mut crate::ui::UiScope<'_>) {
        panic!("attach failure");
    }
    let mut world = world(false);
    let mut registry = ViewRegistry::default();
    registry.insert(
        View::systems_only("panic", &[])
            .with_filter::<Source>()
            .with_attach(fail),
    );
    world.insert_resource(registry);
    let node = world.spawn(Source(1));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| attach_handlers_for(
            &mut world, node
        )))
        .is_err()
    );
    assert!(
        world
            .resource::<ActiveAttachments>()
            .unwrap()
            .0
            .borrow()
            .is_empty()
    );
    attach_handlers_for(&mut world, node);
    assert!(view_input_guard(&world, node, 0).is_none());
}

#[crate::event]
fn install_capability(node: entity!(), commands: commands!()) {
    commands.insert(node, Capability);
}

#[crate::event]
fn initialize_capability(trace: res!(mut Trace)) {
    trace.capabilities += 1;
}

#[crate::event]
fn use_capability(_tap: event!(crate::input::event::gesture::Tap), trace: res!(mut Trace)) {
    trace.sources += 1;
}

#[crate::view(component = Source, attach = install_capability::callback())]
fn typed_source() {}

#[crate::view(component = Capability, attach = initialize_capability::callback(), input = use_capability::callback())]
fn typed_capability() {}

#[test]
fn typed_attachment_commands_finish_before_input_and_never_retarget_old_snapshots() {
    use crate::input::event::{contract::Event, gesture::Tap, listeners::EventListeners};
    let mut world = world(false);
    let mut registry = ViewRegistry::default();
    registry.insert(typed_capability::view());
    registry.insert(typed_source::view());
    world.insert_resource(registry);
    let node = world.spawn_empty();
    attach_handlers_for(&mut world, node);
    crate::input::event::binding::install_entity_event(
        &mut world,
        node,
        Tap::EVENT,
        install_capability::callback::<Tap, crate::input::event::callback::NodeContext>(),
    );
    let tap = Tap {
        target: node,
        x: crate::types::Fixed::ZERO,
        y: crate::types::Fixed::ZERO,
    };
    let pool = EventListeners::new(&mut world);
    pool.reserve(crate::input::event::listeners::ListenerCapacity {
        listeners: 0,
        snapshot_steps: 1,
    })
    .unwrap();
    crate::ui::delivery::dispatch_gesture(
        &mut world,
        &crate::input::event::gesture::GestureEvent::Tap {
            target: node,
            x: tap.x,
            y: tap.y,
        },
        0,
    )
    .unwrap();
    assert!(view_input_guard(&world, node, 0).is_none());
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(world.resource::<Trace>().unwrap().capabilities, 1);
    let snapshot = pool.snapshot_gesture(&world, &tap).unwrap();
    world.insert(node, Capability);
    prepare(&mut world);
    snapshot.run(&mut world).unwrap();
    assert_eq!(world.resource::<Trace>().unwrap().sources, 0);
    pool.snapshot_gesture(&world, &tap)
        .unwrap()
        .run(&mut world)
        .unwrap();
    assert_eq!(world.resource::<Trace>().unwrap().sources, 1);

    let second = world.spawn(Source(0));
    attach_handlers_for(&mut world, second);
    assert!(view_input_guard(&world, second, 0).is_some());
    assert_eq!(world.resource::<Trace>().unwrap().capabilities, 3);
}

#[test]
fn nested_construction_keeps_parent_and_child_initialization_distinct() {
    fn parent_attach(scope: &mut crate::ui::UiScope<'_>) {
        let parent = scope.parent();
        let world = scope.world_mut();
        let child = world.spawn(Source(0));
        crate::ui::construction::finish_widget_static(world, parent, child);
        world.insert(parent, Source(0));
    }
    #[crate::component]
    struct Container;
    let mut world = world(true);
    world.resource_mut::<ViewRegistry>().unwrap().insert(
        View::systems_only("container", &[])
            .with_filter::<Container>()
            .with_attach(parent_attach),
    );
    let parent = world.spawn(Container);
    attach_handlers_for(&mut world, parent);
    prepare(&mut world);
    let child = world.get::<crate::ui::Children>(parent).unwrap().0[0];
    assert_eq!(world.get::<crate::ui::Parent>(child).unwrap().0, parent);
    assert!(view_input_guard(&world, parent, 0).is_some());
    assert!(view_input_guard(&world, child, 0).is_some());
    let trace = world.resource::<Trace>().unwrap();
    assert_eq!((trace.sources, trace.capabilities), (2, 2));
}

#[test]
fn late_registration_does_not_limit_the_number_of_existing_nodes() {
    let mut world = world(false);
    world.insert_resource(ViewRegistry::default());
    for _ in 0..1100 {
        let node = world.spawn(Capability);
        attach_handlers_for(&mut world, node);
    }
    world
        .resource_mut::<ViewRegistry>()
        .unwrap()
        .insert(capability_view());
    prepare(&mut world);
    assert_eq!(world.resource::<Trace>().unwrap().capabilities, 1100);
}

#[test]
fn queued_retirement_precedes_slot_reuse_and_survives_cascading_removal() {
    #[derive(Default)]
    struct Events(Vec<(Entity, bool)>);
    fn attach(scope: &mut crate::ui::UiScope<'_>) {
        let entity = scope.parent();
        scope
            .world_mut()
            .resource_mut::<Events>()
            .unwrap()
            .0
            .push((entity, true));
    }
    fn retire(world: &mut World, entity: Entity) {
        world
            .resource_mut::<Events>()
            .unwrap()
            .0
            .push((entity, false));
        if let Some(next) = world.resource::<Entity>().copied() {
            world.remove_resource::<Entity>();
            world.remove::<Source>(next);
        }
    }
    for filtered in [false, true] {
        let mut world = World::new();
        let mut registry = ViewRegistry::default();
        let view = View::systems_only("retirement", &[])
            .with_attach(attach)
            .with_retire(retire);
        registry.insert(if filtered {
            view.with_filter::<Source>()
        } else {
            view
        });
        world.insert_resource(registry);
        world.insert_resource(Events::default());
        let old = world.spawn(Source(0));
        attach_handlers_for(&mut world, old);
        let other = world.spawn(Source(0));
        attach_handlers_for(&mut world, other);
        prepare(&mut world);
        world.resource_mut::<Events>().unwrap().0.clear();
        if filtered {
            world.insert_resource(other);
        }
        world.despawn(old);
        let new = world.spawn(Source(0));
        assert_eq!(new.id, old.id);
        assert_ne!(new, old);
        attach_handlers_for(&mut world, new);
        assert_eq!(
            world.resource::<Events>().unwrap().0[..2],
            [(old, false), (new, true)]
        );
        prepare(&mut world);
        assert!(view_input_guard(&world, new, 0).is_some());
        assert_eq!(
            world
                .resource::<Events>()
                .unwrap()
                .0
                .iter()
                .filter(|&&(e, on)| e == old && !on)
                .count(),
            1
        );
        if filtered {
            assert!(view_input_guard(&world, other, 0).is_none());
            assert_eq!(
                world.resource::<Events>().unwrap().0.last(),
                Some(&(other, false))
            );
        }
        world.resource_mut::<Events>().unwrap().0.clear();
        world.despawn(new);
        prepare(&mut world);
        assert_eq!(world.resource::<Events>().unwrap().0, [(new, false)]);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..8 {
                    prepare(&mut world);
                }
            }),
            0
        );
        assert_eq!(world.resource::<Events>().unwrap().0, [(new, false)]);
    }
}

#[test]
fn reconciliation_cannot_invoke_a_replaced_or_deleted_later_instance() {
    #[derive(Default)]
    struct ReconcileState {
        change: bool,
        calls: Vec<Entity>,
        removed: Vec<Entity>,
    }
    fn retire(world: &mut World, entity: Entity) {
        world
            .resource_mut::<ReconcileState>()
            .unwrap()
            .removed
            .push(entity);
    }
    fn reconcile(world: &mut World, entity: Entity) -> bool {
        assert!(world.has::<Source>(entity));
        world
            .resource_mut::<ReconcileState>()
            .unwrap()
            .calls
            .push(entity);
        if world.resource::<ReconcileState>().unwrap().change {
            world.resource_mut::<ReconcileState>().unwrap().change = false;
            let [replace, delete] = *world.resource::<[Entity; 2]>().unwrap();
            world.insert(replace, Source(2));
            world.despawn(delete);
        }
        false
    }
    let mut world = World::new();
    let mut registry = ViewRegistry::default();
    registry.insert(
        View::systems_only("reconcile", &[])
            .with_filter::<Source>()
            .with_reconcile(reconcile)
            .with_retire(retire),
    );
    world.insert_resource(registry);
    world.insert_resource(ReconcileState::default());
    let nodes = core::array::from_fn::<_, 3, _>(|_| {
        let entity = world.spawn(Source(0));
        attach_handlers_for(&mut world, entity);
        entity
    });
    prepare(&mut world);
    world.insert_resource([nodes[1], nodes[2]]);
    *world.resource_mut::<ReconcileState>().unwrap() = ReconcileState {
        change: true,
        ..Default::default()
    };
    prepare(&mut world);
    let state = world.resource::<ReconcileState>().unwrap();
    assert!(!state.calls.contains(&nodes[2]));
    assert!(state.calls.contains(&nodes[1]));
    assert_eq!(state.removed.len(), 2);
    assert!(state.removed.contains(&nodes[1]));
    assert!(state.removed.contains(&nodes[2]));
    assert!(view_input_guard(&world, nodes[1], 0).is_some());
    assert!(view_input_guard(&world, nodes[2], 0).is_none());
}
