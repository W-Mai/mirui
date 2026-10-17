use super::*;
use x11rb::protocol::xproto::ConnectionExt;

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
