use super::super::{
    callback::NodeContext,
    gesture::{GestureEvent, Tap},
    runtime,
};
use super::*;
use crate::test_support::tracked_allocations;
use crate::types::Fixed;

#[derive(Default)]
struct Trace {
    entries: [Option<(Entity, FocusChanged)>; 16],
    len: usize,
    redirect: Option<Entity>,
    hide_on_gain: Option<Entity>,
}

#[crate::event]
fn changed(
    event: event!(FocusChanged),
    node: entity!(),
    trace: res!(mut Trace),
    focus: res!(mut FocusState),
) {
    trace.entries[trace.len] = Some((node, *event));
    trace.len += 1;
    if !event.focused
        && let Some(next) = trace.redirect.take()
    {
        focus.focused = Some(next);
    }
}

fn setup() -> (World, [Entity; 3]) {
    let mut world = World::new();
    world.insert_resource(FocusState::default());
    world.insert_resource(Trace::default());
    let nodes = core::array::from_fn(|_| {
        let node = world.spawn_empty();
        world.insert(node, Focusable);
        runtime::install_entity(
            &mut world,
            node,
            FocusChanged::EVENT,
            changed::callback::<NodeContext>(),
        )
        .unwrap();
        node
    });
    (world, nodes)
}

fn select(world: &mut World, node: Option<Entity>) {
    world.resource_mut::<FocusState>().unwrap().focused = node;
    world.callback_completed();
}

fn take(world: &mut World) -> alloc::vec::Vec<(Entity, FocusChanged)> {
    let trace = world.resource_mut::<Trace>().unwrap();
    let result = trace.entries[..trace.len]
        .iter()
        .flatten()
        .copied()
        .collect();
    trace.len = 0;
    result
}

struct Targets(Entity, Entity);

#[crate::event]
fn move_focus(targets: res!(Targets), focus: res!(mut FocusState)) {
    focus.focused = Some(targets.0);
    focus.focused = None;
    focus.focused = Some(targets.1);
}

#[crate::system]
fn focus_first(targets: res!(Targets), focus: res!(mut FocusState)) {
    focus.focused = Some(targets.0);
}

#[crate::system]
fn focus_second(targets: res!(Targets), focus: res!(mut FocusState)) {
    focus.focused = Some(targets.1);
}

#[test]
fn author_callback_folds_local_writes_and_systems_keep_separate_transitions() {
    let (mut world, [a, b, _]) = setup();
    world.insert_resource(Targets(a, b));
    let callback = move_focus::callback::<Tap, NodeContext>()
        .prepare(&mut world, crate::ecs::CommandCapacity::new())
        .unwrap();
    callback
        .invoke(
            &mut world,
            &Tap {
                target: a,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            a,
        )
        .unwrap();
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(
        take(&mut world),
        alloc::vec![(
            b,
            FocusChanged {
                focused: true,
                related: None
            }
        )]
    );
    let mut systems = crate::ecs::system::SystemScheduler::new();
    systems.add(focus_first::system());
    systems.add(focus_second::system());
    assert_eq!(
        tracked_allocations(|| {
            systems.run_all(&mut world).unwrap();
            crate::ui::delivery::flush(&mut world).unwrap();
        }),
        0
    );
    assert_eq!(
        take(&mut world),
        alloc::vec![
            (
                b,
                FocusChanged {
                    focused: false,
                    related: Some(a)
                }
            ),
            (
                a,
                FocusChanged {
                    focused: true,
                    related: Some(b)
                }
            ),
            (
                a,
                FocusChanged {
                    focused: false,
                    related: Some(b)
                }
            ),
            (
                b,
                FocusChanged {
                    focused: true,
                    related: Some(a)
                }
            ),
        ]
    );
}

#[test]
fn transitions_preserve_each_commit_loss_before_gain_and_reentrant_redirect() {
    let (mut world, [a, b, c]) = setup();
    select(&mut world, Some(a));
    crate::ui::delivery::flush(&mut world).unwrap();
    take(&mut world);
    world.resource_mut::<Trace>().unwrap().redirect = Some(c);
    assert_eq!(
        tracked_allocations(|| {
            select(&mut world, Some(b));
            crate::ui::delivery::flush(&mut world).unwrap();
        }),
        0
    );
    assert_eq!(
        take(&mut world),
        alloc::vec![
            (
                a,
                FocusChanged {
                    focused: false,
                    related: Some(b)
                }
            ),
            (
                b,
                FocusChanged {
                    focused: true,
                    related: Some(a)
                }
            ),
            (
                b,
                FocusChanged {
                    focused: false,
                    related: Some(c)
                }
            ),
            (
                c,
                FocusChanged {
                    focused: true,
                    related: Some(b)
                }
            ),
        ]
    );
    assert_eq!(world.resource::<FocusState>().unwrap().focused, Some(c));
    select(&mut world, Some(a));
    select(&mut world, Some(c));
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world).len(), 4);
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
}

