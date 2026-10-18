use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use x11rb::{
    protocol::{Event, xproto::*},
    rust_connection::RustConnection,
};

use super::{
    read::Read,
    ui::clipboard::{ClipboardData, ClipboardError},
    wire::{State, Wire},
};

struct Reader {
    connection: RustConnection<Wire>,
    state: Arc<Mutex<State>>,
    read: Read,
    now: Instant,
}

impl Reader {
    fn new() -> Self {
        let (connection, state) = super::connection();
        state.lock().unwrap().owner = 100;
        let now = Instant::now();
        let atoms = super::protocol::Atoms::new(&connection)
            .unwrap()
            .reply()
            .unwrap();
        let read = Read::begin(
            &connection,
            atoms,
            200,
            500,
            2000,
            now + Duration::from_secs(10),
            true,
        )
        .unwrap();
        Self {
            connection,
            state,
            read,
            now,
        }
    }

    fn atom(&self, name: &[u8]) -> u32 {
        self.connection
            .intern_atom(false, name)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    }

    fn deliver(
        &mut self,
        kind: u32,
        format: u8,
        bytes: Vec<u8>,
    ) -> Result<Option<ClipboardData>, ClipboardError> {
        let (requestor, selection, target, property, time) =
            *self.state.lock().unwrap().conversions.last().unwrap();
        self.state
            .lock()
            .unwrap()
            .properties
            .insert((requestor, property), (kind, format, bytes));
        self.read.event(
            &self.connection,
            &Event::SelectionNotify(SelectionNotifyEvent {
                response_type: SELECTION_NOTIFY_EVENT,
                sequence: 0,
                requestor,
                selection,
                target,
                property,
                time,
            }),
            self.now,
        )
    }

    fn timestamp(&mut self, timestamp: u32) -> Result<Option<ClipboardData>, ClipboardError> {
        self.deliver(
            AtomEnum::INTEGER.into(),
            32,
            timestamp.to_ne_bytes().to_vec(),
        )
    }

    fn targets(&mut self, atoms: &[u32]) {
        assert!(
            self.deliver(
                AtomEnum::ATOM.into(),
                32,
                atoms.iter().flat_map(|atom| atom.to_ne_bytes()).collect()
            )
            .unwrap()
            .is_none()
        );
    }

    fn chunk(
        &mut self,
        kind: u32,
        bytes: Vec<u8>,
    ) -> Result<Option<ClipboardData>, ClipboardError> {
        self.state
            .lock()
            .unwrap()
            .properties
            .insert((200, 500), (kind, 8, bytes));
        self.read.event(
            &self.connection,
            &Event::PropertyNotify(PropertyNotifyEvent {
                response_type: PROPERTY_NOTIFY_EVENT,
                sequence: 0,
                window: 200,
                atom: 500,
                time: 2001,
                state: Property::NEW_VALUE,
            }),
            self.now,
        )
    }
}

#[test]
fn formats_share_one_verified_snapshot_and_property_pages_keep_binary_tails() {
    let mut reader = Reader::new();
    assert!(reader.timestamp(1000).unwrap().is_none());
    let plain = reader.atom(b"text/plain");
    let utf8 = reader.atom(b"UTF8_STRING");
    let binary = reader.atom(b"application/x-test");
    reader.targets(&[utf8, plain, AtomEnum::STRING.into(), binary]);
    assert_eq!(
        reader.state.lock().unwrap().conversions.last().unwrap().2,
        plain
    );
    assert!(
        reader
            .deliver(plain, 8, b"a\0b".to_vec())
            .unwrap()
            .is_none()
    );
    let mut bytes = vec![31; 100_001];
    bytes[99_999] = 0;
    bytes[100_000] = 0;
    assert!(reader.deliver(binary, 8, bytes.clone()).unwrap().is_none());
    let result = reader.timestamp(1000).unwrap().unwrap();
    let formats = result.items()[0].formats();
    assert_eq!(formats.len(), 2);
    assert_eq!(formats[0].mime(), "text/plain");
    assert_eq!(formats[0].bytes(), b"a\0b");
    assert_eq!(formats[1].bytes(), bytes);
    assert!(
        !reader
            .state
            .lock()
            .unwrap()
            .properties
            .contains_key(&(200, 500))
    );
}

#[test]
fn either_owner_change_or_same_owner_replacement_rejects_all_formats() {
    for change_owner in [false, true] {
        let mut reader = Reader::new();
        reader.timestamp(1000).unwrap();
        let plain = reader.atom(b"UTF8_STRING");
        reader.targets(&[plain]);
        reader.deliver(plain, 8, b"snapshot".to_vec()).unwrap();
        if change_owner {
            reader.state.lock().unwrap().owner = 101;
        }
        assert!(matches!(
            reader.timestamp(1001),
            Err(ClipboardError::Unavailable)
        ));
        assert!(reader.timestamp(1000).is_err());
    }
}

