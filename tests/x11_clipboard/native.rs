use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use x11rb::{
    connection::Connection, protocol::xproto::ConnectionExt, rust_connection::RustConnection,
};

use super::{
    session::Session,
    ui::clipboard::{ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem},
};

const WAIT: Duration = Duration::from_secs(10);
type NativeSession = Session<RustConnection>;

#[path = "server.rs"]
mod server;
use server::Server;

fn session(display: &str) -> NativeSession {
    let (connection, screen) = x11rb::connect(Some(display)).unwrap();
    Session::new(connection, screen).unwrap()
}

fn timestamp(session: &mut NativeSession, previous: u32) -> u32 {
    let deadline = Instant::now() + WAIT;
    loop {
        session.request_time().unwrap();
        loop {
            if let Some(event) = session.connection().poll_for_event().unwrap() {
                let time = session.timestamp(&event);
                assert!(session.event(&event, Instant::now()).unwrap().is_none());
                if let Some(time) = time {
                    if time != previous {
                        return time;
                    }
                    break;
                }
            }
            assert!(Instant::now() < deadline, "server timestamp timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn finish_read(
    source: &mut NativeSession,
    reader: &mut NativeSession,
) -> Result<ClipboardData, ClipboardError> {
    let deadline = Instant::now() + WAIT;
    loop {
        while let Some(event) = source.connection().poll_for_event().unwrap() {
            assert!(source.event(&event, Instant::now()).unwrap().is_none());
        }
        while let Some(event) = reader.connection().poll_for_event().unwrap() {
            if let Some(result) = reader.event(&event, Instant::now()).unwrap() {
                return result;
            }
        }
        let now = Instant::now();
        assert!(source.expire(now).unwrap().is_none());
        if let Some(result) = reader.expire(now).unwrap() {
            return result;
        }
        assert!(now < deadline, "selection exchange timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn exchange(source: &mut NativeSession, reader: &mut NativeSession) -> ClipboardData {
    reader.begin_read(Instant::now() + WAIT).unwrap();
    finish_read(source, reader).unwrap()
}

fn assert_data(actual: &ClipboardData, expected: &ClipboardData) {
    assert_eq!(actual.items().len(), expected.items().len());
    for (actual, expected) in actual.items().iter().zip(expected.items()) {
        assert_eq!(actual.formats().len(), expected.formats().len());
        for expected in expected.formats() {
            let actual = actual
                .formats()
                .iter()
                .find(|format| format.mime() == expected.mime())
                .unwrap();
            assert_eq!(actual.bytes(), expected.bytes(), "{}", expected.mime());
        }
    }
}

fn window_count(session: &NativeSession) -> usize {
    let root = session.connection().setup().roots[0].root;
    session
        .connection()
        .query_tree(root)
        .unwrap()
        .reply()
        .unwrap()
        .children
        .len()
}

#[test]
#[ignore = "requires Xvfb; starts an isolated server with TCP disabled"]
fn isolated_server_preserves_snapshots_and_retires_read_windows() {
    let (_server, display) = Server::start();
    let mut source = session(&display);
    let mut reader = session(&display);
    let baseline_windows = window_count(&reader);

    let mut item = ClipboardItem::with_capacity(4).unwrap();
    item.push(ClipboardFormat::borrowed(
        "text/plain",
        "native 雪\n".as_bytes(),
    ))
    .unwrap();
    item.push(ClipboardFormat::new(
        "application/x-first",
        vec![0x83; 200_001],
    ))
    .unwrap();
    let mut bytes = vec![0x42; 131_075];
    bytes[131_074] = 0;
    item.push(ClipboardFormat::new("application/x-second", bytes))
        .unwrap();
    item.push(ClipboardFormat::borrowed("application/x-empty", b""))
        .unwrap();
    let mut data = ClipboardData::with_capacity(1, 400_000).unwrap();
    data.push(item).unwrap();
    let data = Arc::new(data);
    let time = timestamp(&mut source, 0);
    source.write(data.clone(), time).unwrap();
    for _ in 0..3 {
        assert_data(&exchange(&mut source, &mut reader), &data);
        assert_eq!(window_count(&reader), baseline_windows);
        assert!(source.deadline().is_none());
        assert!(reader.deadline().is_none());
    }

    // Reasserting the same owner with the same timestamp still invalidates a read.
    reader.begin_read(Instant::now() + WAIT).unwrap();
    timestamp(&mut reader, 0);
    let clipboard = source
        .connection()
        .intern_atom(false, b"CLIPBOARD")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    let owner = source
        .connection()
        .get_selection_owner(clipboard)
        .unwrap()
        .reply()
        .unwrap()
        .owner;
    source
        .connection()
        .set_selection_owner(owner, clipboard, time)
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        finish_read(&mut source, &mut reader),
        Err(ClipboardError::Unavailable)
    ));
    assert_eq!(window_count(&reader), baseline_windows);

    // A replacement on the same owner cannot complete a pending older snapshot.
    reader.begin_read(Instant::now() + WAIT).unwrap();
    timestamp(&mut reader, 0);
    let next_time = timestamp(&mut source, time);
    let replacement = Arc::new(ClipboardData::text("replacement").unwrap());
    source.write(replacement.clone(), next_time).unwrap();
    assert!(finish_read(&mut source, &mut reader).is_err());
    assert_eq!(window_count(&reader), baseline_windows);
    assert_data(&exchange(&mut source, &mut reader), &replacement);

    let mut successor = session(&display);
    let next_time = timestamp(&mut successor, next_time);
    successor.write(data.clone(), next_time).unwrap();
    while let Some(event) = source.connection().poll_for_event().unwrap() {
        assert!(source.event(&event, Instant::now()).unwrap().is_none());
    }
    assert_data(&exchange(&mut successor, &mut reader), &data);
    drop(successor);
    let clipboard = reader
        .connection()
        .intern_atom(false, b"CLIPBOARD")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    let deadline = Instant::now() + WAIT;
    while reader
        .connection()
        .get_selection_owner(clipboard)
        .unwrap()
        .reply()
        .unwrap()
        .owner
        != x11rb::NONE
    {
        assert!(
            Instant::now() < deadline,
            "closed source retained ownership"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    reader.begin_read(Instant::now() + WAIT).unwrap();
    assert_eq!(
        finish_read(&mut source, &mut reader).map(|_| ()),
        Err(ClipboardError::Unavailable)
    );
    assert_eq!(window_count(&reader), baseline_windows);
}
