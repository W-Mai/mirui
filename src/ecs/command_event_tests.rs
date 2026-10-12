use super::*;
use alloc::string::String;

#[derive(Default)]
struct Observed {
    pointer: usize,
    len: usize,
    calls: usize,
}

#[expect(
    clippy::ptr_arg,
    reason = "The callback must match the owned String event type."
)]
fn observe(world: &mut World, _: Entity, event: &String) -> Result<(), CommandError> {
    let observed = world.resource_mut::<Observed>().unwrap();
    observed.pointer = event.as_ptr() as usize;
    observed.len = event.len();
    observed.calls += 1;
    Ok(())
}

fn fixture() -> (World, Entity, CommandQueue) {
    let mut world = World::new();
    world.insert_resource(Observed::default());
    let entity = world.spawn_empty();
    let queue = CommandQueue::bounded(CommandCapacity::new().entity_value::<()>(3)).unwrap();
    (world, entity, queue)
}

#[test]
fn event_is_borrowed_until_commit_without_copying_or_allocating() {
    let (mut world, entity, mut queue) = fixture();
    let event = String::from("整段文本🙂");
    let allocations = crate::test_support::tracked_allocations(|| {
        let mut batch = queue.begin();
        batch.commands().apply_event(entity, observe);
        batch.finish_with_event(&mut world, &event).unwrap();
    });
    let observed = world.resource::<Observed>().unwrap();
    assert_eq!(
        (observed.pointer, observed.len),
        (event.as_ptr() as usize, event.len())
    );
    assert_eq!(allocations, 0);
    assert_eq!(observed.calls, 1);
    assert_eq!(
        core::mem::size_of::<EventOperation<String>>(),
        core::mem::size_of::<usize>()
    );
}

#[test]
fn missing_or_wrong_context_discards_remaining_commands_and_does_not_replay() {
    for wrong_type in [false, true] {
        let (mut world, entity, mut queue) = fixture();
        let mut batch = queue.begin();
        batch.commands().apply_event(entity, observe);
        batch.commands().apply(entity, (), |world, _, ()| {
            world.resource_mut::<Observed>().unwrap().calls += 10;
            Ok(())
        });
        let result = if wrong_type {
            batch.finish_with_event(&mut world, &123_u32)
        } else {
            batch.finish(&mut world)
        };
        assert_eq!(result, Err(CommandError::EventContext));
        queue.begin().finish(&mut world).unwrap();
        assert_eq!(world.resource::<Observed>().unwrap().calls, 0);
        let mut batch = queue.begin();
        batch.commands().apply_event(entity, observe);
        batch
            .finish_with_event(&mut world, &String::from("new"))
            .unwrap();
        assert_eq!(world.resource::<Observed>().unwrap().calls, 1);
    }
}

#[test]
fn owned_and_borrowed_commands_share_fifo_and_keep_completed_work_on_error() {
    let (mut world, entity, mut queue) = fixture();
    let mut batch = queue.begin();
    batch.commands().apply(entity, (), |world, _, ()| {
        world.resource_mut::<Observed>().unwrap().calls = 10;
        Ok(())
    });
    batch.commands().apply_event(entity, observe);
    batch
        .commands()
        .apply_event::<u32>(entity, |_, _, _| Ok(()));
    assert_eq!(
        batch.finish_with_event(&mut world, &String::from("ok")),
        Err(CommandError::EventContext)
    );
    assert_eq!(world.resource::<Observed>().unwrap().calls, 11);
    queue.begin().finish(&mut world).unwrap();
    assert_eq!(world.resource::<Observed>().unwrap().calls, 11);
}

#[test]
fn nested_commit_keeps_each_event_on_its_own_stack() {
    let (mut world, entity, mut queue) = fixture();
    let mut batch = queue.begin();
    batch
        .commands()
        .apply_event::<String>(entity, |world, node, outer| {
            let inner = String::from("inner");
            let mut queue = CommandQueue::bounded(CommandCapacity::new().entity_value::<()>(1))?;
            let mut batch = queue.begin();
            batch.commands().apply_event(node, observe);
            batch.finish_with_event(world, &inner)?;
            assert_eq!(
                world.resource::<Observed>().unwrap().pointer,
                inner.as_ptr() as usize
            );
            observe(world, node, outer)
        });
    let outer = String::from("outer");
    batch.finish_with_event(&mut world, &outer).unwrap();
    assert_eq!(
        world.resource::<Observed>().unwrap().pointer,
        outer.as_ptr() as usize
    );
    assert_eq!(world.resource::<Observed>().unwrap().calls, 2);
}

#[test]
fn panic_discards_the_batch_without_owning_or_dropping_the_event() {
    let (mut world, entity, mut queue) = fixture();
    let event = String::from("retained");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut batch = queue.begin();
        batch
            .commands()
            .apply_event::<String>(entity, |_, _, _| panic!("callback panic"));
        batch.commands().apply_event(entity, observe);
        batch.finish_with_event(&mut world, &event).unwrap();
    }));
    assert!(result.is_err());
    assert_eq!(event, "retained");
    queue.begin().finish(&mut world).unwrap();
    assert_eq!(world.resource::<Observed>().unwrap().calls, 0);
}
