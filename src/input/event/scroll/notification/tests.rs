use super::super::controller::Driver;
use super::*;
use crate::input::event::{EventCapacity, callback::NodeContext, runtime};
use crate::test_support::tracked_allocations;
use crate::types::Fixed;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Notice {
    Start(ScrollStarted),
    Change(ScrollChanged),
    End(ScrollEnded),
}

struct Trace {
    entries: [Option<(Entity, Notice)>; 32],
    len: usize,
    redirect: Option<(Entity, ScrollOffset)>,
}

impl Trace {
    fn push(&mut self, entity: Entity, notice: Notice) {
        self.entries[self.len] = Some((entity, notice));
        self.len += 1;
    }
}

#[crate::event]
fn started(node: entity!(), event: event!(ScrollStarted), trace: res!(mut Trace)) {
    trace.push(node, Notice::Start(*event));
}

#[crate::event]
fn changed(
    node: entity!(),
    event: event!(ScrollChanged),
    trace: res!(mut Trace),
    offsets: query!(mut ScrollOffset),
) {
    trace.push(node, Notice::Change(*event));
    if let Some((node, next)) = trace.redirect.take()
        && let Some(offset) = offsets.get(node)
    {
        *offset = next;
    }
}

#[crate::event]
fn ended(node: entity!(), event: event!(ScrollEnded), trace: res!(mut Trace)) {
    trace.push(node, Notice::End(*event));
}

fn listen(world: &mut World, node: Entity) {
    runtime::install_entity(
        world,
        node,
        ScrollStarted::EVENT,
        started::callback::<NodeContext>(),
    )
    .unwrap();
    runtime::install_entity(
        world,
        node,
        ScrollChanged::EVENT,
        changed::callback::<NodeContext>(),
    )
    .unwrap();
    runtime::install_entity(
        world,
        node,
        ScrollEnded::EVENT,
        ended::callback::<NodeContext>(),
    )
    .unwrap();
}

fn setup(capacity: EventCapacity) -> (World, Entity) {
    let mut world = World::new();
    world.insert_resource(Trace {
        entries: [None; 32],
        len: 0,
        redirect: None,
    });
    world.insert_resource(ScrollMotion::default());
    runtime::configure(&mut world, capacity).unwrap();
    let node = world.spawn_empty();
    world.insert(node, ScrollOffset::default());
    listen(&mut world, node);
    (world, node)
}

fn offset(y: Fixed) -> ScrollOffset {
    ScrollOffset { x: Fixed::ZERO, y }
}

fn write(world: &mut World, node: Entity, y: Fixed) {
    *world.get_mut::<ScrollOffset>(node).unwrap() = offset(y);
}

fn take(world: &mut World) -> alloc::vec::Vec<(Entity, Notice)> {
    let trace = world.resource_mut::<Trace>().unwrap();
    let result = trace.entries[..trace.len]
        .iter()
        .flatten()
        .copied()
        .collect();
    trace.len = 0;
    result
}

fn triplet(node: Entity, old: Fixed, new: Fixed) -> [(Entity, Notice); 3] {
    [
        (
            node,
            Notice::Start(ScrollStarted {
                offset: offset(old),
            }),
        ),
        (
            node,
            Notice::Change(ScrollChanged {
                old: offset(old),
                new: offset(new),
            }),
        ),
        (
            node,
            Notice::End(ScrollEnded {
                offset: offset(new),
                reason: ScrollEndReason::Settled,
            }),
        ),
    ]
}

#[test]
fn program_changes_publish_fractional_triplets_without_hot_allocations() {
    let (mut world, node) = setup(EventCapacity::default());
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
    let next = -Fixed::ONE / 4;
    assert_eq!(
        tracked_allocations(|| {
            write(&mut world, node, 100.into());
            write(&mut world, node, next);
            world.callback_completed();
            crate::ui::delivery::flush(&mut world).unwrap();
        }),
        0
    );
    assert_eq!(take(&mut world), triplet(node, Fixed::ZERO, next));
    write(&mut world, node, 80.into());
    write(&mut world, node, next);
    world.callback_completed();
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
}

