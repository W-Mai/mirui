use super::*;
use crate::prelude::*;
use crate::ui::clipboard::{
    self as clipboard, ClipboardFinished, ClipboardFormat, ClipboardItem, ClipboardOutcome,
    CopySelection, CutSelection, InspectSelection, PasteSelection, SelectionState,
};
use x11rb::rust_connection::Stream;

mod native;

mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/x11_clipboard/wire.rs"
    ));
}

const FORMATS: &[ClipboardFormat] = &[
    ClipboardFormat::borrowed("text/plain", b"clipboard"),
    ClipboardFormat::borrowed("application/x-test", &[0, 7, 255, 0]),
];
const ITEMS: &[ClipboardItem] = &[ClipboardItem::borrowed(FORMATS)];

#[derive(Default)]
struct Observed {
    results: Vec<Result<ClipboardOutcome, ClipboardError>>,
    pasted: Option<ClipboardData>,
    cuts: usize,
}

#[crate::event]
fn inspect(_event: event!(InspectSelection)) -> Result<SelectionState, ClipboardError> {
    Ok(SelectionState::new(true, 1_u8))
}

#[crate::event]
fn export(_event: event!(CopySelection)) -> Result<ClipboardData, ClipboardError> {
    Ok(ClipboardData::borrowed(ITEMS))
}

#[crate::event]
fn import(
    event: event!(PasteSelection),
    observed: res!(mut Observed),
) -> Result<(), ClipboardError> {
    observed.pasted = Some(event.data().clone());
    Ok(())
}

#[crate::event]
fn remove(
    _event: event!(CutSelection),
    observed: res!(mut Observed),
) -> Result<(), ClipboardError> {
    observed.cuts += 1;
    Ok(())
}

#[crate::event]
fn finished(event: event!(ClipboardFinished), observed: res!(mut Observed)) {
    observed.results.push(event.result);
}

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn fixture() -> (TestApp, Entity) {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    app.add_resource(Observed::default());
    let root = app.spawn_root().id();
    let editor = app.compose(root, |cx| {
        ui! {
            View () on InspectSelection(inspect::callback()) on CopySelection(export::callback()) on CutSelection(remove::callback()) on PasteSelection(import::callback()) on ClipboardFinished(finished::callback())
        }
    });
    (app, editor)
}

fn write(app: &mut TestApp, editor: Entity) -> ClipboardWrite {
    clipboard::cut(&mut app.world, editor);
    match clipboard::take_request(&mut app.world).unwrap() {
        ClipboardRequest::Write(request) => request,
        _ => panic!("expected write"),
    }
}

fn complete(app: &mut TestApp, response: ClipboardResponse) {
    clipboard::complete(&mut app.world, &response);
    crate::ui::delivery::flush(&mut app.world).unwrap();
}

