use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use x11rb::{
    connection::Connection,
    protocol::{Event, xproto::*},
};

use super::{
    session::Session,
    ui::clipboard::{ClipboardData, ClipboardError},
};

fn denied(request: (u32, u32, u32, u32, u32)) -> Event {
    let (requestor, selection, target, _, time) = request;
    Event::SelectionNotify(SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        sequence: 0,
        requestor,
        selection,
        target,
        property: x11rb::NONE,
        time,
    })
}

#[test]
fn read_windows_retire_on_rejection_timeout_and_initialization_failure() {
    let (connection, state) = super::connection();
    let mut session = Session::new(connection, 0).unwrap();
    assert_eq!(state.lock().unwrap().windows.len(), 1);
    let atom_count = state.lock().unwrap().atoms.len();
    let now = Instant::now();
    assert_eq!(
        session.begin_read(2000, now),
        Err(ClipboardError::Unavailable)
    );
    assert_eq!(state.lock().unwrap().windows.len(), 1);
    assert_eq!(state.lock().unwrap().destroyed.len(), 1);
    state.lock().unwrap().owner = 42;
    session.begin_read(2000, now).unwrap();
    let request = *state.lock().unwrap().conversions.last().unwrap();
    assert_eq!(state.lock().unwrap().windows.len(), 2);
    assert_eq!(
        session.begin_read(2001, now),
        Err(ClipboardError::Unavailable)
    );
    assert_eq!(state.lock().unwrap().windows.len(), 2);
    assert_eq!(session.deadline(), Some(now + Duration::from_secs(10)));
    assert!(
        session
            .expire(now + Duration::from_secs(9))
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        session.expire(now + Duration::from_secs(10)).unwrap(),
        Some(Err(ClipboardError::Unavailable))
    ));
    assert_eq!(state.lock().unwrap().windows.len(), 1);
    assert!(session.deadline().is_none());
    assert!(
        session
            .expire(now + Duration::from_secs(11))
            .unwrap()
            .is_none()
    );

    for _ in 0..32 {
        session.begin_read(2001, now).unwrap();
        assert!(session.event(&denied(request), now).unwrap().is_none());
        let current = *state.lock().unwrap().conversions.last().unwrap();
        assert_ne!(current.0, request.0);
        assert!(matches!(
            session.event(&denied(current), now).unwrap(),
            Some(Err(ClipboardError::Unsupported))
        ));
        assert_eq!(state.lock().unwrap().windows.len(), 1);
    }
    assert_eq!(state.lock().unwrap().atoms.len(), atom_count);
}

#[test]
fn invalid_screen_does_not_create_a_window() {
    let (connection, state) = super::connection();
    assert!(matches!(
        Session::new(connection, 1),
        Err(ClipboardError::Unavailable)
    ));
    assert!(state.lock().unwrap().windows.is_empty());
}

#[test]
fn complete_reads_keep_the_source_and_destroy_only_their_request_window() {
    let (connection, state) = super::connection();
    let mut session = Session::new(connection, 0).unwrap();
    let owner = state.lock().unwrap().windows[0];
    session.request_time().unwrap();
    let time_event = state.lock().unwrap().events.pop_front().unwrap();
    assert_eq!(session.timestamp(&time_event), Some(2000));
    let data = Arc::new(ClipboardData::text(&"x".repeat(100_001)).unwrap());
    session.write(data.clone(), 1000).unwrap();
    let now = Instant::now();
    session.begin_read(2000, now).unwrap();
    let request_window = state.lock().unwrap().windows[1];
    let mut cursor = 0;
    let mut result = None;
    for _ in 0..100 {
        let conversion = state.lock().unwrap().conversions.get(cursor).copied();
        if let Some((requestor, selection, target, property, time)) = conversion {
            cursor += 1;
            assert!(
                session
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
                        now
                    )
                    .unwrap()
                    .is_none()
            );
        }
        loop {
            let event = state.lock().unwrap().events.pop_front();
            let Some(event) = event else {
                break;
            };
            if let Some(completion) = session.event(&event, now).unwrap() {
                assert!(result.is_none());
                result = Some(completion.unwrap());
            }
        }
        if result.is_some() {
            break;
        }
    }
    assert_eq!(result.unwrap().plain_text().unwrap(), "x".repeat(100_001));
    assert_eq!(state.lock().unwrap().windows, vec![owner]);
    assert_eq!(state.lock().unwrap().destroyed, vec![request_window]);
    assert_eq!(state.lock().unwrap().owner, owner);
    assert_eq!(Arc::strong_count(&data), 2);
    assert!(session.deadline().is_none());
    session.connection().flush().unwrap();
    session.begin_read(2001, now).unwrap();
    assert_ne!(state.lock().unwrap().windows[1], request_window);

    for _ in 0..100 {
        let request = state.lock().unwrap().conversions.get(cursor).copied();
        if let Some((requestor, selection, target, property, time)) = request {
            cursor += 1;
            session
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
                    now,
                )
                .unwrap();
        }
        if Arc::strong_count(&data) == 3 {
            state.lock().unwrap().owner = 42;
        }
        loop {
            let event = state.lock().unwrap().events.pop_front();
            let Some(event) = event else {
                break;
            };
            if let Some(completion) = session.event(&event, now).unwrap() {
                assert!(matches!(completion, Err(ClipboardError::Unavailable)));
                assert_eq!(Arc::strong_count(&data), 2);
                assert_eq!(state.lock().unwrap().windows, vec![owner]);
                assert!(session.deadline().is_none());
                return;
            }
        }
    }
    panic!("owner replacement during INCR must retire the read and its source transfer");
}

#[test]
fn request_window_destruction_completes_the_read_once() {
    let (connection, state) = super::connection();
    let mut session = Session::new(connection, 0).unwrap();
    state.lock().unwrap().owner = 42;
    let now = Instant::now();
    session.begin_read(2000, now).unwrap();
    let window = state.lock().unwrap().windows[1];
    let event = Event::DestroyNotify(DestroyNotifyEvent {
        response_type: DESTROY_NOTIFY_EVENT,
        sequence: 0,
        event: window,
        window,
    });
    assert!(matches!(
        session.event(&event, now).unwrap(),
        Some(Err(ClipboardError::Unavailable))
    ));
    assert!(session.event(&event, now).unwrap().is_none());
    assert_eq!(state.lock().unwrap().windows.len(), 1);
}
