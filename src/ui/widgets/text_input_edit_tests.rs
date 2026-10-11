use super::*;

fn selected(input: &mut TextInput, anchor: usize, cursor: usize) {
    input
        .set_selection(TextSelection { anchor, cursor })
        .unwrap();
}

#[test]
fn explicit_capacity_preserves_content_and_selection_across_storage_changes() {
    let mut input = TextInput::new();
    assert_eq!(input.capacity(), TEXT_INPUT_CAP);
    input.replace_selection("A中🙂Z").unwrap();
    selected(&mut input, 8, 1);
    for capacity in [9, 64, 512, 9, 512] {
        input.set_capacity(capacity).unwrap();
        assert_eq!(input.capacity(), capacity);
        assert_eq!(input.as_str(), "A中🙂Z");
        assert_eq!(
            input.selection(),
            TextSelection {
                anchor: 8,
                cursor: 1
            }
        );
    }
    let before = input.clone();
    for capacity in [0, 8, usize::MAX] {
        assert!(input.set_capacity(capacity).is_err());
        assert_eq!(input.buffer, before.buffer);
        assert_eq!(input.capacity(), before.capacity());
        assert_eq!(input.selection(), before.selection());
    }
    let mut copy = input.clone();
    copy.replace_selection("!").unwrap();
    assert_eq!(copy.as_str(), "A!Z");
    assert_eq!(input.as_str(), "A中🙂Z");
}

#[test]
fn declared_capacity_is_enforced_independently_of_retained_storage() {
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            let mut input = TextInput::new().with_capacity(0).unwrap();
            assert!(input.replace_selection("a").is_err());
            input.set_capacity(3).unwrap();
            input.replace_selection("中").unwrap();
            assert!(!input.insert_char('a'));
            selected(&mut input, 0, 3);
            input.replace_selection("").unwrap();
            input.set_capacity(TEXT_INPUT_CAP).unwrap();
        }),
        0
    );
    let mut input = TextInput::new().with_capacity(512).unwrap();
    let text = "中".repeat(100);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            input.set_capacity(4).unwrap();
            input.replace_selection("🙂").unwrap();
            assert!(!input.insert_char('a'));
            input.set_capacity(512).unwrap();
            selected(&mut input, 4, 0);
            input.replace_selection(&text).unwrap();
            assert_eq!(input.len(), 300);
            assert_eq!(input.cursor(), 300);
            input.move_left();
            assert_eq!(input.cursor(), 297);
            input.delete_forward();
            input.backspace();
            assert_eq!(input.len(), 294);
            selected(&mut input, 0, 294);
            input.replace_selection("").unwrap();
            input.set_capacity(0).unwrap();
        }),
        0
    );
}

#[test]
fn replacement_preserves_utf8_and_collapses_directed_selections() {
    for (anchor, cursor) in [(1, 8), (8, 1)] {
        let mut input = TextInput::new();
        assert!(input.replace_selection("A中🙂Z").unwrap());
        selected(&mut input, anchor, cursor);
        assert!(input.replace_selection("你好").unwrap());
        assert_eq!(input.as_str(), "A你好Z");
        assert_eq!(input.len(), 8);
        assert_eq!(
            input.selection(),
            TextSelection {
                anchor: 7,
                cursor: 7
            }
        );
        assert!(input.delete_forward());
        assert_eq!(input.as_str(), "A你好");
        assert!(input.backspace());
        assert_eq!(input.as_str(), "A你");
    }
}

#[test]
fn invalid_selection_preserves_both_endpoints() {
    let mut input = TextInput::new();
    input.replace_selection("中🙂").unwrap();
    selected(&mut input, 7, 3);
    let before = input.selection();
    for (anchor, cursor, invalid) in [(0, 1, 1), (2, 7, 2), (0, 8, 8), (usize::MAX, 0, usize::MAX)]
    {
        assert_eq!(
            input.set_selection(TextSelection { anchor, cursor }),
            Err(TextEditError::InvalidBoundary { offset: invalid })
        );
        assert_eq!(input.selection(), before);
        assert_eq!(input.as_str(), "中🙂");
    }
}

