use super::*;

fn start(app: &mut TestApp, node: Entity) -> ClipboardRead {
    let mut queue =
        crate::ecs::commands::CommandQueue::bounded(CommandCapacity::new().clipboard(1)).unwrap();
    let mut batch = queue.begin();
    batch.commands().clipboard().paste(node);
    batch.finish(&mut app.world).unwrap();
    let Some(ClipboardRequest::Read(read)) = take_request(&mut app.world) else {
        panic!("expected a clipboard read");
    };
    read
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
fn paste_replaces_reversed_selection_or_inserts_at_the_caret_once() {
    for (anchor, cursor, expected, end) in [(8, 1, "A你好Z", 7), (4, 4, "A中你好🙂Z", 10)] {
        let (mut app, node) = fixture();
        select(&mut app, node, anchor, cursor);
        let read = start(&mut app, node);
        app.world
            .get::<TextEditing>(node)
            .unwrap()
            .read(|text| assert_eq!(text, "A中🙂Z"));
        let response = read.complete(ClipboardData::text("你好"));
        complete(&mut app.world, &response);
        complete(&mut app.world, &response);
        let editor = app.world.get::<TextEditing>(node).unwrap();
        editor.read(|text| assert_eq!(text, expected));
        assert_eq!(
            editor.selection(),
            TextSelection {
                anchor: end,
                cursor: end
            }
        );
        assert_eq!(result(&mut app), Ok(ClipboardOutcome::Pasted));
        assert_eq!(app.world.resource::<Results>().unwrap().0.len(), 1);
    }
}

#[test]
fn selection_or_preedit_aba_rejects_a_pending_paste() {
    for preedit in [false, true] {
        let (mut app, node) = fixture();
        select(&mut app, node, 1, 4);
        let read = start(&mut app, node);
        if preedit {
            let editor = app.world.get_mut::<TextEditing>(node).unwrap();
            editor.update_preedit("ni", None).unwrap();
            editor.clear_preedit();
        } else {
            select(&mut app, node, 0, 0);
            select(&mut app, node, 1, 4);
        }
        complete(&mut app.world, &read.complete(ClipboardData::text("new")));
        app.world
            .get::<TextEditing>(node)
            .unwrap()
            .read(|text| assert_eq!(text, "A中🙂Z"));
        assert_eq!(result(&mut app), Err(ClipboardError::Unavailable));
    }
}

#[test]
fn invalid_payloads_and_platform_errors_preserve_selection_body_and_preedit() {
    for (text, error) in [
        ("\n", ClipboardError::InvalidData),
        (
            "abcdefghijklmnopqrstuvwxyz1234567890",
            ClipboardError::Capacity,
        ),
    ] {
        let (mut app, node) = fixture();
        select(&mut app, node, 1, 4);
        app.world
            .get_mut::<TextEditing>(node)
            .unwrap()
            .update_preedit("ni", None)
            .unwrap();
        let read = start(&mut app, node);
        complete(&mut app.world, &read.complete(ClipboardData::text(text)));
        let editor = app.world.get::<TextEditing>(node).unwrap();
        editor.read(|text| assert_eq!(text, "A中🙂Z"));
        editor.read_display(|text| assert_eq!(text, "Ani🙂Z"));
        assert_eq!(
            editor.selection(),
            TextSelection {
                anchor: 1,
                cursor: 4
            }
        );
        assert_eq!(result(&mut app), Err(error));
        let read = start(&mut app, node);
        complete(
            &mut app.world,
            &read.complete(Err(ClipboardError::PermissionDenied)),
        );
        assert_eq!(result(&mut app), Err(ClipboardError::PermissionDenied));
    }
}

#[test]
fn text_import_selects_plain_representation_but_rejects_multiple_items() {
    static FORMATS: &[ClipboardFormat] = &[
        ClipboardFormat::borrowed("text/html", b"<b>new</b>"),
        ClipboardFormat::borrowed("text/plain", b"new"),
    ];
    static ITEMS: &[ClipboardItem] = &[ClipboardItem::borrowed(FORMATS)];
    let (mut app, node) = fixture();
    select(&mut app, node, 1, 4);
    let read = start(&mut app, node);
    complete(
        &mut app.world,
        &read.complete(Ok(ClipboardData::borrowed(ITEMS))),
    );
    assert_eq!(result(&mut app), Ok(ClipboardOutcome::Pasted));
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "Anew🙂Z"));
    let read = start(&mut app, node);
    let mut data = ClipboardData::with_capacity(2, 100).unwrap();
    data.push(ITEMS[0].clone()).unwrap();
    data.push(ITEMS[0].clone()).unwrap();
    complete(&mut app.world, &read.complete(Ok(data)));
    assert_eq!(result(&mut app), Err(ClipboardError::Unsupported));
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "Anew🙂Z"));
}

#[test]
fn reads_and_writes_share_one_ordered_platform_queue() {
    let (mut app, node) = fixture();
    select(&mut app, node, 0, 1);
    copy(&mut app.world, node);
    let write = take_write(&mut app.world).unwrap();
    paste(&mut app.world, node);
    assert!(take_request(&mut app.world).is_none());
    complete(&mut app.world, &write.complete(Ok(())));
    let Some(ClipboardRequest::Read(read)) = take_request(&mut app.world) else {
        panic!("ordered read");
    };
    complete(&mut app.world, &read.complete(ClipboardData::text("B")));
    assert_eq!(result(&mut app), Ok(ClipboardOutcome::Pasted));
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "B中🙂Z"));
}
