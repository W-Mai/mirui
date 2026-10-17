use super::*;
use crate::input::event::{focus::FocusState, input::InputEvent};

struct Selected(usize);

#[crate::event]
fn select_all(
    _event: event!(crate::ui::selection::SelectAll),
    selected: res!(mut Selected),
) -> Result<(), crate::ui::selection::RequestError> {
    selected.0 += 1;
    Ok(())
}

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
fn requested_native_paste_keeps_the_original_target_and_selection_guard() {
    for changed in [false, true] {
        let (mut app, node) = fixture();
        app.add_resource(FocusState {
            focused: Some(node),
        });
        select(&mut app, node, 8, 1);
        deliver(&mut app, ClipboardInput::paste_requested());
        let Some(ClipboardRequest::Read(read)) = take_request(&mut app.world) else {
            panic!("expected the existing platform read protocol");
        };
        assert!(take_request(&mut app.world).is_none());
        app.world.resource_mut::<FocusState>().unwrap().focused = app.root;
        if changed {
            select(&mut app, node, 1, 1);
        }
        let response = read.complete(ClipboardData::text("new"));
        complete(&mut app.world, &response);
        complete(&mut app.world, &response);
        flush(&mut app);
        app.world.get::<TextEditing>(node).unwrap().read(|text| {
            assert_eq!(text, if changed { "A中🙂Z" } else { "AnewZ" });
        });
        let results = &app.world.resource::<Results>().unwrap().0;
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].result,
            if changed {
                Err(ClipboardError::Unavailable)
            } else {
                Ok(ClipboardOutcome::Pasted)
            }
        );
    }
}

#[test]
fn native_selection_does_not_access_the_clipboard_or_require_text() {
    let (mut app, editor) = fixture();
    app.add_resource(Selected(0));
    let target = app.compose(app.root.unwrap(), |cx| {
        ui! { View () on SelectAll(select_all::callback()) }
    });
    app.add_resource(FocusState {
        focused: Some(target),
    });
    deliver(&mut app, ClipboardInput::select_all());
    assert_eq!(app.world.resource::<Selected>().unwrap().0, 1);
    assert!(take_request(&mut app.world).is_none());
    assert!(app.world.resource::<Results>().unwrap().0.is_empty());
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    deliver(&mut app, ClipboardInput::select_all());
    assert_eq!(
        app.world.get::<TextEditing>(editor).unwrap().selection(),
        TextSelection {
            anchor: 0,
            cursor: "A中🙂Z".len(),
        }
    );
    assert!(take_request(&mut app.world).is_none());
}

#[test]
fn payload_free_native_actions_without_focus_do_not_start_operations() {
    let (mut app, _) = fixture();
    for input in [
        ClipboardInput::paste_requested(),
        ClipboardInput::select_all(),
    ] {
        deliver(&mut app, input);
    }
    assert!(take_request(&mut app.world).is_none());
    assert!(app.world.resource::<Results>().unwrap().0.is_empty());
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

#[test]
fn deferred_native_paste_keeps_its_target_and_dispatches_only_once() {
    use std::sync::{Arc, Mutex};

    let (mut app, node) = fixture();
    app.add_resource(FocusState {
        focused: Some(node),
    });
    select(&mut app, node, 8, 1);
    let slot = Arc::new(Mutex::new(None));
    let output = slot.clone();
    let input = ClipboardInput::paste_deferred(move |request| {
        assert!(output.lock().unwrap().replace(request).is_none());
        ClipboardDispatch::Pending
    });
    deliver(&mut app, input.clone());
    deliver(&mut app, input);
    app.world.resource_mut::<FocusState>().unwrap().focused = None;
    app.poll_event();
    let response = slot
        .lock()
        .unwrap()
        .take()
        .unwrap()
        .complete(ClipboardData::text("file"));
    complete(&mut app.world, &response);
    complete(&mut app.world, &response);
    flush(&mut app);
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "AfileZ"));
    assert!(take_request(&mut app.world).is_none());
    let results = &app.world.resource::<Results>().unwrap().0;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].result, Ok(ClipboardOutcome::Pasted));
}

#[test]
fn deferred_native_paste_captures_selection_before_waiting_for_an_in_flight_operation() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let (mut app, node) = fixture();
    app.add_resource(FocusState {
        focused: Some(node),
    });
    select(&mut app, node, 1, 8);
    cut(&mut app.world, node);
    let write = take_write(&mut app.world).unwrap();
    let called = Arc::new(AtomicBool::new(false));
    let output = called.clone();
    deliver(
        &mut app,
        ClipboardInput::paste_deferred(move |request| {
            output.store(true, Ordering::Relaxed);
            ClipboardDispatch::Complete(request.complete(ClipboardData::text("stale")))
        }),
    );
    app.poll_event();
    assert!(!called.load(Ordering::Relaxed));
    complete(&mut app.world, &write.complete(Ok(())));
    app.poll_event();
    assert!(called.load(Ordering::Relaxed));
    flush(&mut app);
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "AZ"));
    let results = &app.world.resource::<Results>().unwrap().0;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].result, Ok(ClipboardOutcome::Cut));
    assert_eq!(results[1].result, Err(ClipboardError::Unavailable));
}

#[test]
fn deferred_native_paste_failure_does_not_edit_or_leave_the_service_running() {
    let (mut app, node) = fixture();
    app.add_resource(FocusState {
        focused: Some(node),
    });
    select(&mut app, node, 8, 1);
    deliver(
        &mut app,
        ClipboardInput::paste_deferred(|request| {
            ClipboardDispatch::Complete(request.complete(Err(ClipboardError::Platform)))
        }),
    );
    app.poll_event();
    flush(&mut app);
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, "A中🙂Z"));
    assert!(take_request(&mut app.world).is_none());
    assert!(app.world.resource::<Service>().unwrap().running.is_none());
    assert_eq!(
        app.world.resource::<Results>().unwrap().0[0].result,
        Err(ClipboardError::Platform)
    );
}

#[test]
fn deferred_native_sources_are_released_when_unhandled_or_over_budget() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct Source(Arc<AtomicUsize>);
    impl Drop for Source {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let dropped = Arc::new(AtomicUsize::new(0));
    let make_input = || {
        let source = Source(dropped.clone());
        ClipboardInput::paste_deferred(move |_| {
            drop(source);
            panic!("a discarded source must not start reading");
        })
    };
    let (mut app, node) = fixture();
    deliver(&mut app, make_input());
    assert_eq!(dropped.load(Ordering::Relaxed), 1);
    app.add_resource(FocusState {
        focused: Some(node),
    });
    select(&mut app, node, 0, 1);
    copy(&mut app.world, node);
    let _write = take_write(&mut app.world).unwrap();
    let limit = app.world.resource::<Service>().unwrap().limit;
    for _ in 1..limit {
        copy(&mut app.world, node);
    }
    deliver(&mut app, make_input());
    assert_eq!(dropped.load(Ordering::Relaxed), 2);
    assert_eq!(
        app.world.resource::<Results>().unwrap().0[0].result,
        Err(ClipboardError::Capacity)
    );
}
