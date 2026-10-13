use super::*;
use crate::input::event::{callback::NodeContext, runtime};
use crate::test_support::tracked_allocations;
use crate::types::{Dimension, Viewport};
use crate::ui::{Children, HitTarget, Style, Widget};

#[derive(Default)]
struct Trace {
    events: [Option<(Entity, bool)>; 32],
    len: usize,
    hide: Option<Entity>,
    taps: usize,
}

#[crate::event]
fn changed(
    event: event!(HoverChanged),
    node: entity!(),
    trace: res!(mut Trace),
    commands: commands!(),
) {
    let index = trace.len;
    trace.events[index] = Some((node, event.hovered));
    trace.len += 1;
    if event.hovered
        && let Some(hide) = trace.hide.take()
    {
        commands.insert(hide, crate::ui::Hidden);
    }
}

fn listen(world: &mut World, node: Entity) {
    runtime::install_entity(
        world,
        node,
        HoverChanged::EVENT,
        changed::callback::<NodeContext>(),
    )
    .unwrap();
}

fn setup() -> (World, [Entity; 3]) {
    setup_with_capacity(super::super::EventCapacity::default())
}

fn setup_with_capacity(capacity: super::super::EventCapacity) -> (World, [Entity; 3]) {
    let mut world = World::new();
    runtime::configure(&mut world, capacity).unwrap();
    world.insert_resource(Trace::default());
    let nodes = core::array::from_fn(|index| {
        let node = world.spawn_empty();
        world.insert(node, Widget);
        world.insert(
            node,
            Style {
                layout: crate::ui::layout::LayoutStyle {
                    width: Dimension::px(100),
                    height: Dimension::px(if index == 0 { 100 } else { 40 }),
                    direction: crate::ui::layout::FlexDirection::Column,
                    shrink: Fixed::ZERO,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(node, Children(Vec::new()));
        listen(&mut world, node);
        node
    });
    let [root, a, b] = nodes;
    for child in [a, b] {
        world.insert(child, Parent(root));
        world.insert(child, HitTarget);
    }
    world.get_mut::<Children>(root).unwrap().0.extend([a, b]);
    layout(&mut world, root);
    (world, nodes)
}

fn layout(world: &mut World, root: Entity) {
    crate::ui::render_system::update_layout(world, root, &Viewport::new(100, 100, Fixed::ONE));
}

fn input(world: &mut World, root: Entity, input: InputEvent) {
    assert_eq!(
        hit_test::geometry_viewport(world).map(|geometry| geometry.0),
        Some(root)
    );
    runtime::observe_pointer(world, &input, 100, 100);
    crate::ui::delivery::flush(world).unwrap();
}

fn moved(kind: PointerKind, id: u8, y: i32) -> InputEvent {
    InputEvent::PointerMove {
        kind,
        id,
        x: 10.into(),
        y: y.into(),
    }
}

fn take(world: &mut World) -> Vec<(Entity, bool)> {
    let trace = world.resource_mut::<Trace>().unwrap();
    let result = trace.events[..trace.len]
        .iter()
        .flatten()
        .copied()
        .collect();
    trace.len = 0;
    result
}

#[test]
fn paths_order_enter_and_leave_without_toggling_shared_ancestors() {
    let (mut world, [root, a, b]) = setup();
    assert!(!world.has::<HitTarget>(root));
    input(&mut world, root, moved(PointerKind::Mouse, 0, 10));
    assert_eq!(take(&mut world), [(root, true), (a, true)]);
    assert_eq!(
        tracked_allocations(|| {
            input(&mut world, root, moved(PointerKind::Mouse, 0, 50));
        }),
        0
    );
    assert_eq!(take(&mut world), [(a, false), (b, true)]);
    input(
        &mut world,
        root,
        InputEvent::PointerLeave {
            kind: PointerKind::Mouse,
            id: 0,
        },
    );
    assert_eq!(take(&mut world), [(b, false), (root, false)]);
    input(
        &mut world,
        root,
        InputEvent::PointerEnter {
            kind: PointerKind::Mouse,
            id: 0,
        },
    );
    assert!(take(&mut world).is_empty());
}

#[test]
fn multiple_hover_pointers_form_a_union_and_contact_does_not_reset_it() {
    let (mut world, [root, a, b]) = setup();
    input(&mut world, root, moved(PointerKind::Mouse, 0, 10));
    take(&mut world);
    input(&mut world, root, moved(PointerKind::Pen, 1, 50));
    assert_eq!(take(&mut world), [(b, true)]);
    input(
        &mut world,
        root,
        InputEvent::PointerDown {
            kind: PointerKind::Mouse,
            id: 0,
            x: 10.into(),
            y: 10.into(),
        },
    );
    assert!(take(&mut world).is_empty());
    input(&mut world, root, moved(PointerKind::Touch, 0, 90));
    input(&mut world, root, moved(PointerKind::Unknown, 0, 90));
    assert!(take(&mut world).is_empty());
    world.insert(root, crate::ui::dirty::Dirty);
    input(
        &mut world,
        root,
        InputEvent::PointerLeave {
            kind: PointerKind::Mouse,
            id: 0,
        },
    );
    assert_eq!(take(&mut world), [(a, false)]);
    input(
        &mut world,
        root,
        InputEvent::PointerLeave {
            kind: PointerKind::Pen,
            id: 1,
        },
    );
    assert_eq!(take(&mut world), [(b, false), (root, false)]);
}

#[test]
fn hiding_or_disabling_a_hit_leaf_updates_ancestors_without_new_pointer_input() {
    for disabled in [false, true] {
        let (mut world, [root, a, _]) = setup();
        input(&mut world, root, moved(PointerKind::Mouse, 0, 10));
        take(&mut world);
        if disabled {
            world.insert(a, crate::ui::UserState::Disabled);
        } else {
            world.insert(a, crate::ui::Hidden);
        }
        world.callback_completed();
        crate::ui::delivery::flush(&mut world).unwrap();
        assert_eq!(take(&mut world), [(a, false), (root, false)]);
    }
}

#[test]
fn published_geometry_survives_paint_dirty_cleanup_and_moves_stationary_hover() {
    let (mut world, [root, a, b]) = setup();
    input(&mut world, root, moved(PointerKind::Mouse, 0, 50));
    take(&mut world);
    world.get_mut::<Style>(a).unwrap().layout.height = Dimension::px(60);
    world.insert(a, crate::ui::dirty::Dirty);
    world.callback_completed();
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    layout(&mut world, root);
    for entity in [root, a, b] {
        world.remove::<crate::ui::dirty::Dirty>(entity);
        world.remove::<crate::ui::dirty::VisualDirty>(entity);
    }
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world), [(b, false), (a, true)]);
}

#[test]
fn duplicate_listener_does_not_duplicate_emission_or_replay_hover() {
    let (mut world, [root, a, _]) = setup();
    input(&mut world, root, moved(PointerKind::Mouse, 0, 10));
    take(&mut world);
    listen(&mut world, a);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    input(
        &mut world,
        root,
        InputEvent::PointerLeave {
            kind: PointerKind::Mouse,
            id: 0,
        },
    );
    assert_eq!(take(&mut world), [(a, false), (a, false), (root, false)]);
}

#[test]
fn failed_enqueue_advances_baseline_and_does_not_retry() {
    let (mut world, [root, a, b]) = setup_with_capacity(
        super::super::EventCapacity::new()
            .notifications::<HoverChanged>(1)
            .commands(crate::ecs::CommandCapacity::new().insert::<crate::ui::Hidden>(1))
            .flush_limit(8),
    );
    input(&mut world, root, moved(PointerKind::Mouse, 0, 10));
    assert_eq!(take(&mut world), [(root, true)]);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    input(&mut world, root, moved(PointerKind::Mouse, 0, 50));
    assert_eq!(take(&mut world), [(a, false)]);
    input(
        &mut world,
        root,
        InputEvent::PointerLeave {
            kind: PointerKind::Mouse,
            id: 0,
        },
    );
    assert_eq!(take(&mut world), [(b, false)]);
}

#[test]
fn retired_identity_drops_queued_notifications_without_retargeting() {
    let (mut world, [root, a, _]) = setup();
    let gate = crate::core::reactive::retained::RetainedEffects::new(&mut world, |_, _| true);
    world.insert(a, gate);
    runtime::observe_pointer(&mut world, &moved(PointerKind::Mouse, 0, 10), 100, 100);
    crate::core::reactive::retained::retire(&mut world, a);
    world
        .get_mut::<crate::core::reactive::retained::RetainedEffects>(a)
        .unwrap()
        .active = false;
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world), [(root, true), (root, false)]);
    world
        .get_mut::<crate::core::reactive::retained::RetainedEffects>(a)
        .unwrap()
        .active = true;
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world), [(root, true), (a, true)]);
}

