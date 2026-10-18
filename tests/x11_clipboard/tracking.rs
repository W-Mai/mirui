use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use x11rb::{
    protocol::{
        Event,
        xfixes::{SelectionEvent, SelectionNotifyEvent as SelectionChanged},
        xproto::*,
    },
    rust_connection::RustConnection,
};

use super::{
    session::Session,
    ui::clipboard::{ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem},
    wire::{State, Wire},
};

struct Fixture {
    session: Session<RustConnection<Wire>>,
    state: Arc<Mutex<State>>,
    now: Instant,
    window: u32,
    cursor: usize,
}

#[test]
fn extension_errors_do_not_disable_consistency_checks() {
    let (connection, state) = super::connection();
    state.lock().unwrap().xfixes = true;
    state.lock().unwrap().xfixes_failure = Some(0);
    assert!(matches!(
        Session::new(connection, 0),
        Err(ClipboardError::Platform)
    ));
    assert!(state.lock().unwrap().windows.is_empty());

    let (connection, state) = super::connection();
    state.lock().unwrap().xfixes = true;
    let mut session = Session::new(connection, 0).unwrap();
    state.lock().unwrap().xfixes_failure = Some(2);
    assert_eq!(
        session.begin_read(Instant::now() + Duration::from_secs(10)),
        Err(ClipboardError::Platform)
    );
    let state = state.lock().unwrap();
    assert_eq!(state.windows.len(), 1);
    assert_eq!(state.destroyed.len(), 1);
    assert!(state.conversions.is_empty());
    assert!(session.deadline().is_none());
}

impl Fixture {
    fn new() -> Self {
        let (connection, state) = super::connection();
        state.lock().unwrap().xfixes = true;
        let mut session = Session::new(connection, 0).unwrap();
        let mut item = ClipboardItem::with_capacity(2).unwrap();
        item.push(ClipboardFormat::new(
            "application/x-bytes",
            vec![37; 100_001],
        ))
        .unwrap();
        item.push(ClipboardFormat::borrowed("application/x-empty", b""))
            .unwrap();
        let mut data = ClipboardData::with_capacity(1, 100_001).unwrap();
        data.push(item).unwrap();
        session.write(Arc::new(data), 1000).unwrap();
        let now = Instant::now();
        session.begin_read(now + Duration::from_secs(10)).unwrap();
        let window = state.lock().unwrap().windows[1];
        assert_eq!(state.lock().unwrap().selection_masks[&window], 7);
        Self {
            session,
            state,
            now,
            window,
            cursor: 0,
        }
    }

    fn changed(&self, subtype: SelectionEvent) -> Event {
        let state = self.state.lock().unwrap();
        Event::XfixesSelectionNotify(SelectionChanged {
            response_type: 90,
            subtype,
            sequence: 0,
            window: self.window,
            owner: state.owner,
            selection: state.atoms[b"CLIPBOARD".as_slice()],
            timestamp: 2000,
            selection_timestamp: 1000,
        })
    }

    fn start(&mut self) {
        let event = self.state.lock().unwrap().events.pop_front().unwrap();
        assert!(matches!(event, Event::PropertyNotify(_)));
        assert!(self.session.event(&event, self.now).unwrap().is_none());
    }

    fn confirmation(&mut self) -> Event {
        for _ in 0..128 {
            let request = self
                .state
                .lock()
                .unwrap()
                .conversions
                .get(self.cursor)
                .copied();
            if let Some((requestor, selection, target, property, time)) = request {
                self.cursor += 1;
                let owner = self.state.lock().unwrap().owner;
                assert!(
                    self.session
                        .event(
                            &Event::SelectionRequest(SelectionRequestEvent {
                                response_type: SELECTION_REQUEST_EVENT,
                                sequence: 0,
                                owner,
                                requestor,
                                selection,
                                target,
                                property,
                                time,
                            }),
                            self.now
                        )
                        .unwrap()
                        .is_none()
                );
            }
            loop {
                let event = self.state.lock().unwrap().events.pop_front();
                let Some(event) = event else { break };
                if matches!(&event, Event::PropertyNotify(event)
                    if event.window == self.window && event.atom == self.state.lock().unwrap().atoms[b"TIMESTAMP".as_slice()] && event.state == Property::NEW_VALUE)
                {
                    return event;
                }
                assert!(
                    self.session.event(&event, self.now).unwrap().is_none(),
                    "payload must wait for confirmation"
                );
            }
        }
        panic!("confirmation was not requested");
    }