#[test]
fn hidden_and_destroyed_focus_clears_without_retargeting_reused_bindings() {
    let (mut world, [a, b, _]) = setup();
    select(&mut world, Some(a));
    crate::ui::delivery::flush(&mut world).unwrap();
    take(&mut world);
    world.insert(a, crate::ui::UserState::Disabled);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    assert_eq!(
        take(&mut world),
        alloc::vec![(
            a,
            FocusChanged {
                focused: false,
                related: None
            }
        )]
    );
    let gate = crate::core::reactive::retained::RetainedEffects::new(&mut world, |_, _| true);
    world.insert(b, gate);
    select(&mut world, Some(b));
    crate::core::reactive::retained::retire(&mut world, b);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    world.despawn(a);
    select(&mut world, Some(a));
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
}

#[test]
fn first_listener_does_not_replay_existing_focus_and_overflow_is_not_retried() {
    let mut world = World::new();
    let a = world.spawn_empty();
    let b = world.spawn_empty();
    world.insert_resource(FocusState { focused: Some(a) });
    world.insert_resource(Trace::default());
    runtime::configure(
        &mut world,
        super::super::EventCapacity::new()
            .notifications::<FocusChanged>(1)
            .flush_limit(4),
    )
    .unwrap();
    for node in [a, b] {
        runtime::install_entity(
            &mut world,
            node,
            FocusChanged::EVENT,
            changed::callback::<NodeContext>(),
        )
        .unwrap();
    }
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
    select(&mut world, Some(b));
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(
        take(&mut world),
        alloc::vec![(
            a,
            FocusChanged {
                focused: false,
                related: Some(b)
            }
        )]
    );
    assert_eq!(world.resource::<FocusState>().unwrap().focused, Some(b));
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
}

#[crate::event]
fn tapped(trace: res!(mut Trace)) {
    assert_eq!(trace.len, 1);
    assert!(trace.entries[0].unwrap().1.focused);
}

#[test]
fn tap_delivers_focus_before_authored_tap() {
    let (mut world, [a, _, _]) = setup();
    runtime::install_entity(
        &mut world,
        a,
        Tap::EVENT,
        tapped::callback::<Tap, NodeContext>(),
    )
    .unwrap();
    let event: GestureEvent = Tap {
        target: a,
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    }
    .into();
    crate::ui::delivery::dispatch_gesture(&mut world, &event, 0).unwrap();
    assert_eq!(world.resource::<Trace>().unwrap().len, 1);
}

#[crate::event]
fn unexpected_tap() {
    panic!("focus callback invalidated the tap target");
}

#[derive(Default)]
struct ActivationCount(usize);

#[crate::event]
fn activate(count: res!(mut ActivationCount)) {
    count.0 += 1;
}

