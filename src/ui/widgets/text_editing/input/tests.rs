use super::*;
use crate::app::App;
use crate::test_support::tracked_allocations;
use crate::ui::widgets::TextDocument;

type TestApp =
    App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;

fn fixture() -> (TestApp, Entity) {
    let mut app = App::headless(32, 32);
    app.with_widget(super::super::view());
    let root = app.spawn_root().id();
    app.world.insert(root, TextEditing::default());
    crate::input::event::widget_input::reconcile(&mut app.world);
    (app, root)
}

#[test]
fn local_and_shared_inputs_apply_the_same_unicode_editing_rules() {
    for shared in [false, true] {
        let (mut app, node) = fixture();
        if shared {
            let document = app.add_model(TextDocument::new());
            app.world.insert(node, TextEditing::new(document));
        }
        apply(&mut app.world, node, Edit::Replace("A中🙂Z")).unwrap();
        app.world
            .get_mut::<TextEditing>(node)
            .unwrap()
            .set_selection(TextSelection {
                anchor: 8,
                cursor: 1,
            })
            .unwrap();
        apply(&mut app.world, node, Edit::Replace("é")).unwrap();
        let editor = app.world.get::<TextEditing>(node).unwrap();
        editor.read(|text| assert_eq!(text, "AéZ"));
        assert_eq!(editor.cursor(), 3);
        apply(&mut app.world, node, Edit::Navigate(Navigation::Left, true)).unwrap();
        assert_eq!(
            app.world.get::<TextEditing>(node).unwrap().selection(),
            TextSelection {
                anchor: 3,
                cursor: 1
            }
        );
        apply(&mut app.world, node, Edit::Backspace).unwrap();
        apply(&mut app.world, node, Edit::Delete).unwrap();
        app.world
            .get::<TextEditing>(node)
            .unwrap()
            .read(|text| assert_eq!(text, "A"));
    }
}

#[test]
fn invalid_replacements_leave_content_selection_and_revision_unchanged() {
    let (mut app, node) = fixture();
    app.world.insert(
        node,
        TextEditing::default()
            .with_capacity(4)
            .unwrap()
            .with_text("中")
            .unwrap(),
    );
    for text in ["🙂", "\n", "\0"] {
        apply(&mut app.world, node, Edit::Replace(text)).unwrap();
        let editor = app.world.get::<TextEditing>(node).unwrap();
        editor.read(|text| assert_eq!(text, "中"));
        assert_eq!(editor.cursor(), 3);
    }
    app.world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .selection_revision = u64::MAX;
    apply(&mut app.world, node, Edit::Backspace).unwrap();
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "中"));
}

#[test]
fn external_shrink_normalization_persists_before_later_growth() {
    let (mut app, node) = fixture();
    let document = app.add_model(TextDocument::new());
    app.world.insert(
        node,
        TextEditing::new(document.clone())
            .with_text("A中🙂Z")
            .unwrap(),
    );
    document.set_text("中").unwrap();
    normalize_selections::system()
        .run_once(&mut app.world)
        .unwrap();
    document.set_text("longer text").unwrap();
    assert_eq!(app.world.get::<TextEditing>(node).unwrap().cursor(), 3);
}

#[test]
fn expired_and_foreign_documents_are_not_edited() {
    let (mut app, node) = fixture();
    let mut other = App::headless(32, 32);
    let document = other.add_model(TextDocument::new());
    app.world.insert(node, TextEditing::new(document.clone()));
    apply(&mut app.world, node, Edit::Replace("foreign")).unwrap();
    assert!(document.is_empty());
    drop(other);
    apply(&mut app.world, node, Edit::Replace("expired")).unwrap();
}

#[test]
fn prepared_input_edits_reuse_component_and_document_storage() {
    for shared in [false, true] {
        let (mut app, node) = fixture();
        if shared {
            let document = app.add_model(TextDocument::new());
            app.world.insert(node, TextEditing::new(document));
        }
        apply(&mut app.world, node, Edit::Replace("中")).unwrap();
        apply(&mut app.world, node, Edit::Backspace).unwrap();
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..8 {
                    apply(&mut app.world, node, Edit::Replace("中")).unwrap();
                    apply(&mut app.world, node, Edit::Backspace).unwrap();
                }
            }),
            0
        );
    }
}