#[test]
fn callbacks_can_hide_hovered_paths_without_recursive_borrows() {
    let (mut world, [root, a, _]) = setup();
    world.resource_mut::<Trace>().unwrap().hide = Some(a);
    input(&mut world, root, moved(PointerKind::Mouse, 0, 10));
    assert_eq!(
        take(&mut world),
        [(root, true), (a, true), (a, false), (root, false)]
    );
}

#[test]
fn default_notification_payload_already_accommodates_hover() {
    use super::super::focus::FocusChanged;
    assert!(core::mem::size_of::<HoverChanged>() <= core::mem::size_of::<FocusChanged>());
    assert!(core::mem::align_of::<HoverChanged>() <= core::mem::align_of::<FocusChanged>());
}

#[test]
fn listener_on_an_already_hovered_ancestor_starts_without_replay() {
    let (mut world, [root, a, _]) = setup();
    let ancestor = world.spawn_empty();
    world.insert(root, Parent(ancestor));
    input(&mut world, root, moved(PointerKind::Mouse, 0, 10));
    take(&mut world);
    listen(&mut world, ancestor);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    input(
        &mut world,
        root,
        InputEvent::PointerLeave {
            kind: PointerKind::Mouse,
            id: 0,
        },
    );
    assert_eq!(
        take(&mut world),
        [(a, false), (root, false), (ancestor, false)]
    );
}