    fn retired(&self) {
        let state = self.state.lock().unwrap();
        assert_eq!(state.windows.len(), 1);
        assert!(state.selection_masks.is_empty());
        assert!(
            !state
                .properties
                .keys()
                .any(|(window, _)| *window == self.window)
        );
        assert!(self.session.deadline().is_none());
    }
}

#[test]
fn tracking_ignores_prior_changes_and_confirms_all_formats_without_timestamp_conversion() {
    let mut fixture = Fixture::new();
    let old = fixture.changed(SelectionEvent::SET_SELECTION_OWNER);
    assert!(fixture.session.event(&old, fixture.now).unwrap().is_none());
    fixture.start();
    let marker = fixture.confirmation();
    let data = fixture
        .session
        .event(&marker, fixture.now)
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(data.items()[0].formats().len(), 2);
    assert_eq!(data.items()[0].formats()[0].bytes(), vec![37; 100_001]);
    assert!(data.items()[0].formats()[1].bytes().is_empty());
    {
        let state = fixture.state.lock().unwrap();
        assert!(
            state
                .conversions
                .iter()
                .all(|request| request.2 != state.atoms[b"TIMESTAMP".as_slice()])
        );
    }
    fixture.retired();
    assert!(
        fixture
            .session
            .event(&marker, fixture.now)
            .unwrap()
            .is_none()
    );
    assert!(fixture.session.event(&old, fixture.now).unwrap().is_none());
}

#[test]
fn any_selection_change_rejects_reading_and_completed_but_unconfirmed_payloads() {
    for confirming in [false, true] {
        for subtype in [
            SelectionEvent::SET_SELECTION_OWNER,
            SelectionEvent::SELECTION_WINDOW_DESTROY,
            SelectionEvent::SELECTION_CLIENT_CLOSE,
        ] {
            let mut fixture = Fixture::new();
            fixture.start();
            let marker = confirming.then(|| fixture.confirmation());
            let change = fixture.changed(subtype);
            assert!(matches!(
                fixture.session.event(&change, fixture.now).unwrap(),
                Some(Err(ClipboardError::Unavailable))
            ));
            fixture.retired();
            if let Some(marker) = marker {
                assert!(
                    fixture
                        .session
                        .event(&marker, fixture.now)
                        .unwrap()
                        .is_none()
                );
            }
            assert!(
                fixture
                    .session
                    .event(&change, fixture.now)
                    .unwrap()
                    .is_none()
            );
            fixture
                .session
                .begin_read(fixture.now + Duration::from_secs(10))
                .unwrap();
            assert!(
                fixture
                    .session
                    .event(&change, fixture.now)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn deadline_covers_start_transfer_and_confirmation_and_cannot_revive() {
    for phase in 0..3 {
        let mut fixture = Fixture::new();
        if phase > 0 {
            fixture.start();
        }
        let marker = (phase == 2).then(|| fixture.confirmation());
        let deadline = fixture.now + Duration::from_secs(10);
        assert_eq!(fixture.session.deadline(), Some(deadline));
        assert!(matches!(
            fixture.session.expire(deadline).unwrap(),
            Some(Err(ClipboardError::Unavailable))
        ));
        fixture.retired();
        if let Some(marker) = marker {
            assert!(fixture.session.event(&marker, deadline).unwrap().is_none());
        }
        assert!(fixture.session.expire(deadline).unwrap().is_none());
    }
}
