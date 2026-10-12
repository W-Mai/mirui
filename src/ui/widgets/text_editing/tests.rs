use super::*;
use crate::app::App;
use crate::test_support::tracked_allocations;
use crate::ui::{dirty::Dirty, view::ViewRegistry};

#[test]
fn selection_requests_reject_foreign_and_expired_document_bindings() {
    use crate::input::event::{callback::NodeContext, request};
    use crate::ui::selection::{RequestError, SelectAll};
    let mut owner = App::headless(32, 32);
    let document = owner.add_model(TextDocument::new()).finish();
    document.set_text("old").unwrap();
    let mut world = crate::ecs::World::new();
    let node = world.spawn(TextEditing::new(document));
    request::install(
        &mut world,
        node,
        input::select_all::callback::<NodeContext>(),
    )
    .unwrap();
    assert_eq!(
        request::invoke(&mut world, node, &SelectAll),
        Ok(Err(RequestError::Unavailable))
    );
    drop(owner);
    assert_eq!(
        request::invoke(&mut world, node, &SelectAll),
        Ok(Err(RequestError::Unavailable))
    );
}

#[test]
fn default_inline_document_and_selection_require_no_allocation() {
    assert_eq!(
        tracked_allocations(|| {
            let mut editor = TextEditing::default().with_text("A中🙂").unwrap();
            assert_eq!(editor.capacity(), 32);
            assert_eq!(editor.len(), 8);
            assert!(!editor.is_empty());
            editor.read(|text| assert_eq!(text, "A中🙂"));
            assert_eq!(editor.cursor(), 8);
            editor
                .set_selection(TextSelection {
                    anchor: 8,
                    cursor: 1,
                })
                .unwrap();
            assert_eq!(editor.selection().range(), 1..8);
            assert!(editor.document.is_none());
        }),
        0
    );
}

#[test]
fn shared_content_keeps_selections_local_and_resolves_utf8_boundaries() {
    let mut app = App::headless(32, 32);
    let document = app.add_model(TextDocument::new()).finish();
    document.set_text("abcd🙂").unwrap();
    let mut first = TextEditing::new(document.clone());
    let mut second = TextEditing::new(document.clone());
    first
        .set_selection(TextSelection {
            anchor: 8,
            cursor: 3,
        })
        .unwrap();
    second
        .set_selection(TextSelection {
            anchor: 1,
            cursor: 2,
        })
        .unwrap();
    assert!(first.local.is_none() && second.local.is_none());
    let revision = document.revision();
    document.set_text("中!").unwrap();
    assert_eq!(document.revision(), revision + 1);
    assert_eq!(
        first.selection(),
        TextSelection {
            anchor: 4,
            cursor: 3
        }
    );
    assert_eq!(
        second.selection(),
        TextSelection {
            anchor: 0,
            cursor: 0
        }
    );
    first
        .set_selection(TextSelection {
            anchor: 3,
            cursor: 4,
        })
        .unwrap();
    assert_eq!(second.cursor(), 0);
    first.read(|text| assert_eq!(text, "中!"));
    second.read(|text| assert_eq!(text, "中!"));
    assert_eq!(document.revision(), revision + 1);
}

#[test]
fn invalid_and_unchanged_selections_preserve_revision_and_endpoints() {
    let mut editor = TextEditing::default().with_text("中").unwrap();
    let before = editor.selection;
    let revision = editor.selection_revision;
    for offset in [1, 2, 4, usize::MAX] {
        assert!(
            editor
                .set_selection(TextSelection {
                    anchor: offset,
                    cursor: 0
                })
                .is_err()
        );
        assert_eq!(editor.selection, before);
        assert_eq!(editor.selection_revision, revision);
    }
    assert_eq!(editor.set_selection(before), Ok(false));
    assert_eq!(editor.selection_revision, revision);
    editor.selection_revision = u64::MAX;
    assert_eq!(
        editor.set_selection(TextSelection::default()),
        Err(TextEditError::RevisionExhausted)
    );
    assert_eq!(editor.selection, before);
}

#[crate::model]
struct Form {
    document: model!(owned TextDocument),
}

#[crate::model]
impl Form {
    fn clear(&mut self) {
        self.document.set_text("").unwrap();
    }
}

#[test]
fn owned_document_projection_uses_the_registered_source_without_copying_content() {
    let mut app = App::headless(32, 32);
    let form = app
        .add_model(Form {
            document: TextDocument::new().into(),
        })
        .finish();
    let editor = TextEditing::new(form.document())
        .with_capacity(128)
        .unwrap()
        .with_text("nested")
        .unwrap();
    assert_eq!(form.document().capacity(), 128);
    assert_eq!(editor.cursor(), 6);
    assert!(editor.local.is_none());
    form.clear();
    assert!(editor.is_empty());
    assert_eq!(editor.selection(), TextSelection::default());
}

#[test]
fn shared_document_observation_invalidates_layout_without_painting_and_rebinds_cleanly() {
    let mut app = App::headless(32, 32);
    let first = app.add_model(TextDocument::new()).finish();
    let second = app.add_model(TextDocument::new()).finish();
    let descriptor = view();
    assert!(descriptor.priority().is_none());
    app.with_widget(descriptor);
    let root = app.spawn_root().id();
    app.world.insert(root, TextEditing::new(first.clone()));
    app.render_dirty().unwrap();
    assert!(!app.world.has::<Dirty>(root));
    first.set_text("first").unwrap();
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert!(app.world.has::<Dirty>(root));
    app.render_dirty().unwrap();
    app.world.insert(root, TextEditing::new(second.clone()));
    app.render_dirty().unwrap();
    first.set_text("old").unwrap();
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert!(!app.world.has::<Dirty>(root));
    second.set_capacity(64).unwrap();
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert!(app.world.has::<Dirty>(root));
    app.world.insert(root, TextEditing::default());
    app.render_dirty().unwrap();
    second.set_text("detached").unwrap();
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert!(!app.world.has::<Dirty>(root));
}

#[test]
fn prepared_shared_reads_selection_and_document_changes_allocate_nothing() {
    let mut app = App::headless(32, 32);
    let document = app.add_model(TextDocument::new()).finish();
    document.set_text("A中").unwrap();
    let root = app.spawn_root().id();
    app.with_widget(view());
    app.world.insert(root, TextEditing::new(document.clone()));
    ViewRegistry::reconcile_observations(&mut app.world);
    document.set_text("中A").unwrap();
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(
        tracked_allocations(|| {
            for text in ["A中", "中A", "A中"] {
                document.set_text(text).unwrap();
                let editor = app.world.get_mut::<TextEditing>(root).unwrap();
                editor.read(|actual| assert_eq!(actual, text));
                editor
                    .set_selection(TextSelection {
                        anchor: 0,
                        cursor: 4,
                    })
                    .unwrap();
                assert_eq!(editor.selection().range(), 0..4);
                crate::core::reactive::flush_signal_dirty(&mut app.world);
                ViewRegistry::reconcile_observations(&mut app.world);
            }
        }),
        0
    );
}

#[test]
fn shared_document_binding_keeps_model_owner_and_lifetime_guards() {
    let mut owner = App::headless(32, 32);
    let document = owner.add_model(TextDocument::new()).finish();
    let mut other = App::headless(32, 32);
    other.with_widget(view());
    let root = other.spawn_root().id();
    other.world.insert(root, TextEditing::new(document.clone()));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ViewRegistry::reconcile_observations(&mut other.world);
        }))
        .is_err()
    );
    drop(other);
    let editor = TextEditing::new(document);
    drop(owner);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| editor.read(|_| ()))).is_err()
    );
}
