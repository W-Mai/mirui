use super::*;
use crate::ecs::Entity;
use crate::test_support::tracked_allocations;
use crate::types::Fixed;

fn event(value: i32) -> GestureEvent {
    GestureEvent::Tap {
        x: Fixed::from_int(value),
        y: Fixed::ZERO,
        target: Entity {
            id: 0,
            generation: 0,
        },
    }
}

fn value(event: &GestureEvent) -> i32 {
    let GestureEvent::Tap { x, .. } = event else {
        panic!("expected Tap");
    };
    x.to_int()
}

fn enqueue(world: &mut World, value: i32) {
    world
        .resource_mut::<GestureSystem>()
        .unwrap()
        .events
        .push(event(value));
}

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(GestureSystem::default());
    world
}

#[test]
fn subtree_retirement_discards_unread_and_write_events_but_not_later_input() {
    let mut world = world();
    let root = world.spawn_empty();
    let child = world.spawn_empty();
    let other = world.spawn_empty();
    world.insert(child, crate::ui::Parent(root));
    let tap = |target, value| GestureEvent::Tap {
        x: Fixed::from_int(value),
        y: Fixed::ZERO,
        target,
    };
    for input in [tap(other, 1), tap(child, 2), tap(root, 3), tap(other, 4)] {
        world
            .resource_mut::<GestureSystem>()
            .unwrap()
            .events
            .push(input);
    }
    let mut seen = [0; 4];
    let mut count = 0;
    assert_eq!(
        tracked_allocations(|| {
            let drained = drain_gestures(&mut world, |world, input| {
                seen[count] = value(input);
                count += 1;
                if value(input) == 1 {
                    world
                        .resource_mut::<GestureSystem>()
                        .unwrap()
                        .events
                        .push(tap(child, 5));
                    crate::input::event::retire_subtree_interaction(world, root);
                    world
                        .resource_mut::<GestureSystem>()
                        .unwrap()
                        .events
                        .push(tap(child, 6));
                }
                Ok::<(), ()>(())
            });
            assert_eq!(&seen[..count], &[1, 4]);
            assert_eq!(drained, Ok(2));
            assert_eq!(
                drain_gestures(&mut world, |_, input| {
                    assert_eq!(value(input), 6);
                    Ok::<(), ()>(())
                }),
                Ok(1)
            );
        }),
        0
    );
}

#[crate::event]
fn empty_cell(_event: event!(crate::ui::widgets::lazy_list::CreateCell)) -> Entity {
    crate::ui! {
        View ()
    }
}

#[test]
fn recycling_a_cell_during_dispatch_cancels_its_unread_gesture() {
    use crate::prelude::*;
    use crate::ui::widgets::lazy_list::{LazyList, reconcile};

    fn viewport(world: &mut World, list: Entity, offset: i32) {
        world.insert(
            list,
            crate::ui::ComputedRect(crate::types::Rect::new(0, 0, 200, 80)),
        );
        world
            .get_mut::<crate::input::event::scroll::ScrollOffset>(list)
            .unwrap()
            .y = offset.into();
        reconcile(world);
    }

    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let list = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(100, 40));
        tree.on_event(list, LazyList::CreateCell, empty_cell::callback());
        list
    });
    viewport(&mut app.world, list, 0);
    let cell = app.world.get::<crate::ui::Children>(list).unwrap().0[0];
    assert_eq!(
        app.world.get::<Style>(cell).unwrap().layout.top,
        Dimension::px(0)
    );
    app.world.insert_resource(GestureSystem::default());
    for target in [list, cell] {
        app.world
            .resource_mut::<GestureSystem>()
            .unwrap()
            .events
            .push(GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target,
            });
    }
    let result = drain_gestures(&mut app.world, |world, event| {
        assert_eq!(event.target(), list, "old cell input reached a rebound row");
        viewport(world, list, 800);
        assert_ne!(
            world.get::<Style>(cell).unwrap().layout.top,
            Dimension::px(0)
        );
        Ok::<(), ()>(())
    });
    assert_eq!(result, Ok(1));
}

#[test]
fn batch_swap_reuses_storage_and_defers_callback_events_without_allocation() {
    let mut world = world();
    let system = world.resource::<GestureSystem>().unwrap();
    let writing = system.events.buffer.as_ptr();
    let reading = system.pending.borrow().buffer.as_ptr();
    for value in [1, 2, 3] {
        enqueue(&mut world, value);
    }
    let mut seen = [0; 3];
    let mut index = 0;
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |world, input| {
                    seen[index] = value(input);
                    index += 1;
                    if index == 1 {
                        enqueue(world, 4);
                    }
                    Ok::<(), ()>(())
                }),
                Ok(3)
            );
        }),
        0
    );
    assert_eq!(seen, [1, 2, 3]);
    let system = world.resource::<GestureSystem>().unwrap();
    assert_eq!(system.events.buffer.as_ptr(), reading);
    assert_eq!(system.pending.borrow().buffer.as_ptr(), writing);
    assert_eq!(system.events.buffer.len(), 1);
    assert!(system.pending.borrow().buffer.is_empty());
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |_, input| {
                    assert_eq!(value(input), 4);
                    Ok::<(), ()>(())
                }),
                Ok(1)
            );
            for next in 5..13 {
                enqueue(&mut world, next);
                assert_eq!(
                    drain_gestures(&mut world, |_, input| {
                        assert_eq!(value(input), next);
                        Ok::<(), ()>(())
                    }),
                    Ok(1)
                );
            }
        }),
        0
    );
    let system = world.resource::<GestureSystem>().unwrap();
    assert_eq!(system.events.buffer.as_ptr(), writing);
    assert_eq!(system.pending.borrow().buffer.as_ptr(), reading);
}