#[test]
fn exhausted_pointer_storage_never_replaces_a_live_identity() {
    let (mut world, [root, a, b]) = setup();
    input(&mut world, root, moved(PointerKind::Mouse, 0, 10));
    for id in 0..17 {
        input(&mut world, root, moved(PointerKind::Pen, id, 10));
    }
    assert_eq!(take(&mut world), [(root, true), (a, true)]);
    assert_eq!(
        tracked_allocations(|| {
            input(&mut world, root, moved(PointerKind::Pen, 17, 50));
        }),
        0
    );
    assert!(take(&mut world).is_empty());
    input(
        &mut world,
        root,
        InputEvent::PointerLeave {
            kind: PointerKind::Pen,
            id: 0,
        },
    );
    input(&mut world, root, moved(PointerKind::Pen, 17, 50));
    assert_eq!(take(&mut world), [(b, true)]);
    runtime::cancel_hover(&mut world);
    crate::ui::delivery::flush(&mut world).unwrap();
    let events = take(&mut world);
    assert_eq!(events.len(), 3);
    assert_eq!(events.last(), Some(&(root, false)));
    assert!(events.contains(&(a, false)) && events.contains(&(b, false)));
}

#[test]
fn deleting_a_hovered_leaf_clears_surviving_ancestors() {
    let (mut world, [root, a, _]) = setup();
    input(&mut world, root, moved(PointerKind::Mouse, 0, 10));
    take(&mut world);
    world.despawn(a);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world), [(root, false)]);
}

#[crate::event]
fn tap_after_hover(trace: res!(mut Trace)) {
    assert_eq!(trace.len, 2);
    assert!(
        trace.events[..trace.len]
            .iter()
            .all(|entry| entry.unwrap().1)
    );
    trace.taps += 1;
}

#[test]
fn raw_input_delivers_hover_before_gestures_and_rechecks_callback_mutations() {
    use crate::input::event::gesture::{Tap, system::GestureSystem};
    for hide in [false, true] {
        let (mut world, [root, a, _]) = setup();
        world.insert_resource(GestureSystem::default());
        runtime::install_entity(
            &mut world,
            a,
            Tap::EVENT,
            tap_after_hover::callback::<Tap, NodeContext>(),
        )
        .unwrap();
        if hide {
            world.resource_mut::<Trace>().unwrap().hide = Some(a);
        }
        for (now, input) in [
            (
                10,
                InputEvent::PointerDown {
                    kind: PointerKind::Mouse,
                    id: 0,
                    x: 10.into(),
                    y: 10.into(),
                },
            ),
            (
                20,
                InputEvent::PointerUp {
                    kind: PointerKind::Mouse,
                    id: 0,
                    x: 10.into(),
                    y: 10.into(),
                },
            ),
        ] {
            super::super::deliver_input(&mut world, Some(root), &input, now, 100, 100);
        }
        assert_eq!(world.resource::<Trace>().unwrap().taps, usize::from(!hide));
    }
}
