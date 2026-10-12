use super::*;
use crate::app::App;
use crate::test_support::tracked_allocations;
use crate::ui::widgets::TextSelection;

#[test]
fn edit_target_separates_instance_identity_from_content_and_selection_versions() {
    let mut app = App::headless(32, 32);
    app.with_widget(super::super::view());
    let node = app.spawn_root().id();
    let document = app.add_model(TextDocument::new());
    document.set_text("A中").unwrap();
    app.world.insert(node, TextEditing::new(document.clone()));
    crate::ui::lifecycle::prepare(&mut app.world);
    assert_eq!(
        tracked_allocations(|| {
            let target = EditTarget::capture(&app.world, node).unwrap();
            assert!(target.after_edit(&app.world, false).is_some());
            document.set_text("A中").unwrap();
            assert!(document.replace(2..3, "x").is_err());
            assert!(target.after_edit(&app.world, false).is_some());
            document.set_text("new").unwrap();
            assert!(target.identity.resolve(&app.world).is_some());
            assert!(target.after_edit(&app.world, false).is_none());
            assert!(target.after_edit(&app.world, true).is_some());
            app.world
                .get_mut::<TextEditing>(node)
                .unwrap()
                .set_selection(TextSelection {
                    anchor: 0,
                    cursor: 1,
                })
                .unwrap();
            assert!(target.identity.resolve(&app.world).is_some());
            assert!(target.after_edit(&app.world, true).is_none());
        }),
        0
    );

    let target = EditTarget::capture(&app.world, node).unwrap();
    app.world.insert(node, TextEditing::new(document));
    assert!(target.identity.resolve(&app.world).is_none());
    assert!(target.after_edit(&app.world, false).is_none());
}

#[test]
fn edit_target_rejects_other_worlds_and_replaced_entity_generations() {
    let mut world = World::new();
    let node = world.spawn_empty();
    world.insert(node, TextEditing::default());
    let target = EditTarget::capture(&world, node).unwrap();
    let mut other = World::new();
    let other_node = other.spawn_empty();
    assert_eq!(node, other_node);
    other.insert(other_node, TextEditing::default());
    assert!(target.after_edit(&other, false).is_none());
    assert!(target.identity.resolve(&other).is_none());
    world.despawn(node);
    let replacement = world.spawn_empty();
    world.insert(replacement, TextEditing::default());
    assert!(target.identity.resolve(&world).is_none());
    assert!(target.after_edit(&world, false).is_none());
}

#[test]
fn retained_rebinding_invalidates_old_edits_without_replacing_the_component() {
    use crate::core::reactive::retained::{RetainedEffects, retire};
    let mut world = World::new();
    let node = world.spawn_empty();
    world.insert(node, TextEditing::default());
    let gate = RetainedEffects::new(&mut world, |_, _| true);
    world.insert(node, gate);
    let target = EditTarget::capture(&world, node).unwrap();
    retire(&mut world, node);
    assert!(target.identity.resolve(&world).is_none());
    assert!(target.after_edit(&world, false).is_none());
    assert!(EditTarget::capture(&world, node).is_some());
    world.get_mut::<RetainedEffects>(node).unwrap().active = false;
    assert!(EditTarget::capture(&world, node).is_none());
}

#[test]
fn document_rebinding_invalidates_identity_even_when_edit_versions_match() {
    let mut app = App::headless(32, 32);
    let first = app.add_model(TextDocument::new());
    let second = app.add_model(TextDocument::new());
    first.set_text("first").unwrap();
    second.set_text("other").unwrap();
    let node = app.spawn_root().id();
    app.world.insert(node, TextEditing::new(first));
    let target = EditTarget::capture(&app.world, node).unwrap();
    app.world.get_mut::<TextEditing>(node).unwrap().document = Some(second);
    assert!(target.identity.resolve(&app.world).is_none());
    assert!(target.after_edit(&app.world, false).is_none());
}
