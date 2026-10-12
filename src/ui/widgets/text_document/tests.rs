use super::*;
use crate::app::App;
use crate::core::reactive::{Computed, with_world_scope};
use crate::test_support::tracked_allocations;

#[test]
fn range_edits_validate_before_mutation_and_publish_only_content_changes() {
    let mut document = TextDocument::new();
    document.set_text("A中🙂Z").unwrap();
    let before = document.clone();
    for range in [0..2, 2..8, 1..10, Range { start: 8, end: 1 }] {
        assert!(document.replace(range, "x").is_err());
        assert_eq!(document.buffer, before.buffer);
        assert_eq!(document.len, before.len);
        assert_eq!(document.revision(), before.revision());
    }
    document.replace(1..8, "中🙂").unwrap();
    assert_eq!(document.revision(), 1);
    document.replace(1..8, "é").unwrap();
    document.read(|text| assert_eq!(text, "AéZ"));
    assert_eq!(document.revision(), 2);
    document.replace(1..3, "").unwrap();
    document.read(|text| assert_eq!(text, "AZ"));
}

#[test]
fn document_does_not_impose_an_editor_line_policy() {
    let mut document = TextDocument::new();
    document.set_text("first\nsecond\tcolumn").unwrap();
    document.read(|text| assert_eq!(text, "first\nsecond\tcolumn"));
}

#[test]
fn capacity_and_revision_failures_preserve_document_state() {
    let mut document = TextDocument::new();
    document.set_text("中").unwrap();
    for capacity in [0, 2, usize::MAX] {
        assert!(document.set_capacity(capacity).is_err());
        assert_eq!(document.capacity(), INLINE_CAPACITY);
        assert_eq!(document.revision(), 1);
        document.read(|text| assert_eq!(text, "中"));
    }
    document.set_capacity(3).unwrap();
    assert_eq!(document.revision(), 1);
    assert!(document.replace(3..3, "a").is_err());
    document.revision = u64::MAX;
    assert_eq!(
        document.set_text("a"),
        Err(TextEditError::RevisionExhausted)
    );
    assert_eq!(document.set_text("中"), Ok(()));
    document.read(|text| assert_eq!(text, "中"));
}

#[test]
fn prepared_document_edits_and_capacity_reuse_do_not_allocate() {
    let mut document = TextDocument::new();
    document.set_capacity(512).unwrap();
    let text = "中".repeat(100);
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..4 {
                document.set_text(&text).unwrap();
                document.replace(0..3, "🙂").unwrap();
                document.set_text("").unwrap();
                document.set_capacity(0).unwrap();
                assert!(document.set_text("a").is_err());
                document.set_capacity(512).unwrap();
            }
        }),
        0
    );
}

#[crate::model]
struct Workspace {
    document: model!(owned TextDocument),
}

#[crate::model]
impl Workspace {
    #[observe]
    fn document_revision(&self) -> u64 {
        self.document.revision()
    }
}

#[test]
fn registered_nested_document_reuses_identity_and_publishes_parent_observations() {
    let mut app = App::headless(32, 32);
    let workspace = app
        .add_model(Workspace {
            document: TextDocument::new().into(),
        })
        .finish();
    let first = workspace.document();
    let second = workspace.document();
    let parent = workspace.clone();
    let revision = with_world_scope(&mut app.world, || {
        Computed::new(move || parent.document_revision())
    });
    assert_eq!(revision.get(), 0);
    first.set_text("hello").unwrap();
    second.read(|text| assert_eq!(text, "hello"));
    assert_eq!(revision.get(), 1);
    assert_eq!(second.replace(0..5, "hello"), Ok(()));
    assert_eq!(revision.get(), 1);
    second.set_capacity(3).unwrap_err();
    assert_eq!(revision.get(), 1);
    assert_eq!(
        tracked_allocations(|| {
            first.replace(0..5, "你好").unwrap();
            second.read(|text| assert_eq!(text, "你好"));
            assert_eq!(revision.get(), 2);
        }),
        0
    );
}

#[test]
fn registered_capacity_is_observable_without_changing_content_revision() {
    let mut app = App::headless(32, 32);
    let document = app.add_model(TextDocument::new()).finish();
    let source = document.clone();
    let capacity = with_world_scope(&mut app.world, || Computed::new(move || source.capacity()));
    assert_eq!(capacity.get(), INLINE_CAPACITY);
    document.set_capacity(12).unwrap();
    assert_eq!(capacity.get(), 12);
    assert_eq!(document.revision(), 0);
    document.set_text("12345").unwrap();
    assert!(document.set_capacity(4).is_err());
    assert_eq!(capacity.get(), 12);
    assert_eq!(document.revision(), 1);
}