fn response(bridge: &mut super::super::Bridge) -> ClipboardResponse {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(response) = bridge.poll() {
            return response;
        }
        assert!(
            Instant::now() < deadline,
            "clipboard response did not arrive"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn protocol_peer() -> (Bridge, Arc<Mutex<wire::State>>, JoinHandle<()>) {
    let (socket, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    peer.set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (transport, state) = wire::transport();
    state.lock().unwrap().socket_events = true;
    let server = thread::spawn(move || {
        let mut bytes = [0; 65536];
        loop {
            let count = peer.read(&mut bytes).unwrap();
            if count == 0 {
                break;
            }
            assert_eq!(
                transport.write(&bytes[..count], &mut Vec::new()).unwrap(),
                count
            );
            loop {
                match transport.read(&mut bytes, &mut Vec::new()) {
                    Ok(count) => peer.write_all(&bytes[..count]).unwrap(),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => panic!("protocol read failed: {error}"),
                }
            }
        }
    });
    let bridge = Bridge::spawn(0, move |control| {
        let (stream, _) = platform(DefaultStream::from_unix_stream(socket))?;
        let (stream, cancel) = platform(BoundedStream::new(stream))?;
        stream.set_deadline(Some(Instant::now() + TIMEOUT));
        control.lock().unwrap().cancel = Some(cancel);
        platform(RustConnection::connect_to_stream(stream, 0))
    })
    .unwrap();
    (bridge, state, server)
}

#[test]
fn worker_delivers_owned_formats_through_existing_app_receipts() {
    let (mut bridge, state, server) = protocol_peer();
    let (mut app, editor) = fixture();
    assert!(matches!(
        bridge.write(write(&mut app, editor)),
        ClipboardDispatch::Pending
    ));
    assert_eq!(app.world.resource::<Observed>().unwrap().cuts, 0);
    complete(&mut app, response(&mut bridge));
    assert_eq!(
        app.world.resource::<Observed>().unwrap().results,
        vec![Ok(ClipboardOutcome::Cut)]
    );
    assert_eq!(app.world.resource::<Observed>().unwrap().cuts, 1);

    clipboard::paste(&mut app.world, editor);
    let ClipboardRequest::Read(request) = clipboard::take_request(&mut app.world).unwrap() else {
        panic!("expected read");
    };
    assert!(matches!(bridge.read(request), ClipboardDispatch::Pending));
    complete(&mut app, response(&mut bridge));
    let observed = app.world.resource::<Observed>().unwrap();
    let formats = observed.pasted.as_ref().unwrap().items()[0].formats();
    assert_eq!(formats.len(), 2);
    assert!(formats.iter().any(|format| format.mime() == "application/x-test" && format.bytes() == [0, 7, 255, 0]));
    assert_eq!(observed.results.last(), Some(&Ok(ClipboardOutcome::Pasted)));
    assert_eq!(state.lock().unwrap().windows.len(), 1);

    assert!(matches!(
        bridge.write(write(&mut app, editor)),
        ClipboardDispatch::Pending
    ));
    thread::sleep(Duration::from_millis(20));
    assert!(
        bridge.poll().is_none(),
        "same timestamp must not replace a snapshot"
    );
    state.lock().unwrap().time = Some(2001);
    complete(&mut app, response(&mut bridge));
    assert_eq!(app.world.resource::<Observed>().unwrap().cuts, 2);
    drop(bridge);
    server.join().unwrap();
}

#[test]
fn initialization_failure_and_busy_rejection_preserve_app_operation_identity() {
    let mut bridge = Bridge::new("not-a-display".into(), 0).unwrap();
    let (mut first, first_editor) = fixture();
    let (mut second, second_editor) = fixture();
    assert!(matches!(
        bridge.write(write(&mut first, first_editor)),
        ClipboardDispatch::Pending
    ));
    let ClipboardDispatch::Complete(rejected) = bridge.write(write(&mut second, second_editor))
    else {
        panic!("only one request may be in flight");
    };
    complete(&mut second, rejected);
    complete(&mut first, response(&mut bridge));
    for app in [&first, &second] {
        let observed = app.world.resource::<Observed>().unwrap();
        assert_eq!(observed.cuts, 0);
        assert_eq!(observed.results.len(), 1);
        assert!(observed.results[0].is_err());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !bridge.finished() {
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    assert!(bridge.poll().is_none());
}

#[test]
fn surface_drop_cancels_a_worker_waiting_for_setup_without_joining() {
    let (socket, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let (done, receiver) = mpsc::sync_channel(1);
    let mut bridge = Bridge::spawn(0, move |control| {
        let (stream, _) = platform(DefaultStream::from_unix_stream(socket))?;
        let (stream, cancel) = platform(BoundedStream::new(stream))?;
        control.lock().unwrap().cancel = Some(cancel);
        let result = platform(RustConnection::connect_to_stream(stream, 0));
        done.send(result.is_err()).unwrap();
        result
    })
    .unwrap();
    let (mut app, editor) = fixture();
    assert!(matches!(
        bridge.write(write(&mut app, editor)),
        ClipboardDispatch::Pending
    ));
    peer.read_exact(&mut [0; 12]).unwrap();
    drop(bridge);
    assert!(receiver.recv_timeout(Duration::from_secs(5)).unwrap());
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
    assert_eq!(app.world.resource::<Observed>().unwrap().cuts, 0);
}
