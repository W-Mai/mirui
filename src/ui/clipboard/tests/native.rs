use super::*;
use crate::input::event::{focus::FocusState, input::InputEvent};

fn deliver(app: &mut TestApp, input: ClipboardInput) {
    assert!(
        crate::ui::delivery::dispatch_input(
            &mut app.world,
            app.root,
            &InputEvent::ClipboardInput(input),
            64,
            32,
        )
        .unwrap()
    );
    flush(app);
}

#[test]
fn native_cut_requires_acknowledgment_and_paste_uses_the_component_selection() {
    let (mut app, node) = fixture();
    app.add_resource(FocusState {
        focused: Some(node),
    });
    select(&mut app, node, 8, 1);
    deliver(&mut app, ClipboardInput::copy());
    let write = take_write(&mut app.world).unwrap();
    assert_eq!(write.data().plain_text(), Ok("中🙂"));
    complete(&mut app.world, &write.complete(Ok(())));
    deliver(&mut app, ClipboardInput::cut());
    let write = take_write(&mut app.world).unwrap();
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "A中🙂Z"));
    complete(&mut app.world, &write.complete(Ok(())));
    flush(&mut app);
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "AZ"));
    deliver(&mut app, ClipboardInput::paste(ClipboardData::text("中🙂")));
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "A中🙂Z"));
    assert!(take_request(&mut app.world).is_none());
    let results = &app.world.resource::<Results>().unwrap().0;
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].result, Ok(ClipboardOutcome::Copied));
    assert_eq!(results[1].result, Ok(ClipboardOutcome::Cut));
    assert_eq!(results[2].result, Ok(ClipboardOutcome::Pasted));
}

#[test]
fn native_paste_rejects_incomplete_data_without_editing_or_reading_the_platform() {
    let (mut app, node) = fixture();
    app.add_resource(FocusState {
        focused: Some(node),
    });
    select(&mut app, node, 8, 1);
    deliver(
        &mut app,
        ClipboardInput::paste(Err(ClipboardError::Unsupported)),
    );
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "A中🙂Z"));
    assert!(take_request(&mut app.world).is_none());
    assert_eq!(
        app.world.resource::<Results>().unwrap().0[0].result,
        Err(ClipboardError::Unsupported)
    );
    app.world.resource_mut::<FocusState>().unwrap().focused = None;
    deliver(
        &mut app,
        ClipboardInput::paste(ClipboardData::text("ignored")),
    );
    assert_eq!(app.world.resource::<Results>().unwrap().0.len(), 1);
}

#[test]
fn native_paste_invalidates_an_older_pending_cut_without_waiting_for_it() {
    let (mut app, node) = fixture();
    app.add_resource(FocusState {
        focused: Some(node),
    });
    select(&mut app, node, 1, 8);
    deliver(&mut app, ClipboardInput::cut());
    let write = take_write(&mut app.world).unwrap();
    deliver(&mut app, ClipboardInput::paste(ClipboardData::text("new")));
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "AnewZ"));
    complete(&mut app.world, &write.complete(Ok(())));
    flush(&mut app);
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "AnewZ"));
    let results = &app.world.resource::<Results>().unwrap().0;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].result, Ok(ClipboardOutcome::Pasted));
    assert_eq!(results[1].result, Ok(ClipboardOutcome::CopiedWithoutCut));
}