#[test]
fn active_motion_survives_stationary_samples_and_ends_only_after_controller() {
    for driver in [Driver::Pointer, Driver::Stream] {
        let (mut world, node) = setup(EventCapacity::default());
        world.resource_mut::<ScrollMotion>().unwrap().record(
            node,
            (Fixed::ZERO, Fixed::ONE),
            driver,
            8,
        );
        write(&mut world, node, Fixed::ONE);
        crate::ui::delivery::flush(&mut world).unwrap();
        let notices = take(&mut world);
        assert_eq!(notices.len(), 2);
        assert_eq!(notices[0], triplet(node, Fixed::ZERO, Fixed::ONE)[0]);
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
        *world.resource_mut::<ScrollMotion>().unwrap() = ScrollMotion::default();
        crate::ui::delivery::flush(&mut world).unwrap();
        assert_eq!(
            take(&mut world),
            [triplet(node, Fixed::ZERO, Fixed::ONE)[2]]
        );
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
    }
}

#[test]
fn cancellation_publishes_clamping_before_one_terminal_notice() {
    let (mut world, node) = setup(EventCapacity::default());
    world.resource_mut::<ScrollMotion>().unwrap().record(
        node,
        (Fixed::ZERO, -Fixed::ONE),
        Driver::Pointer,
        8,
    );
    write(&mut world, node, -Fixed::ONE);
    crate::ui::delivery::flush(&mut world).unwrap();
    take(&mut world);
    super::super::system::cancel_scroll_drag(&mut world);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(
        take(&mut world),
        [
            (
                node,
                Notice::Change(ScrollChanged {
                    old: offset(-Fixed::ONE),
                    new: ScrollOffset::default()
                })
            ),
            (
                node,
                Notice::End(ScrollEnded {
                    offset: ScrollOffset::default(),
                    reason: ScrollEndReason::Cancelled
                })
            ),
        ]
    );
    super::super::system::cancel_scroll_drag(&mut world);
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
}

#[test]
fn callbacks_publish_subsequent_changes_without_reentering_observer_borrows() {
    let (mut world, node) = setup(EventCapacity::default());
    world.resource_mut::<Trace>().unwrap().redirect = Some((node, offset(2.into())));
    write(&mut world, node, Fixed::ONE);
    assert_eq!(
        tracked_allocations(|| {
            crate::ui::delivery::flush(&mut world).unwrap();
        }),
        0
    );
    let expected: alloc::vec::Vec<_> = triplet(node, Fixed::ZERO, Fixed::ONE)
        .into_iter()
        .chain(triplet(node, Fixed::ONE, 2.into()))
        .collect();
    assert_eq!(take(&mut world), expected);
}

#[test]
fn dropped_notifications_advance_the_baseline_without_replay() {
    let (mut world, node) = setup(
        EventCapacity::new()
            .notifications::<ScrollChanged>(1)
            .flush_limit(1),
    );
    write(&mut world, node, Fixed::ONE);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(
        take(&mut world),
        [triplet(node, Fixed::ZERO, Fixed::ONE)[0]]
    );
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
    write(&mut world, node, 2.into());
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world), [triplet(node, Fixed::ONE, 2.into())[0]]);
}

#[test]
fn invalid_sources_and_retired_rows_never_deliver_old_motion() {
    let (mut world, node) = setup(EventCapacity::default());
    let gate = retained::RetainedEffects::new(&mut world, |_, _| true);
    world.insert(node, gate);
    crate::ui::delivery::flush(&mut world).unwrap();
    // Install the retained identity before taking a new observation baseline.
    world.remove::<ScrollOffset>(node);
    crate::ui::delivery::flush(&mut world).unwrap();
    world.insert(node, ScrollOffset::default());
    crate::ui::delivery::flush(&mut world).unwrap();
    write(&mut world, node, Fixed::ONE);
    world.callback_completed();
    retained::retire(&mut world, node);
    write(&mut world, node, 90.into());
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    write(&mut world, node, 91.into());
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world), triplet(node, 90.into(), 91.into()));
    world.despawn(node);
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
}

#[test]
fn installing_during_motion_does_not_replay_its_start() {
    let (mut world, _) = setup(EventCapacity::default());
    let node = world.spawn_empty();
    world.insert(node, offset(10.into()));
    world.resource_mut::<ScrollMotion>().unwrap().record(
        node,
        (Fixed::ZERO, Fixed::ONE),
        Driver::Stream,
        8,
    );
    listen(&mut world, node);
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
    write(&mut world, node, 11.into());
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world), [triplet(node, 10.into(), 11.into())[1]]);
    *world.resource_mut::<ScrollMotion>().unwrap() = ScrollMotion::default();
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world), [triplet(node, 10.into(), 11.into())[2]]);
}

