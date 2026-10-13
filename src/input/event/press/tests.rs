use super::*;
use crate::input::event::{
    callback::NodeContext, gesture::system::GestureSystem, input::PointerKind, runtime,
};
use crate::types::{Dimension, Viewport};
use crate::ui::{Children, HitTarget, Style, Widget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Notice {
    Down,
    Up,
    Cancel(PressCancelReason),
}

#[derive(Default)]
struct Trace {
    events: [Option<(Entity, Notice)>; 32],
    len: usize,
}
impl Trace {
    fn record(&mut self, entity: Entity, notice: Notice) {
        self.events[self.len] = Some((entity, notice));
        self.len += 1;
    }
}

#[crate::event]
fn pressed(node: entity!(), trace: res!(mut Trace)) {
    trace.record(node, Notice::Down);
}
#[crate::event]
fn released(node: entity!(), trace: res!(mut Trace)) {
    trace.record(node, Notice::Up);
}
#[crate::event]
fn cancelled(node: entity!(), event: event!(PressCancelled), trace: res!(mut Trace)) {
    trace.record(node, Notice::Cancel(event.reason));
}

fn listen(world: &mut World, node: Entity) {
    runtime::install_entity(
        world,
        node,
        Pressed::EVENT,
        pressed::callback::<Pressed, NodeContext>(),
    )
    .unwrap();
    runtime::install_entity(
        world,
        node,
        Released::EVENT,
        released::callback::<Released, NodeContext>(),
    )
    .unwrap();
    runtime::install_entity(
        world,
        node,
        PressCancelled::EVENT,
        cancelled::callback::<NodeContext>(),
    )
    .unwrap();
}

fn setup(capacity: super::super::EventCapacity) -> (World, Entity, Entity) {
    let mut world = World::new();
    runtime::configure(&mut world, capacity).unwrap();
    world.insert_resource(Trace::default());
    world.insert_resource(GestureSystem::default());
    let root = world.spawn_empty();
    let child = world.spawn_empty();
    for entity in [root, child] {
        world.insert(entity, Widget);
        world.insert(
            entity,
            Style {
                layout: crate::ui::layout::LayoutStyle {
                    width: Dimension::px(100),
                    height: Dimension::px(100),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        listen(&mut world, entity);
    }
    world.insert(root, Children(alloc::vec![child]));
    world.insert(child, Parent(root));
    world.insert(child, HitTarget);
    crate::ui::render_system::try_update_layout(
        &mut world,
        root,
        &Viewport::new(100, 100, Fixed::ONE),
    )
    .unwrap();
    (world, root, child)
}

fn down() -> InputEvent {
    InputEvent::PointerDown {
        kind: PointerKind::Mouse,
        id: 0,
        x: 10.into(),
        y: 10.into(),
    }
}
fn up() -> InputEvent {
    InputEvent::PointerUp {
        kind: PointerKind::Mouse,
        id: 0,
        x: 10.into(),
        y: 10.into(),
    }
}
fn moved(x: i32) -> InputEvent {
    InputEvent::PointerMove {
        kind: PointerKind::Mouse,
        id: 0,
        x: x.into(),
        y: 10.into(),
    }
}
fn input(world: &mut World, root: Entity, input: InputEvent) {
    super::super::dispatch_input(world, Some(root), &input, 10, 100, 100);
    crate::ui::delivery::flush(world).unwrap();
}
fn take(world: &mut World) -> Vec<(Entity, Notice)> {
    let trace = world.resource_mut::<Trace>().unwrap();
    let events = trace.events[..trace.len]
        .iter()
        .flatten()
        .copied()
        .collect();
    trace.len = 0;
    events
}

#[test]
fn a_press_captures_the_original_path_and_ignores_other_pointer_endings() {
    let (mut world, root, child) = setup(Default::default());
    assert!(!world.has::<HitTarget>(root));
    input(&mut world, root, down());
    assert_eq!(
        take(&mut world),
        [(child, Notice::Down), (root, Notice::Down)]
    );
    input(&mut world, root, down());
    input(
        &mut world,
        root,
        InputEvent::PointerUp {
            kind: PointerKind::Touch,
            id: 0,
            x: 10.into(),
            y: 10.into(),
        },
    );
    input(
        &mut world,
        root,
        InputEvent::PointerCancel {
            kind: PointerKind::Mouse,
            id: 1,
            x: 10.into(),
            y: 10.into(),
        },
    );
    input(
        &mut world,
        root,
        InputEvent::PointerLeave {
            kind: PointerKind::Mouse,
            id: 0,
        },
    );
    assert!(take(&mut world).is_empty());
    let new_parent = world.spawn_empty();
    listen(&mut world, new_parent);
    world.insert(child, Parent(new_parent));
    input(&mut world, root, up());
    assert_eq!(take(&mut world), [(child, Notice::Up), (root, Notice::Up)]);
    input(&mut world, root, up());
    assert!(take(&mut world).is_empty());
}

#[test]
fn drag_multi_pointer_and_platform_cancel_each_end_only_once() {
    for (event, reason) in [
        (moved(30), PressCancelReason::Gesture),
        (
            InputEvent::PointerDown {
                kind: PointerKind::Touch,
                id: 0,
                x: 10.into(),
                y: 10.into(),
            },
            PressCancelReason::Gesture,
        ),
        (
            InputEvent::PointerCancel {
                kind: PointerKind::Mouse,
                id: 0,
                x: 10.into(),
                y: 10.into(),
            },
            PressCancelReason::Platform,
        ),
    ] {
        let (mut world, root, child) = setup(Default::default());
        input(&mut world, root, down());
        take(&mut world);
        input(&mut world, root, event);
        assert_eq!(
            take(&mut world),
            [
                (child, Notice::Cancel(reason)),
                (root, Notice::Cancel(reason))
            ]
        );
        input(&mut world, root, up());
        assert!(take(&mut world).is_empty());
    }
}

#[test]
fn long_press_keeps_capture_and_release_need_not_be_a_tap() {
    let (mut world, root, child) = setup(Default::default());
    input(&mut world, root, down());
    take(&mut world);
    let gestures = world.resource_mut::<GestureSystem>().unwrap();
    gestures
        .recognizer
        .check_long_press(600, &mut gestures.events);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    input(
        &mut world,
        root,
        InputEvent::PointerUp {
            kind: PointerKind::Mouse,
            id: 0,
            x: 120.into(),
            y: 10.into(),
        },
    );
    assert_eq!(take(&mut world), [(child, Notice::Up), (root, Notice::Up)]);
}

#[test]
fn cancellation_reasons_and_unavailable_targets_do_not_become_releases() {
    for reason in [
        PressCancelReason::Platform,
        PressCancelReason::Suspended,
        PressCancelReason::Unavailable,
    ] {
        let (mut world, root, child) = setup(Default::default());
        input(&mut world, root, down());
        take(&mut world);
        if reason == PressCancelReason::Unavailable {
            world.insert(child, crate::ui::Hidden);
            world.callback_completed();
        } else {
            super::super::cancel_active_input(&mut world, 20, reason);
        }
        crate::ui::delivery::flush(&mut world).unwrap();
        assert_eq!(
            take(&mut world),
            [
                (child, Notice::Cancel(reason)),
                (root, Notice::Cancel(reason))
            ]
        );
        input(&mut world, root, up());
        assert!(take(&mut world).is_empty());
    }
}

#[test]
fn retired_target_does_not_deliver_termination_to_replacement_data() {
    let (mut world, root, child) = setup(Default::default());
    let gate = retained::RetainedEffects::new(&mut world, |_, _| true);
    world.insert(child, gate);
    input(&mut world, root, down());
    take(&mut world);
    super::super::retire_subtree_interaction(&mut world, child);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(
        take(&mut world),
        [(root, Notice::Cancel(PressCancelReason::Unavailable))]
    );
    input(&mut world, root, up());
    assert!(take(&mut world).is_empty());
}

#[test]
fn notification_overflow_does_not_retry_or_change_recognition() {
    let (mut world, root, child) = setup(
        super::super::EventCapacity::new()
            .notifications::<PressCancelled>(1)
            .flush_limit(4),
    );
    input(&mut world, root, down());
    assert_eq!(take(&mut world), [(child, Notice::Down)]);
    input(&mut world, root, up());
    assert_eq!(take(&mut world), [(child, Notice::Up)]);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
}

#[test]
fn warmed_press_delivery_does_not_allocate_and_fits_the_default_payload() {
    use super::super::focus::FocusChanged;
    assert!(core::mem::size_of::<PressCancelled>() <= core::mem::size_of::<FocusChanged>());
    assert!(core::mem::align_of::<PressCancelled>() <= core::mem::align_of::<FocusChanged>());
    let (mut world, root, _) = setup(Default::default());
    input(&mut world, root, down());
    input(&mut world, root, up());
    take(&mut world);
    world
        .resource_mut::<GestureSystem>()
        .unwrap()
        .events
        .clear();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            input(&mut world, root, down());
            input(&mut world, root, up());
        }),
        0
    );
}

#[test]
fn scroll_takeover_cancels_press_before_release() {
    use crate::input::event::scroll::{ScrollConfig, ScrollDragState, ScrollOffset};
    let (mut world, root, child) = setup(Default::default());
    world.insert_resource(ScrollDragState::default());
    world.insert(
        root,
        ScrollConfig {
            content_height: 300.into(),
            ..Default::default()
        },
    );
    world.insert(root, ScrollOffset::default());
    input(&mut world, root, down());
    take(&mut world);
    input(
        &mut world,
        root,
        InputEvent::PointerMove {
            kind: PointerKind::Mouse,
            id: 0,
            x: 10.into(),
            y: (-20).into(),
        },
    );
    assert!(world.resource::<ScrollDragState>().unwrap().resolved);
    assert_eq!(
        take(&mut world),
        [
            (child, Notice::Cancel(PressCancelReason::Scroll)),
            (root, Notice::Cancel(PressCancelReason::Scroll))
        ]
    );
    input(&mut world, root, down());
    assert!(take(&mut world).is_empty());
    input(&mut world, root, up());
    assert!(take(&mut world).is_empty());
}

#[test]
fn listening_after_capture_does_not_join_the_frozen_path() {
    let (mut world, root, child) = setup(Default::default());
    let outer = world.spawn_empty();
    world.insert(root, Parent(outer));
    input(&mut world, root, down());
    take(&mut world);
    listen(&mut world, outer);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    input(&mut world, root, up());
    assert_eq!(take(&mut world), [(child, Notice::Up), (root, Notice::Up)]);
}

#[test]
fn disabled_or_removed_targets_end_the_surviving_capture_path() {
    for removed in [false, true] {
        let (mut world, root, child) = setup(Default::default());
        input(&mut world, root, down());
        take(&mut world);
        if removed {
            world.despawn(child);
        } else {
            world.insert(child, crate::ui::UserState::Disabled);
        }
        world.callback_completed();
        crate::ui::delivery::flush(&mut world).unwrap();
        let events = take(&mut world);
        assert_eq!(
            events.last(),
            Some(&(root, Notice::Cancel(PressCancelReason::Unavailable)))
        );
        assert_eq!(events.len(), if removed { 1 } else { 2 });
        input(&mut world, root, up());
        assert!(take(&mut world).is_empty());
    }
}
