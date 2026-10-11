use super::*;

fn selected(input: &mut TextInput, anchor: usize, cursor: usize) {
    input
        .set_selection(TextSelection { anchor, cursor })
        .unwrap();
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