#[derive(Default)]
struct Notices(alloc::vec::Vec<&'static str>);

#[crate::event]
fn changed(_event: event!(Changed), notices: res!(mut Notices)) {
    notices.0.push("content");
}

#[crate::event]
fn selected(_event: event!(SelectionChanged), notices: res!(mut Notices)) {
    notices.0.push("selection");
}

#[crate::event]
fn submitted(_event: event!(Submitted), notices: res!(mut Notices)) {
    notices.0.push("submit");
}

#[test]
fn capability_view_accepts_fragments_and_keys_without_an_appearance_component() {
    use crate::input::event::{runtime, view_input, widget_input};
    let (mut app, node) = fixture();
    app.world
        .insert_resource(Notices(alloc::vec::Vec::with_capacity(8)));
    widget_input::attach_handlers_for(&mut app.world, node);
    assert!(app.world.has::<Focusable>(node));
    assert!(app.world.has::<TextEditable>(node));
    runtime::install_entity(
        &mut app.world,
        node,
        TextEditing::Changed,
        changed::callback::<crate::__macro_support::NodeContext>(),
    )
    .unwrap();
    runtime::install_entity(
        &mut app.world,
        node,
        TextEditing::SelectionChanged,
        selected::callback::<crate::__macro_support::NodeContext>(),
    )
    .unwrap();
    runtime::install_entity(
        &mut app.world,
        node,
        TextEditing::Submitted,
        submitted::callback::<crate::__macro_support::NodeContext>(),
    )
    .unwrap();
    assert!(
        view_input::invoke_payload(
            super::super::view(),
            &mut app.world,
            node,
            &TextCommitted::new("中🙂"),
            None
        )
        .unwrap()
    );
    runtime::flush(&mut app.world).unwrap();
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "中🙂"));
    assert_eq!(
        app.world.resource::<Notices>().unwrap().0,
        ["content", "selection"]
    );
    let enter = Key {
        code: KEY_RETURN,
        pressed: true,
        modifiers: None,
        repeat: None,
    };
    assert!(
        view_input::invoke_payload(super::super::view(), &mut app.world, node, &enter, None)
            .unwrap()
    );
    runtime::flush(&mut app.world).unwrap();
    assert_eq!(
        app.world.resource::<Notices>().unwrap().0,
        ["content", "selection", "submit"]
    );
}

#[test]
fn prepared_typed_callback_and_notifications_allocate_nothing() {
    use crate::input::event::{runtime, view_input};
    let (mut app, node) = fixture();
    app.world
        .insert_resource(Notices(alloc::vec::Vec::with_capacity(8)));
    runtime::install_entity(
        &mut app.world,
        node,
        TextEditing::Changed,
        changed::callback::<crate::__macro_support::NodeContext>(),
    )
    .unwrap();
    runtime::install_entity(
        &mut app.world,
        node,
        TextEditing::SelectionChanged,
        selected::callback::<crate::__macro_support::NodeContext>(),
    )
    .unwrap();
    let callback = committed::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut app.world, view_input::fixture_capacity())
        .unwrap();
    callback
        .invoke(&mut app.world, &TextCommitted::new("warm"), node)
        .unwrap();
    runtime::flush(&mut app.world).unwrap();
    app.world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .set_selection(TextSelection {
            anchor: 0,
            cursor: 4,
        })
        .unwrap();
    assert_eq!(
        tracked_allocations(|| {
            callback
                .invoke(&mut app.world, &TextCommitted::new("中🙂"), node)
                .unwrap();
            runtime::flush(&mut app.world).unwrap();
        }),
        0
    );
}

#[crate::model]
struct Draft {
    document: model!(owned TextDocument),
    seen: u64,
    rewrite: bool,
}

#[crate::model]
impl Draft {
    #[effects]
    fn take_revision(&mut self) -> Option<u64> {
        let revision = self.document.revision();
        if revision == self.seen {
            return None;
        }
        if self.rewrite {
            self.document.set_text("revised").unwrap();
        }
        self.seen = self.document.revision();
        Some(self.seen)
    }
}

#[test]
fn nested_effects_read_committed_content_and_do_not_receive_stale_selection() {
    use alloc::rc::Rc;
    use core::cell::Cell;
    for rewrite in [false, true] {
        let (mut app, node) = fixture();
        let draft = app.add_model(Draft {
            document: TextDocument::new().into(),
            seen: 0,
            rewrite,
        });
        let document = draft.document();
        let seen = Rc::new(Cell::new(0));
        let output = seen.clone();
        let source = document.clone();
        draft
            .on_effect(move |revision: &u64| {
                assert_eq!(source.revision(), *revision);
                source.read(|text| assert_eq!(text, if rewrite { "revised" } else { "A" }));
                output.set(*revision);
            })
            .unwrap();
        app.world.insert(node, TextEditing::new(document));
        apply(&mut app.world, node, Edit::Replace("A")).unwrap();
        assert_eq!(seen.get(), if rewrite { 2 } else { 1 });
        assert_eq!(
            app.world.get::<TextEditing>(node).unwrap().cursor(),
            if rewrite { 0 } else { 1 }
        );
    }
}