#[test]
fn recursive_drain_rejects_before_swapping_or_consuming_the_write_buffer() {
    let mut world = world();
    enqueue(&mut world, 1);
    enqueue(&mut world, 2);
    let mut seen = [0; 2];
    let mut index = 0;
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |world, input| {
                    seen[index] = value(input);
                    index += 1;
                    if index == 1 {
                        enqueue(world, 3);
                        assert_eq!(
                            drain_gestures(world, |_, _| -> Result<(), ()> {
                                panic!("recursive callback ran")
                            }),
                            Err(GestureDrainError::Busy)
                        );
                    }
                    Ok::<(), ()>(())
                }),
                Ok(2)
            );
            assert_eq!(
                drain_gestures(&mut world, |_, input| {
                    assert_eq!(value(input), 3);
                    Ok::<(), ()>(())
                }),
                Ok(1)
            );
        }),
        0
    );
    assert_eq!(seen, [1, 2]);
}

#[test]
fn unwinding_discards_unread_batch_but_preserves_write_events_and_read_capacity() {
    let mut world = world();
    for value in [1, 2, 3] {
        enqueue(&mut world, value);
    }
    let writing = world
        .resource::<GestureSystem>()
        .unwrap()
        .events
        .buffer
        .as_ptr();
    let mut called = 0;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = drain_gestures(&mut world, |world, input| -> Result<(), ()> {
            called += 1;
            assert_eq!(value(input), 1);
            enqueue(world, 4);
            panic!("gesture callback unwind");
        });
    }));
    assert!(result.is_err());
    assert_eq!(called, 1);
    let system = world.resource::<GestureSystem>().unwrap();
    assert!(system.pending.borrow().buffer.is_empty());
    assert_eq!(system.pending.borrow().buffer.as_ptr(), writing);
    assert_eq!(system.pending.borrow().buffer.capacity(), 4);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |_, input| {
                    assert_eq!(value(input), 4);
                    Ok::<(), ()>(())
                }),
                Ok(1)
            );
        }),
        0
    );
}

#[test]
fn removing_the_resource_does_not_invalidate_the_active_batch() {
    let mut world = world();
    for value in [1, 2, 3] {
        enqueue(&mut world, value);
    }
    let mut seen = [0; 3];
    let mut index = 0;
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |world, input| {
                    seen[index] = value(input);
                    index += 1;
                    if index == 1 {
                        world.remove_resource::<GestureSystem>().unwrap();
                    }
                    Ok::<(), ()>(())
                }),
                Ok(3)
            );
            assert_eq!(
                drain_gestures(&mut world, |_, _| -> Result<(), ()> {
                    panic!("absent resource dispatched")
                }),
                Ok(0)
            );
        }),
        0
    );
    assert_eq!(seen, [1, 2, 3]);
}

#[test]
fn replacement_keeps_its_events_separate_from_the_active_batch() {
    let mut world = world();
    for value in [1, 2, 3] {
        enqueue(&mut world, value);
    }
    let mut replacement = GestureSystem::default();
    replacement.events.push(event(4));
    let mut replacement = Some(replacement);
    let mut seen = [0; 3];
    let mut index = 0;
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |world, input| {
                    seen[index] = value(input);
                    index += 1;
                    if index == 1 {
                        world.insert_resource(replacement.take().unwrap());
                    }
                    Ok::<(), ()>(())
                }),
                Ok(3)
            );
            assert_eq!(
                drain_gestures(&mut world, |_, input| {
                    assert_eq!(value(input), 4);
                    Ok::<(), ()>(())
                }),
                Ok(1)
            );
        }),
        0
    );
    assert_eq!(seen, [1, 2, 3]);
}

#[test]
fn dispatch_failure_clears_the_unread_batch_and_retains_new_events_and_capacity() {
    let mut world = world();
    for value in [1, 2, 3] {
        enqueue(&mut world, value);
    }
    let writing = world
        .resource::<GestureSystem>()
        .unwrap()
        .events
        .buffer
        .as_ptr();
    let mut calls = 0;
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |world, input| {
                    calls += 1;
                    assert_eq!(value(input), 1);
                    enqueue(world, 4);
                    Err("dispatch failed")
                }),
                Err(GestureDrainError::Dispatch("dispatch failed"))
            );
        }),
        0
    );
    assert_eq!(calls, 1);
    let system = world.resource::<GestureSystem>().unwrap();
    assert!(system.pending.borrow().buffer.is_empty());
    assert_eq!(system.pending.borrow().buffer.as_ptr(), writing);
    assert_eq!(system.pending.borrow().buffer.capacity(), 4);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |_, input| {
                    assert_eq!(value(input), 4);
                    Ok::<(), ()>(())
                }),
                Ok(1)
            );
        }),
        0
    );
}
