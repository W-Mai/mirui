use super::*;

fn cut(app: &mut TestApp, node: Entity) {
    let mut queue =
        crate::ecs::commands::CommandQueue::bounded(CommandCapacity::new().clipboard(1)).unwrap();
    let mut batch = queue.begin();
    batch.commands().clipboard().cut(node);
    batch.finish(&mut app.world).unwrap();
}

fn body(app: &TestApp, node: Entity, expected: &str) {
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, expected));
}

fn result(app: &mut TestApp) -> Result<ClipboardOutcome, ClipboardError> {
    flush(app);
    app.world
        .resource::<Results>()
        .unwrap()
        .0
        .last()
        .unwrap()
        .result
}

#[test]
fn cut_waits_for_acknowledgment_then_deletes_only_the_captured_utf8_selection() {
    let (mut app, node) = fixture();
    select(&mut app, node, 8, 1);
    cut(&mut app, node);
    let write = take_write(&mut app.world).unwrap();
    assert_eq!(write.data().plain_text(), Ok("中🙂"));
    body(&app, node, "A中🙂Z");
    let ack = write.complete(Ok(()));
    complete(&mut app.world, &ack);
    body(&app, node, "AZ");
    assert_eq!(
        app.world.get::<TextEditing>(node).unwrap().selection(),
        TextSelection {
            anchor: 1,
            cursor: 1
        }
    );
    assert_eq!(result(&mut app), Ok(ClipboardOutcome::Cut));
    select(&mut app, node, 0, 2);
    complete(&mut app.world, &ack);
    body(&app, node, "AZ");
    assert_eq!(app.world.resource::<Results>().unwrap().0.len(), 1);
}

#[test]
fn failed_writes_preserve_body_selection_and_preedit() {
    let (mut app, node) = fixture();
    select(&mut app, node, 1, 4);
    app.world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .update_preedit("ni", None)
        .unwrap();
    cut(&mut app, node);
    let write = take_write(&mut app.world).unwrap();
    complete(
        &mut app.world,
        &write.complete(Err(ClipboardError::PermissionDenied)),
    );
    body(&app, node, "A中🙂Z");
    let input = app.world.get::<TextEditing>(node).unwrap();
    assert_eq!(
        input.selection(),
        TextSelection {
            anchor: 1,
            cursor: 4
        }
    );
    input.read_display(|text| assert_eq!(text, "Ani🙂Z"));
    assert_eq!(result(&mut app), Err(ClipboardError::PermissionDenied));
}

#[test]
fn changed_and_restored_selection_or_composition_never_deletes_current_content() {
    for composition in [false, true] {
        let (mut app, node) = fixture();
        select(&mut app, node, 1, 4);
        cut(&mut app, node);
        let write = take_write(&mut app.world).unwrap();
        if composition {
            let editor = app.world.get_mut::<TextEditing>(node).unwrap();
            editor.update_preedit("ni", None).unwrap();
            editor.clear_preedit();
        } else {
            select(&mut app, node, 0, 0);
            select(&mut app, node, 1, 4);
        }
        complete(&mut app.world, &write.complete(Ok(())));
        body(&app, node, "A中🙂Z");
        assert_eq!(result(&mut app), Ok(ClipboardOutcome::CopiedWithoutCut));
    }
}

#[crate::event]
fn replacement_cut(_request: event!(CutSelection)) -> Result<(), ClipboardError> {
    panic!("a replacement provider must not receive an earlier cut");
}

#[test]
fn replaced_editor_or_provider_does_not_receive_the_pending_cut() {
    for provider in [false, true] {
        let (mut app, node) = fixture();
        select(&mut app, node, 0, 1);
        cut(&mut app, node);
        let write = take_write(&mut app.world).unwrap();
        if provider {
            request::install(
                &mut app.world,
                node,
                replacement_cut::callback::<crate::input::event::callback::NodeContext>(),
            )
            .unwrap();
        } else {
            app.world
                .insert(node, TextEditing::default().with_text("new").unwrap());
        }
        complete(&mut app.world, &write.complete(Ok(())));
        body(&app, node, if provider { "A中🙂Z" } else { "new" });
        assert_eq!(result(&mut app), Ok(ClipboardOutcome::CopiedWithoutCut));
    }
}

#[crate::event]
fn rejected_cut(
    node: entity!(),
    _request: event!(CutSelection),
    commands: commands!(),
) -> Result<(), ClipboardError> {
    commands.apply_event::<CutSelection>(node, |_, _, _| {
        Err(crate::ecs::CommandError::Commit(
            crate::ecs::CommandCommitError::OperationRejected {
                operation: "cut selection",
            },
        ))
    });
    Ok(())
}

#[test]
fn deferred_rejection_is_not_hidden_by_the_callback_return_value() {
    let (mut app, node) = fixture();
    request::install(
        &mut app.world,
        node,
        rejected_cut::callback::<crate::input::event::callback::NodeContext>(),
    )
    .unwrap();
    select(&mut app, node, 0, 1);
    cut(&mut app, node);
    let write = take_write(&mut app.world).unwrap();
    complete(&mut app.world, &write.complete(Ok(())));
    body(&app, node, "A中🙂Z");
    assert_eq!(result(&mut app), Ok(ClipboardOutcome::CopiedWithoutCut));
}

#[test]
fn empty_cut_does_not_write_or_change_the_document() {
    let (mut app, node) = fixture();
    cut(&mut app, node);
    assert!(take_write(&mut app.world).is_none());
    body(&app, node, "A中🙂Z");
    assert_eq!(result(&mut app), Ok(ClipboardOutcome::EmptySelection));
}
