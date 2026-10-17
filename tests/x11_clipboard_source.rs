#![cfg(any(target_os = "linux", target_os = "macos"))]

extern crate alloc;

pub mod ui {
    pub mod clipboard {
        pub use mirui::ui::clipboard::ClipboardError;
        mod data {
            include!("../src/ui/clipboard/data.rs");
        }
        pub(crate) use data::validate_mime;
        pub use data::{ClipboardData, ClipboardFormat, ClipboardItem};
    }
}

#[path = "../src/surface/host_clipboard/x11/source.rs"]
mod source;

#[path = "../src/surface/host_clipboard/x11/protocol.rs"]
mod protocol;
#[path = "../src/surface/host_clipboard/x11/read.rs"]
mod read;

#[path = "../src/surface/host_clipboard/x11/session.rs"]
mod session;

#[path = "x11_clipboard/session.rs"]
mod session_contracts;

#[path = "x11_clipboard/read.rs"]
mod read_contracts;

#[path = "x11_clipboard/wire.rs"]
mod wire;

#[path = "x11_clipboard/native.rs"]
mod native_contracts;

fn connection() -> (
    x11rb::rust_connection::RustConnection<wire::Wire>,
    std::sync::Arc<std::sync::Mutex<wire::State>>,
) {
    let (wire, state) = wire::transport();
    (
        x11rb::rust_connection::RustConnection::connect_to_stream(wire, 0).unwrap(),
        state,
    )
}

#[test]
fn source_wire_protocol() {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    use x11rb::protocol::{Event, xproto::*};
    use x11rb::x11_utils::TryParse;

    let (connection, state) = connection();
    let previous_mask = u32::from(EventMask::PROPERTY_CHANGE | EventMask::FOCUS_CHANGE);
    state.lock().unwrap().masks.insert(200, previous_mask);
    let atoms = protocol::Atoms::new(&connection).unwrap().reply().unwrap();
    let mut source = source::Source::new(&connection, 100, atoms).unwrap();
    let now = Instant::now();
    source
        .claim(
            &connection,
            Arc::new(ui::clipboard::ClipboardData::text("hello").unwrap()),
            1000,
        )
        .unwrap();
    let atom = |name: &[u8]| state.lock().unwrap().atoms[name];
    let request_time = core::cell::Cell::new(1000);
    let request = |target, property| {
        Event::SelectionRequest(SelectionRequestEvent {
            response_type: SELECTION_REQUEST_EVENT,
            sequence: 0,
            time: request_time.get(),
            owner: 100,
            requestor: 200,
            selection: atom(b"CLIPBOARD"),
            target,
            property,
        })
    };
    source
        .event(&connection, &request(atom(b"UTF8_STRING"), 500), now)
        .unwrap();
    assert_eq!(state.lock().unwrap().properties[&(200, 500)].2, b"hello");
    source
        .event(&connection, &request(atom(b"TIMESTAMP"), 501), now)
        .unwrap();
    assert_eq!(
        state.lock().unwrap().properties[&(200, 501)].2,
        1000_u32.to_ne_bytes()
    );
    source
        .event(&connection, &request(atom(b"TARGETS"), 502), now)
        .unwrap();
    assert_eq!(state.lock().unwrap().properties[&(200, 502)].2.len(), 24);
    source.event(&connection, &request(9999, 503), now).unwrap();
    assert_eq!(
        SelectionNotifyEvent::try_parse(state.lock().unwrap().notifications.last().unwrap())
            .unwrap()
            .0
            .property,
        0
    );

    let pairs = [
        atom(b"UTF8_STRING"),
        504,
        9999,
        505,
        atom(b"UTF8_STRING"),
        506,
        atom(b"UTF8_STRING"),
        504,
        atom(b"UTF8_STRING"),
        507,
    ];
    let atom_pair = atom(b"ATOM_PAIR");
    state.lock().unwrap().properties.insert(
        (200, 506),
        (
            atom_pair,
            32,
            pairs.into_iter().flat_map(u32::to_ne_bytes).collect(),
        ),
    );
    source
        .event(&connection, &request(atom(b"MULTIPLE"), 506), now)
        .unwrap();
    assert_eq!(state.lock().unwrap().properties[&(200, 504)].2, b"hello");
    assert_eq!(
        &state.lock().unwrap().properties[&(200, 506)].2[12..16],
        &[0; 4]
    );
    assert_eq!(
        &state.lock().unwrap().properties[&(200, 506)].2[20..24],
        &[0; 4]
    );
    assert_eq!(
        &state.lock().unwrap().properties[&(200, 506)].2[28..32],
        &[0; 4]
    );
    assert_eq!(state.lock().unwrap().properties[&(200, 507)].2, b"hello");

    let old = "x".repeat(100_000);
    request_time.set(1001);
    source
        .claim(
            &connection,
            Arc::new(ui::clipboard::ClipboardData::text(&old).unwrap()),
            1001,
        )
        .unwrap();
    source
        .event(&connection, &request(atom(b"UTF8_STRING"), 510), now)
        .unwrap();
    let incr = atom(b"INCR");
    assert_eq!(state.lock().unwrap().properties[&(200, 510)].0, incr);
    source
        .claim(
            &connection,
            Arc::new(ui::clipboard::ClipboardData::text("new").unwrap()),
            1002,
        )
        .unwrap();
    let delete = |property| {
        Event::PropertyNotify(PropertyNotifyEvent {
            response_type: PROPERTY_NOTIFY_EVENT,
            sequence: 0,
            window: 200,
            atom: property,
            time: 1001,
            state: Property::DELETE,
        })
    };
    let mut copied = Vec::new();
    for _ in 0..3 {
        source.event(&connection, &delete(510), now).unwrap();
        copied.extend_from_slice(&state.lock().unwrap().properties[&(200, 510)].2);
    }
    assert_eq!(copied, old.as_bytes());
    assert!(state.lock().unwrap().properties[&(200, 510)].2.is_empty());
    assert_eq!(state.lock().unwrap().masks[&200], previous_mask);
    source
        .expire(&connection, now + Duration::from_secs(11))
        .unwrap();
}