#[test]
fn rejected_replacement_preserves_the_entire_transaction() {
    let mut input = TextInput::new();
    input.replace_selection("A中🙂Z").unwrap();
    selected(&mut input, 8, 1);
    let before = input.clone();
    let oversized = "a".repeat(TEXT_INPUT_CAP);
    for text in [oversized.as_str(), "line\nbreak", "\0", "\r", "\t"] {
        assert!(input.replace_selection(text).is_err());
        assert_eq!(input.buffer, before.buffer);
        assert_eq!(input.len(), before.len());
        assert_eq!(input.selection(), before.selection());
    }
    let exact = "a".repeat(TEXT_INPUT_CAP - 2);
    assert!(input.replace_selection(&exact).unwrap());
    assert_eq!(input.len(), TEXT_INPUT_CAP);
}

#[test]
fn unchanged_replacement_only_collapses_selection() {
    let mut input = TextInput::new();
    input.replace_selection("abc").unwrap();
    selected(&mut input, 2, 1);
    assert!(!input.replace_selection("b").unwrap());
    assert_eq!(input.as_str(), "abc");
    assert_eq!(
        input.selection(),
        TextSelection {
            anchor: 2,
            cursor: 2
        }
    );
}

#[test]
fn delete_and_navigation_respect_selection_in_both_directions() {
    for (anchor, cursor) in [(1, 8), (8, 1)] {
        let mut input = TextInput::new();
        input.replace_selection("A中🙂Z").unwrap();
        selected(&mut input, anchor, cursor);
        let mut backward = input.clone();
        assert!(backward.backspace());
        assert_eq!(backward.as_str(), "AZ");
        assert!(input.delete_forward());
        assert_eq!(input.as_str(), "AZ");
        assert_eq!(input.selection(), backward.selection());
        for (move_to, expected) in [
            (TextInput::move_left as fn(&mut TextInput), 0),
            (TextInput::move_right, 2),
            (TextInput::move_home, 0),
            (TextInput::move_end, 2),
        ] {
            selected(&mut input, 2, 0);
            move_to(&mut input);
            assert_eq!(
                input.selection(),
                TextSelection {
                    anchor: expected,
                    cursor: expected
                }
            );
        }
    }
}

#[test]
fn prepared_edits_do_not_allocate() {
    let mut input = TextInput::new();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..8 {
                input.replace_selection("中🙂").unwrap();
                let len = input.len();
                selected(&mut input, len, 0);
                input.replace_selection("hello").unwrap();
                selected(&mut input, 0, 5);
                assert!(input.backspace());
                assert!(input.is_empty());
            }
        }),
        0
    );
}

#[test]
fn queued_edits_apply_to_current_content_in_order_without_allocating() {
    let mut world = World::new();
    let entity = world.spawn(TextInput::new().with_capacity(512).unwrap());
    let mut queue = crate::ecs::commands::CommandQueue::bounded(
        crate::ecs::CommandCapacity::new().entity_value::<InputEdit>(2),
    )
    .unwrap();
    for _ in 0..2 {
        let mut batch = queue.begin();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let mut commands = batch.commands();
                commands.apply(entity, InputEdit::Character('A'), apply_input_edit);
                commands.apply(entity, InputEdit::Character('中'), apply_input_edit);
            }),
            0
        );
        assert_eq!(
            crate::test_support::tracked_allocations(|| batch.finish(&mut world).unwrap()),
            0
        );
        let input = world.get_mut::<TextInput>(entity).unwrap();
        assert_eq!(input.as_str(), "A中");
        let len = input.len();
        input
            .set_selection(TextSelection {
                anchor: 0,
                cursor: len,
            })
            .unwrap();
        input.replace_selection("").unwrap();
    }
}

#[test]
fn queued_edits_revalidate_missing_inputs_and_capacity_at_commit() {
    let mut world = World::new();
    let entity = world.spawn(TextInput::new());
    let mut queue = crate::ecs::commands::CommandQueue::default();
    let mut batch = queue.begin();
    batch
        .commands()
        .apply(entity, InputEdit::Character('中'), apply_input_edit);
    world
        .get_mut::<TextInput>(entity)
        .unwrap()
        .replace_selection(&"a".repeat(TEXT_INPUT_CAP))
        .unwrap();
    batch.finish(&mut world).unwrap();
    assert_eq!(
        world.get::<TextInput>(entity).unwrap().len(),
        TEXT_INPUT_CAP
    );
    let mut batch = queue.begin();
    batch.commands().remove::<TextInput>(entity);
    batch
        .commands()
        .apply(entity, InputEdit::Character('A'), apply_input_edit);
    batch.finish(&mut world).unwrap();
    assert!(!world.has::<TextInput>(entity));
}
