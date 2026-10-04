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
fn batch_swap_reuses_storage_and_defers_callback_events_without_allocation() {
    let mut world = world();
    let system = world.resource::<GestureSystem>().unwrap();
    let writing = system.events.buffer.as_ptr();
    let reading = system.pending.borrow().as_ptr();
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
                }),
                Ok(3)
            );
        }),
        0
    );
    assert_eq!(seen, [1, 2, 3]);
    let system = world.resource::<GestureSystem>().unwrap();
    assert_eq!(system.events.buffer.as_ptr(), reading);
    assert_eq!(system.pending.borrow().as_ptr(), writing);
    assert_eq!(system.events.buffer.len(), 1);
    assert!(system.pending.borrow().is_empty());
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |_, input| assert_eq!(value(input), 4)),
                Ok(1)
            );
            for next in 5..13 {
                enqueue(&mut world, next);
                assert_eq!(
                    drain_gestures(&mut world, |_, input| assert_eq!(value(input), next)),
                    Ok(1)
                );
            }
        }),
        0
    );
    let system = world.resource::<GestureSystem>().unwrap();
    assert_eq!(system.events.buffer.as_ptr(), writing);
    assert_eq!(system.pending.borrow().as_ptr(), reading);
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
                            drain_gestures(world, |_, _| panic!("recursive callback ran")),
                            Err(GestureDrainError::Busy)
                        );
                    }
                }),
                Ok(2)
            );
            assert_eq!(
                drain_gestures(&mut world, |_, input| assert_eq!(value(input), 3)),
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
        let _ = drain_gestures(&mut world, |world, input| {
            called += 1;
            assert_eq!(value(input), 1);
            enqueue(world, 4);
            panic!("gesture callback unwind");
        });
    }));
    assert!(result.is_err());
    assert_eq!(called, 1);
    let system = world.resource::<GestureSystem>().unwrap();
    assert!(system.pending.borrow().is_empty());
    assert_eq!(system.pending.borrow().as_ptr(), writing);
    assert_eq!(system.pending.borrow().capacity(), 4);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                drain_gestures(&mut world, |_, input| assert_eq!(value(input), 4)),
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
                }),
                Ok(3)
            );
            assert_eq!(
                drain_gestures(&mut world, |_, _| panic!("absent resource dispatched")),
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
                }),
                Ok(3)
            );
            assert_eq!(
                drain_gestures(&mut world, |_, input| assert_eq!(value(input), 4)),
                Ok(1)
            );
        }),
        0
    );
    assert_eq!(seen, [1, 2, 3]);
}
