use super::*;

#[test]
fn queued_edits_apply_to_current_content_in_order_without_allocating() {
    let mut world = World::new();
    let entity = world.spawn(TextEditing::default().with_capacity(512).unwrap());
    let mut queue = crate::ecs::commands::CommandQueue::bounded(
        crate::ecs::CommandCapacity::new().entity_value::<Edit>(2),
    )
    .unwrap();
    for _ in 0..2 {
        let mut batch = queue.begin();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let mut commands = batch.commands();
                commands.apply(entity, Edit::Replace("A"), apply);
                commands.apply(entity, Edit::Replace("中"), apply);
            }),
            0
        );
        assert_eq!(
            crate::test_support::tracked_allocations(|| batch.finish(&mut world).unwrap()),
            0
        );
        let input = world.get_mut::<TextEditing>(entity).unwrap();
        input.read(|text| assert_eq!(text, "A中"));
        let len = input.len();
        input
            .set_selection(TextSelection {
                anchor: 0,
                cursor: len,
            })
            .unwrap();
        apply(&mut world, entity, Edit::Replace("")).unwrap();
    }
}

#[test]
fn queued_edits_revalidate_missing_inputs_and_capacity_at_commit() {
    let mut world = World::new();
    let entity = world.spawn(TextEditing::default());
    let mut queue = crate::ecs::commands::CommandQueue::default();
    let mut batch = queue.begin();
    batch.commands().apply(entity, Edit::Replace("中"), apply);
    world.insert(
        entity,
        TextEditing::default().with_text(&"a".repeat(32)).unwrap(),
    );
    batch.finish(&mut world).unwrap();
    assert_eq!(world.get::<TextEditing>(entity).unwrap().len(), 32);
    let mut batch = queue.begin();
    batch.commands().remove::<TextEditing>(entity);
    batch.commands().apply(entity, Edit::Replace("A"), apply);
    batch.finish(&mut world).unwrap();
    assert!(!world.has::<TextEditing>(entity));
}