#[test]
fn ownership_changes_keep_transfers_bounded_and_release_peer_observation() {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    use x11rb::protocol::{Event, xproto::*};
    use x11rb::x11_utils::TryParse;

    let (connection, state) = connection();
    let atoms = protocol::Atoms::new(&connection).unwrap().reply().unwrap();
    let mut source = source::Source::new(&connection, 100, atoms).unwrap();
    let now = Instant::now();
    let data = Arc::new(ui::clipboard::ClipboardData::text(&"x".repeat(100_000)).unwrap());
    source.claim(&connection, data.clone(), 1000).unwrap();
    assert!(source.claim(&connection, data.clone(), 1000).is_err());
    assert!(source.claim(&connection, data.clone(), 999).is_err());
    let atom = |name: &[u8]| state.lock().unwrap().atoms[name];
    let request = |property| {
        Event::SelectionRequest(SelectionRequestEvent {
            response_type: SELECTION_REQUEST_EVENT,
            sequence: 0,
            time: 1000,
            owner: 100,
            requestor: 200,
            selection: atom(b"CLIPBOARD"),
            target: atom(b"UTF8_STRING"),
            property,
        })
    };
    for property in 500..516 {
        source.event(&connection, &request(property), now).unwrap();
        assert_eq!(
            SelectionNotifyEvent::try_parse(state.lock().unwrap().notifications.last().unwrap())
                .unwrap()
                .0
                .property,
            property
        );
    }
    assert_eq!(Arc::strong_count(&data), 18);
    source.event(&connection, &request(516), now).unwrap();
    assert_eq!(
        SelectionNotifyEvent::try_parse(state.lock().unwrap().notifications.last().unwrap())
            .unwrap()
            .0
            .property,
        0
    );
    let clear = Event::SelectionClear(SelectionClearEvent {
        response_type: SELECTION_CLEAR_EVENT,
        sequence: 0,
        time: 1001,
        owner: 100,
        selection: atom(b"CLIPBOARD"),
    });
    source.event(&connection, &clear, now).unwrap();
    assert_eq!(Arc::strong_count(&data), 18);
    state.lock().unwrap().owner = 999;
    source.event(&connection, &clear, now).unwrap();
    assert_eq!(Arc::strong_count(&data), 17);
    source.event(&connection, &request(517), now).unwrap();
    assert_eq!(
        SelectionNotifyEvent::try_parse(state.lock().unwrap().notifications.last().unwrap())
            .unwrap()
            .0
            .property,
        0
    );
    let delete = Event::PropertyNotify(PropertyNotifyEvent {
        response_type: PROPERTY_NOTIFY_EVENT,
        sequence: 0,
        time: 1002,
        window: 200,
        atom: 500,
        state: Property::DELETE,
    });
    source
        .event(&connection, &delete, now + Duration::from_secs(9))
        .unwrap();
    assert_eq!(state.lock().unwrap().properties[&(200, 500)].2.len(), 65536);
    source
        .expire(&connection, now + Duration::from_secs(10))
        .unwrap();
    assert_eq!(Arc::strong_count(&data), 1);
    assert_eq!(state.lock().unwrap().masks[&200], 0);

    source.claim(&connection, data.clone(), 1003).unwrap();
    let mut new_request = match request(518) {
        Event::SelectionRequest(value) => value,
        _ => unreachable!(),
    };
    new_request.time = 1003;
    source
        .event(&connection, &Event::SelectionRequest(new_request), now)
        .unwrap();
    assert_eq!(Arc::strong_count(&data), 3);
    source
        .event(
            &connection,
            &Event::DestroyNotify(DestroyNotifyEvent {
                response_type: DESTROY_NOTIFY_EVENT,
                sequence: 0,
                event: 200,
                window: 200,
            }),
            now,
        )
        .unwrap();
    assert_eq!(Arc::strong_count(&data), 2);
}
#[path = "../src/surface/host_clipboard/x11/transfer.rs"]
mod transfer;