#[test]
fn storage_is_prepared_only_for_listened_nodes() {
    let observer = ScrollObservers::default();
    assert_eq!(observer.0.borrow().capacity(), 0);
    let limits = EventCapacity::default()
        .resolve()
        .unwrap()
        .notifications
        .unwrap();
    assert!(limits.payload_alignment >= core::mem::align_of::<ScrollChanged>());
    assert!(
        limits.payload_bytes
            >= core::mem::size_of::<ScrollStarted>()
                + core::mem::size_of::<ScrollChanged>()
                + core::mem::size_of::<ScrollEnded>()
    );
}

std::thread_local! {
    static TIME: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

fn clock() -> u64 {
    TIME.with(core::cell::Cell::get)
}

fn scrolling_world() -> (World, Entity) {
    let (mut world, node) = setup(EventCapacity::default());
    TIME.with(|time| time.set(0));
    world.insert_resource(crate::ecs::MonoClock::new(clock));
    world.insert_resource(crate::ecs::DeltaTimeMs(8));
    world.insert_resource(super::super::ScrollDragState {
        last_resolved_target: Some(node),
        ..Default::default()
    });
    world.insert(node, crate::ui::Widget);
    world.insert(
        node,
        crate::ui::Style {
            layout: crate::ui::layout::LayoutStyle {
                width: crate::types::Dimension::px(100),
                height: crate::types::Dimension::px(100),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(
        node,
        super::super::ScrollConfig {
            content_height: 300.into(),
            ..Default::default()
        },
    );
    crate::ui::render_system::update_layout(
        &mut world,
        node,
        &crate::types::Viewport::new(100, 100, Fixed::ONE),
    );
    (world, node)
}

fn input(world: &mut World, node: Entity, input: crate::input::event::input::InputEvent, now: u32) {
    TIME.with(|time| time.set(u64::from(now) * 1_000_000));
    super::super::system::scroll_system_with_target(world, node, &input, 100, 100, Some(node), now);
    crate::ui::delivery::flush(world).unwrap();
}

#[test]
fn wheel_stream_and_pointer_inertia_end_after_real_controller_settles() {
    use crate::input::event::input::{InputEvent, PointerKind};
    for wheel in [false, true] {
        let (mut world, node) = scrolling_world();
        if !wheel {
            input(
                &mut world,
                node,
                InputEvent::PointerDown {
                    kind: PointerKind::Mouse,
                    id: 0,
                    x: 10.into(),
                    y: 50.into(),
                },
                0,
            );
            assert!(take(&mut world).is_empty());
        }
        let mut starts = 0;
        let mut ends = 0;
        for (time, y) in [(8, 40), (16, 30), (24, 20)] {
            let event = if wheel {
                InputEvent::Wheel {
                    dx: Fixed::ZERO,
                    dy: (-10).into(),
                    x: 10.into(),
                    y: 10.into(),
                }
            } else {
                InputEvent::PointerMove {
                    kind: PointerKind::Mouse,
                    id: 0,
                    x: 10.into(),
                    y: y.into(),
                }
            };
            input(&mut world, node, event, time);
            for (_, notice) in take(&mut world) {
                starts += usize::from(matches!(notice, Notice::Start(_)));
                assert!(!matches!(notice, Notice::End(_)));
            }
        }
        if !wheel {
            input(
                &mut world,
                node,
                InputEvent::PointerUp {
                    kind: PointerKind::Mouse,
                    id: 0,
                    x: 10.into(),
                    y: 20.into(),
                },
                24,
            );
            assert!(take(&mut world).is_empty());
        }
        for frame in 4..300 {
            TIME.with(|time| time.set(frame * 8_000_000));
            super::super::system::scroll_inertia_system(&mut world).unwrap();
            crate::ui::delivery::flush(&mut world).unwrap();
            for (_, notice) in take(&mut world) {
                assert!(!matches!(notice, Notice::Start(_)));
                if let Notice::End(end) = notice {
                    ends += 1;
                    assert_eq!(end.reason, ScrollEndReason::Settled);
                    assert!(!active(&world, node));
                }
            }
            if !active(&world, node) {
                break;
            }
        }
        assert_eq!((starts, ends), (1, 1), "wheel={wheel}");
    }
}

#[test]
fn rotary_is_a_discrete_segment_and_hard_boundaries_do_not_start_motion() {
    use crate::input::event::input::InputEvent;
    let (mut world, node) = scrolling_world();
    input(&mut world, node, InputEvent::Rotary { id: 0, delta: -1 }, 0);
    assert_eq!(take(&mut world), triplet(node, Fixed::ZERO, 20.into()));
    world
        .get_mut::<super::super::ScrollConfig>(node)
        .unwrap()
        .elastic = false;
    write(&mut world, node, Fixed::ZERO);
    crate::ui::delivery::flush(&mut world).unwrap();
    take(&mut world);
    input(
        &mut world,
        node,
        InputEvent::Wheel {
            dx: Fixed::ZERO,
            dy: 20.into(),
            x: 10.into(),
            y: 10.into(),
        },
        8,
    );
    assert!(take(&mut world).is_empty());
    TIME.with(|time| time.set(200_000_000));
    super::super::system::scroll_inertia_system(&mut world).unwrap();
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
}

#[test]
fn hidden_detached_and_removed_sources_rebase_without_sending_old_segments() {
    let (mut world, node) = setup(EventCapacity::default());
    world.insert_resource(crate::ui::WidgetRoot(node));
    world.insert(node, crate::ui::Hidden);
    write(&mut world, node, Fixed::ONE);
    crate::ui::delivery::flush(&mut world).unwrap();
    world.remove::<crate::ui::Hidden>(node);
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    let root = world.spawn_empty();
    world.resource_mut::<crate::ui::WidgetRoot>().unwrap().0 = root;
    write(&mut world, node, 2.into());
    crate::ui::delivery::flush(&mut world).unwrap();
    world.insert(node, crate::ui::Parent(root));
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    world.remove::<ScrollOffset>(node);
    crate::ui::delivery::flush(&mut world).unwrap();
    world.insert(node, offset(100.into()));
    crate::ui::delivery::flush(&mut world).unwrap();
    assert!(take(&mut world).is_empty());
    write(&mut world, node, 101.into());
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(take(&mut world), triplet(node, 100.into(), 101.into()));
}

#[test]
fn switching_targets_cancels_the_old_segment_and_same_target_takeover_keeps_it() {
    use crate::input::event::input::{InputEvent, PointerKind};
    let (mut world, first) = scrolling_world();
    let second = world.spawn_empty();
    world.insert(second, ScrollOffset::default());
    world.insert(
        second,
        super::super::ScrollConfig {
            content_height: 300.into(),
            ..Default::default()
        },
    );
    world.insert(
        second,
        crate::ui::ComputedRect(crate::types::Rect::new(0, 0, 100, 100)),
    );
    listen(&mut world, second);
    input(
        &mut world,
        first,
        InputEvent::Wheel {
            dx: Fixed::ZERO,
            dy: (-10).into(),
            x: 10.into(),
            y: 10.into(),
        },
        0,
    );
    take(&mut world);
    let pointer = |down, y| {
        if down {
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: 10.into(),
                y,
            }
        } else {
            InputEvent::PointerMove {
                kind: PointerKind::Mouse,
                id: 0,
                x: 10.into(),
                y,
            }
        }
    };
    input(&mut world, first, pointer(true, 50.into()), 8);
    input(&mut world, first, pointer(false, 40.into()), 16);
    assert!(
        take(&mut world)
            .iter()
            .all(|(_, notice)| matches!(notice, Notice::Change(_)))
    );
    input(
        &mut world,
        first,
        InputEvent::PointerUp {
            kind: PointerKind::Mouse,
            id: 0,
            x: 10.into(),
            y: 40.into(),
        },
        16,
    );
    take(&mut world);
    input(&mut world, second, pointer(true, 50.into()), 24);
    input(&mut world, second, pointer(false, 40.into()), 32);
    let notices = take(&mut world);
    assert_eq!(notices.len(), 3);
    assert!(
        matches!(notices[0], (node, Notice::End(ScrollEnded { reason: ScrollEndReason::Cancelled, .. })) if node == first)
    );
    assert!(matches!(notices[1], (node, Notice::Start(_)) if node == second));
    assert!(matches!(notices[2], (node, Notice::Change(_)) if node == second));
}

#[test]
fn scroll_listeners_require_the_offset_source_without_adding_hit_feedback() {
    let (mut world, node) = setup(EventCapacity::default());
    assert!(!world.has::<crate::ui::HitTarget>(node));
    let missing = world.spawn_empty();
    assert!(
        runtime::install_entity(
            &mut world,
            missing,
            ScrollChanged::EVENT,
            changed::callback::<NodeContext>()
        )
        .is_err()
    );
}