fn tap(world: &mut World, target: Entity) {
    let event = Tap {
        target,
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    }
    .into();
    crate::ui::delivery::dispatch_gesture(world, &event, 0).unwrap();
}

#[test]
fn pointer_focus_uses_nearest_explicit_policy_without_blocking_activation() {
    let (mut world, [editor, child, parent]) = setup();
    world.insert(child, crate::ui::Parent(parent));
    world.insert(parent, PointerFocus::Preserve);
    world.insert_resource(ActivationCount::default());
    runtime::install_entity(
        &mut world,
        child,
        Tap::EVENT,
        activate::callback::<Tap, NodeContext>(),
    )
    .unwrap();
    select(&mut world, Some(editor));
    crate::ui::delivery::flush(&mut world).unwrap();
    take(&mut world);
    tap(&mut world, child);
    world.resource_mut::<ActivationCount>().unwrap().0 = 0;
    assert_eq!(tracked_allocations(|| tap(&mut world, child)), 0);
    assert_eq!(
        world.resource::<FocusState>().unwrap().focused,
        Some(editor)
    );
    assert!(take(&mut world).is_empty());
    assert_eq!(world.resource::<ActivationCount>().unwrap().0, 1);

    world.insert(child, PointerFocus::Automatic);
    tap(&mut world, child);
    assert_eq!(world.resource::<FocusState>().unwrap().focused, Some(child));
    world.insert(child, PointerFocus::Preserve);
    world.insert(parent, PointerFocus::Automatic);
    select(&mut world, Some(editor));
    tap(&mut world, child);
    assert_eq!(
        world.resource::<FocusState>().unwrap().focused,
        Some(editor)
    );

    world.remove::<PointerFocus>(child);
    world.remove::<PointerFocus>(parent);
    world.remove::<Focusable>(child);
    tap(&mut world, child);
    assert_eq!(
        world.resource::<FocusState>().unwrap().focused,
        Some(parent)
    );
    world.remove::<Focusable>(parent);
    tap(&mut world, child);
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    assert_eq!(world.resource::<ActivationCount>().unwrap().0, 5);
}

#[test]
fn preserving_pointer_focus_never_restores_an_invalid_or_cleared_target() {
    let (mut world, [editor, button, _]) = setup();
    world.insert(button, PointerFocus::Preserve);
    select(&mut world, Some(editor));
    world.insert(editor, crate::ui::Hidden);
    tap(&mut world, button);
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    world.remove::<crate::ui::Hidden>(editor);
    tap(&mut world, button);
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    select(&mut world, Some(editor));
    world.despawn(editor);
    tap(&mut world, button);
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    select(&mut world, Some(button));
    select(&mut world, None);
    tap(&mut world, button);
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
}

#[crate::event]
fn hide_parent(event: event!(FocusChanged), trace: res!(mut Trace), commands: commands!()) {
    if event.focused
        && let Some(node) = trace.hide_on_gain.take()
    {
        commands.insert(node, crate::ui::Hidden);
    }
}

#[test]
fn focus_callback_hiding_an_ancestor_cancels_the_original_tap() {
    let (mut world, [a, parent, _]) = setup();
    world.insert(a, crate::ui::Parent(parent));
    world.resource_mut::<Trace>().unwrap().hide_on_gain = Some(parent);
    runtime::install_entity(
        &mut world,
        a,
        FocusChanged::EVENT,
        hide_parent::callback::<NodeContext>(),
    )
    .unwrap();
    runtime::install_entity(
        &mut world,
        a,
        Tap::EVENT,
        unexpected_tap::callback::<Tap, NodeContext>(),
    )
    .unwrap();
    crate::ui::delivery::dispatch_gesture(
        &mut world,
        &Tap {
            target: a,
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        }
        .into(),
        0,
    )
    .unwrap();
    assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    let events = take(&mut world);
    assert_eq!(events.len(), 2);
    assert!(events[0].1.focused);
    assert!(!events[1].1.focused);
}