#[test]
fn incr_requires_a_matching_terminator_and_never_trusts_the_size_hint_as_exact() {
    let mut reader = Reader::new();
    reader.timestamp(1000).unwrap();
    let binary = reader.atom(b"application/x-test");
    let incr = reader.atom(b"INCR");
    reader.targets(&[binary]);
    assert!(
        reader
            .deliver(incr, 32, 2u32.to_ne_bytes().to_vec())
            .unwrap()
            .is_none()
    );
    assert!(
        !reader
            .state
            .lock()
            .unwrap()
            .properties
            .contains_key(&(200, 500))
    );
    assert!(reader.chunk(binary, vec![1, 2, 0]).unwrap().is_none());
    assert!(reader.chunk(binary, vec![0, 255]).unwrap().is_none());
    assert!(reader.chunk(binary, vec![]).unwrap().is_none());
    let result = reader.timestamp(1000).unwrap().unwrap();
    assert_eq!(result.items()[0].formats()[0].bytes(), &[1, 2, 0, 0, 255]);
}

#[test]
fn missing_or_changed_incr_data_cannot_complete_and_progress_does_not_extend_timeout() {
    for failure in 0..3 {
        let mut reader = Reader::new();
        reader.timestamp(1000).unwrap();
        let binary = reader.atom(b"application/x-test");
        let incr = reader.atom(b"INCR");
        reader.targets(&[binary]);
        reader
            .deliver(incr, 32, 4u32.to_ne_bytes().to_vec())
            .unwrap();
        reader.chunk(binary, vec![1]).unwrap();
        match failure {
            0 => assert!(matches!(
                reader.chunk(binary, vec![]),
                Err(ClipboardError::InvalidData)
            )),
            1 => assert!(matches!(
                reader.chunk(binary + 1, vec![1]),
                Err(ClipboardError::InvalidData)
            )),
            _ => {
                reader.now += Duration::from_secs(10);
                assert!(reader.read.expired(reader.now));
                assert!(matches!(
                    reader.chunk(binary, vec![1]),
                    Err(ClipboardError::Unavailable)
                ));
            }
        }
        assert!(reader.timestamp(1000).is_err());
    }
}

#[test]
fn latin1_string_is_transcoded_without_replacing_or_dropping_bytes() {
    let mut reader = Reader::new();
    reader.timestamp(1000).unwrap();
    reader.targets(&[AtomEnum::STRING.into()]);
    reader
        .deliver(
            AtomEnum::STRING.into(),
            8,
            vec![0xe9, 0, b'\r', b'\n', 0xff],
        )
        .unwrap();
    let result = reader.timestamp(1000).unwrap().unwrap();
    assert_eq!(result.plain_text().unwrap(), "é\0\r\nÿ");
}

#[test]
fn source_and_reader_exchange_multiple_incr_formats_on_one_protocol_connection() {
    use super::{
        source::Source,
        ui::clipboard::{ClipboardFormat, ClipboardItem},
    };

    let (connection, state) = super::connection();
    let mask = u32::from(EventMask::PROPERTY_CHANGE);
    state.lock().unwrap().masks.insert(200, mask);
    let atoms = super::protocol::Atoms::new(&connection)
        .unwrap()
        .reply()
        .unwrap();
    let mut source = Source::new(&connection, 100, atoms).unwrap();
    let mut item = ClipboardItem::with_capacity(2).unwrap();
    item.push(ClipboardFormat::new(
        "text/plain",
        "text".repeat(25_000).into_bytes(),
    ))
    .unwrap();
    item.push(ClipboardFormat::new("application/x-test", vec![0; 90_001]))
        .unwrap();
    let mut data = ClipboardData::with_capacity(1, 190_001).unwrap();
    data.push(item).unwrap();
    source.claim(&connection, Arc::new(data), 1000).unwrap();
    let now = Instant::now();
    let mut read = Read::begin(
        &connection,
        atoms,
        200,
        500,
        2000,
        now + Duration::from_secs(10),
        true,
    )
    .unwrap();
    let mut request_index = 0;
    let mut result = None;
    for _ in 0..100 {
        let request = state
            .lock()
            .unwrap()
            .conversions
            .get(request_index)
            .copied();
        if let Some((requestor, selection, target, property, time)) = request {
            request_index += 1;
            source
                .event(
                    &connection,
                    &Event::SelectionRequest(SelectionRequestEvent {
                        response_type: SELECTION_REQUEST_EVENT,
                        sequence: 0,
                        owner: 100,
                        requestor,
                        selection,
                        target,
                        property,
                        time,
                    }),
                    now,
                )
                .unwrap();
        }
        loop {
            let event = state.lock().unwrap().events.pop_front();
            let Some(event) = event else {
                break;
            };
            source.event(&connection, &event, now).unwrap();
            if result.is_none() {
                result = read.event(&connection, &event, now).unwrap();
            }
        }
        if result.is_some() {
            break;
        }
    }
    let result = result.expect("both incremental formats must complete");
    let formats = result.items()[0].formats();
    assert_eq!(formats.len(), 2);
    assert_eq!(formats[0].bytes(), "text".repeat(25_000).as_bytes());
    assert_eq!(formats[1].bytes(), vec![0; 90_001]);
    assert_eq!(state.lock().unwrap().masks[&200], mask);
}
