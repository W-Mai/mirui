use super::*;
use alloc::string::String;
use std::sync::Mutex;

pub(crate) static SERIAL: Mutex<()> = Mutex::new(());

fn event(name: &'static str, depth: u8) -> PerfEvent {
    PerfEvent {
        name,
        start_ns: 1001,
        end_ns: 1002,
        sequence: 0,
        depth,
    }
}

#[test]
fn bounded_postorder_records_distinguish_loss_zero_time_and_bad_clocks() {
    let mut buffer = buffer::Buffer::new(2).unwrap();
    let parent = buffer.enter().unwrap();
    let child = buffer.enter().unwrap();
    assert!(buffer.exit(event("same", child), true));
    assert!(buffer.exit(event("same", parent), true));
    let depth = buffer.enter().unwrap();
    assert!(!buffer.exit(event("full", depth), true));
    let (len, status) = buffer.snapshot();
    assert_eq!((len, status.dropped, status.open_scopes), (2, 1, 0));
    let mut records = [PerfEvent::EMPTY; 2];
    assert_eq!(buffer.read(&mut records), 2);
    assert_eq!((records[0].depth, records[1].depth), (1, 0));
    assert_eq!((records[0].sequence, records[1].sequence), (1, 2));
    let depth = buffer.enter().unwrap();
    let mut zero = event("zero", depth);
    zero.end_ns = zero.start_ns;
    assert!(buffer.exit(zero, true));
    let depth = buffer.enter().unwrap();
    assert!(!buffer.exit(event("missing", depth), false));
    let depth = buffer.enter().unwrap();
    let mut backwards = event("backwards", depth);
    backwards.end_ns = 0;
    assert!(!buffer.exit(backwards, true));
    let (_, status) = buffer.snapshot();
    assert_eq!(status.invalid, 2);
    assert_eq!(buffer.read(&mut records), 1);
    assert_eq!(records[0].sequence, 4);
}

#[test]
fn capture_epochs_reject_old_guards_and_second_consumers() {
    let _serial = SERIAL.lock().unwrap();
    let disabled = enter("disabled");
    let mut capture = Capture::new(8192).unwrap();
    assert!(matches!(Capture::new(8192), Err(CaptureError::InUse)));
    assert!(matches!(Capture::new(1), Err(CaptureError::Capacity)));
    let old = enter("old");
    drop(disabled);
    capture.drain_events(|_, _| {});
    assert_eq!(capture.status().open_scopes, 1);
    capture.stop();
    let first = capture.id();
    drop(capture);
    let mut next = Capture::new(8192).unwrap();
    assert_ne!(first, next.id());
    drop(old);
    {
        let _span = enter("new");
    }
    let mut events = Vec::new();
    next.drain_events(|_, batch| events.extend_from_slice(batch));
    assert_eq!(events.len(), 1);
    assert_eq!((events[0].name, events[0].depth), ("new", 0));
    assert_eq!(next.status().invalid, 0);
}

#[test]
fn registered_threads_drain_once_and_sinks_run_outside_locks() {
    let _serial = SERIAL.lock().unwrap();
    let mut capture = Capture::new(8192).unwrap();
    std::thread::spawn(|| {
        register_thread().unwrap();
        let _parent = enter("worker");
        let _child = enter("child");
    })
    .join()
    .unwrap();
    std::thread::spawn(|| {
        let _ignored = enter("unregistered");
    })
    .join()
    .unwrap();
    let mut worker_tracks = Vec::new();
    capture.drain_events(|info, events| {
        for event in events {
            worker_tracks.push((info.track, event.name));
        }
        // This must not deadlock or be consumed repeatedly by the same snapshot.
        let _sink = enter("sink");
    });
    assert_eq!(worker_tracks.len(), 2);
    assert_eq!(worker_tracks[0].0, worker_tracks[1].0);
    assert_eq!(capture.status().registered_tracks, 2);
    assert!(capture.status().dropped >= 1);
    let mut next = Vec::new();
    capture.drain_events(|_, events| next.extend(events.iter().map(|e| e.name)));
    assert!(next.iter().all(|&name| name == "sink"));
}

#[test]
fn recording_and_export_reuse_the_budget_without_hot_allocations() {
    let _serial = SERIAL.lock().unwrap();
    let mut capture = Capture::new(8192).unwrap();
    assert!(capture.accounted_bytes_per_thread() <= 8192);
    {
        let _warm = enter("warm");
    }
    capture.drain(Some(&mut |_| {}));
    let capacity = capture.event_capacity();
    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..capacity + 5 {
            let _span = enter("bounded");
        }
        capture.drain(Some(&mut |_| {}));
        capture.sort_stage_stats();
    });
    assert_eq!(allocations, 0);
    assert!(capture.status().dropped >= 5);
    assert_eq!(
        capture
            .stage_stats()
            .iter()
            .find(|s| s.name == "bounded")
            .unwrap()
            .count as usize,
        capacity
    );
}

#[test]
fn fractional_timestamps_and_names_are_lossless_json() {
    let info = BatchInfo {
        capture: 7,
        track: 2,
        process: 19,
        registered_tracks: 2,
        status: TraceStatus::default(),
    };
    let mut text = String::new();
    format_chrome_event(info, &event("a\"b\\c\nd\x01", 1), &mut text).unwrap();
    assert!(text.contains(r#""ts":1.001,"dur":0.001"#));
    assert!(text.contains(r#""pid":19,"tid":2"#));
    assert!(text.contains(r#"a\"b\\c\nd\u0001"#));
    assert!(!text.contains('\n'));
}

#[test]
fn oversized_records_cannot_expand_or_leave_partial_json_in_export() {
    let _serial = SERIAL.lock().unwrap();
    const LONG: &str = match core::str::from_utf8(&[b'x'; 4096]) {
        Ok(s) => s,
        Err(_) => panic!(),
    };
    let mut capture = Capture::new(8192).unwrap();
    {
        let _span = enter(LONG);
    }
    {
        let _span = enter("short");
    }
    let mut output = String::new();
    capture.drain(Some(&mut |batch| output.push_str(batch)));
    assert_eq!(capture.status().encoding_dropped, 1);
    assert!(
        output
            .lines()
            .all(|line| line.starts_with('{') && line.ends_with('}'))
    );
    assert!(output.contains(r#""name":"short""#));
    assert!(output.contains(r#""encoding_dropped":1"#));
    assert!(!output.contains("xxxxxxxx"));
}

#[test]
fn depth_overflow_and_non_lifo_guards_mark_incomplete_instead_of_wrapping() {
    let mut buffer = buffer::Buffer::new(2).unwrap();
    for expected in 0..u8::MAX {
        assert_eq!(buffer.enter(), Some(expected));
    }
    assert_eq!(buffer.enter(), None);
    assert_eq!(buffer.snapshot().1.invalid, 1);
    assert!(buffer.exit(event("out-of-order", 0), true));
    assert_eq!(buffer.snapshot().1.invalid, 1);
}
