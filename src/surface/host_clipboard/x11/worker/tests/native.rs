use super::*;
use x11rb::protocol::{Event, xproto::*};

mod server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/x11_clipboard/server.rs"
    ));
}

fn paste(bridge: &mut Bridge, app: &mut TestApp, editor: Entity) {
    clipboard::paste(&mut app.world, editor);
    let ClipboardRequest::Read(request) = clipboard::take_request(&mut app.world).unwrap() else {
        panic!("expected read");
    };
    assert!(matches!(bridge.read(request), ClipboardDispatch::Pending));
    complete(app, response(bridge));
}

#[test]
#[ignore = "requires Xvfb; starts an isolated server with TCP disabled"]
fn native_worker_preserves_app_receipts_and_releases_server_resources() {
    let (_server, display) = server::Server::start();
    let (observer, screen) = x11rb::connect(Some(&display)).unwrap();
    let root = observer.setup().roots[screen].root;
    let clipboard = observer
        .intern_atom(false, b"CLIPBOARD")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    let windows = || {
        observer
            .query_tree(root)
            .unwrap()
            .reply()
            .unwrap()
            .children
            .len()
    };
    let owner = || {
        observer
            .get_selection_owner(clipboard)
            .unwrap()
            .reply()
            .unwrap()
            .owner
    };
    let baseline = windows();
    let mut source = Bridge::new(display.clone(), screen).unwrap();
    let mut reader = Bridge::new(display, screen).unwrap();
    let (mut origin, origin_editor) = fixture();
    let (mut destination, destination_editor) = fixture();

    for index in 0..3 {
        assert!(matches!(
            source.write(write(&mut origin, origin_editor)),
            ClipboardDispatch::Pending
        ));
        assert_eq!(origin.world.resource::<Observed>().unwrap().cuts, index);
        complete(&mut origin, response(&mut source));
        let observed = origin.world.resource::<Observed>().unwrap();
        assert_eq!(observed.cuts, index + 1);
        assert_eq!(observed.results.len(), index + 1);
        assert_eq!(observed.results.last(), Some(&Ok(ClipboardOutcome::Cut)));

        paste(&mut reader, &mut destination, destination_editor);
        let observed = destination.world.resource::<Observed>().unwrap();
        assert_eq!(observed.results.len(), index + 1);
        assert_eq!(observed.results.last(), Some(&Ok(ClipboardOutcome::Pasted)));
        let formats = observed.pasted.as_ref().unwrap().items()[0].formats();
        assert_eq!(formats.len(), FORMATS.len());
        for expected in FORMATS {
            let actual = formats
                .iter()
                .find(|format| format.mime() == expected.mime())
                .unwrap();
            assert_eq!(actual.bytes(), expected.bytes());
        }
        assert_eq!(windows(), baseline + 2);
        assert!(source.poll().is_none());
        assert!(reader.poll().is_none());
    }

    assert_ne!(owner(), x11rb::NONE);
    drop(source);
    let deadline = Instant::now() + Duration::from_secs(5);
    while owner() != x11rb::NONE {
        assert!(
            Instant::now() < deadline,
            "closed worker retained ownership"
        );
        thread::sleep(Duration::from_millis(1));
    }
    destination.world.resource_mut::<Observed>().unwrap().pasted = None;
    paste(&mut reader, &mut destination, destination_editor);
    let observed = destination.world.resource::<Observed>().unwrap();
    assert!(observed.pasted.is_none());
    assert_eq!(observed.results.len(), 4);
    assert_eq!(
        observed.results.last(),
        Some(&Err(ClipboardError::Unavailable))
    );
    drop(reader);
    let deadline = Instant::now() + Duration::from_secs(5);
    while windows() != baseline {
        assert!(Instant::now() < deadline, "closed worker retained windows");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[ignore = "requires Xvfb; starts an isolated server with TCP disabled"]
fn native_pending_reads_release_windows_on_deadline_and_surface_close() {
    let (_server, display) = server::Server::start();
    let (source, screen) = x11rb::connect(Some(&display)).unwrap();
    let root = source.setup().roots[screen].root;
    let owner = source.generate_id().unwrap();
    source
        .create_window(
            0,
            owner,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap()
        .check()
        .unwrap();
    let selection = source
        .intern_atom(false, b"CLIPBOARD")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    source
        .set_selection_owner(owner, selection, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    let windows = || {
        source
            .query_tree(root)
            .unwrap()
            .reply()
            .unwrap()
            .children
            .len()
    };
    let baseline = windows();
    for cancel in [false, true] {
        let mut bridge = Bridge::new(display.clone(), screen).unwrap();
        let (mut app, editor) = fixture();
        clipboard::paste(&mut app.world, editor);
        let request = clipboard::take_request(&mut app.world).unwrap();
        bridge
            .requests
            .try_send(Work {
                request: Some(request),
                replies: bridge.replies.clone(),
                deadline: Instant::now() + Duration::from_secs(2),
            })
            .unwrap_or_else(|_| panic!("read admission failed"));
        bridge.busy = true;
        bridge.wake.write_all(&[1]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(Event::SelectionRequest(_)) = source.poll_for_event().unwrap() {
                break;
            }
            assert!(Instant::now() < deadline, "read did not reach the source");
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(windows(), baseline + 2);
        if !cancel {
            complete(&mut app, response(&mut bridge));
            assert_eq!(
                app.world.resource::<Observed>().unwrap().results,
                [Err(ClipboardError::Unavailable)]
            );
            assert!(bridge.poll().is_none());
        }
        drop(bridge);
        while windows() != baseline {
            assert!(
                Instant::now() < deadline,
                "read resources survived cancellation"
            );
            thread::sleep(Duration::from_millis(1));
        }
        assert!(app.world.resource::<Observed>().unwrap().pasted.is_none());
    }
}
