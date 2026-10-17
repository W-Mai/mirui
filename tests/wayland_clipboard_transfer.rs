#![cfg(any(target_os = "linux", target_os = "macos"))]

pub use mirui::ui;

#[path = "../src/surface/host_clipboard/wayland/transfer.rs"]
mod transfer;

use std::{
    os::fd::OwnedFd,
    sync::Arc,
    time::{Duration, Instant},
};

use rustix::{
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    io::{Errno, read, write},
    pipe::pipe,
};
use transfer::{Budget, Incoming, Outgoing};
use ui::clipboard::{ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem};

fn snapshot(bytes: Vec<u8>) -> Arc<ClipboardData> {
    let mut data = ClipboardData::with_capacity(1, bytes.len()).unwrap();
    let mut item = ClipboardItem::with_capacity(1).unwrap();
    item.push(ClipboardFormat::new("application/x-pipe-test", bytes))
        .unwrap();
    data.push(item).unwrap();
    Arc::new(data)
}

fn nonblocking(fd: &OwnedFd) {
    let flags = fcntl_getfl(fd).unwrap();
    fcntl_setfl(fd, flags | OFlags::NONBLOCK).unwrap();
}

fn deadline() -> Instant {
    Instant::now() + transfer::TIMEOUT
}

#[test]
fn binary_payloads_preserve_eof_and_shared_snapshot_storage() {
    for size in [0, 1, 8192, 262144] {
        let bytes: Vec<u8> = (0..size).map(|n| (n % 251) as u8).chain([0, 0]).collect();
        let original = snapshot(bytes);
        let expected = original.items()[0].formats()[0].bytes();
        let weak = Arc::downgrade(&original);
        let (reader, writer) = pipe().unwrap();
        let end = deadline();
        let mut send = Outgoing::new(writer, Arc::clone(&original), 0, end).unwrap();
        let mut receive = Incoming::new(reader, end).unwrap();
        assert_eq!(Arc::strong_count(&original), 2);

        let mut completed = false;
        for _ in 0..1024 {
            // Each event-loop turn shares its work limit across both directions.
            let mut budget = Budget::new(16384, 8);
            send.poll(Instant::now(), &mut budget).unwrap();
            if receive.poll(Instant::now(), &mut budget).unwrap() {
                completed = true;
                break;
            }
        }
        assert!(completed);
        assert_eq!(receive.finish().unwrap(), expected);
        assert!(send.poll(end, &mut Budget::new(0, 0)).unwrap());
        drop(original);
        assert!(weak.upgrade().is_some());
        drop(send);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn only_eof_completes_a_read_including_empty_content() {
    let end = deadline();
    let (reader, writer) = pipe().unwrap();
    let mut receive = Incoming::new(reader, end).unwrap();
    assert!(
        !receive
            .poll(Instant::now(), &mut Budget::new(8, 8))
            .unwrap()
    );
    assert_eq!(write(&writer, b"part\0").unwrap(), 5);
    assert!(
        !receive
            .poll(Instant::now(), &mut Budget::new(8, 8))
            .unwrap()
    );
    assert_eq!(receive.finish(), Err(ClipboardError::Unavailable));

    let (reader, writer) = pipe().unwrap();
    let mut receive = Incoming::new(reader, end).unwrap();
    let mut send = Outgoing::new(writer, snapshot(Vec::new()), 0, end).unwrap();
    assert!(send.poll(Instant::now(), &mut Budget::new(0, 0)).unwrap());
    assert!(
        receive
            .poll(Instant::now(), &mut Budget::new(1, 1))
            .unwrap()
    );
    assert!(receive.finish().unwrap().is_empty());
}

#[test]
fn byte_and_syscall_budgets_bound_shared_work() {
    let end = deadline();
    let (reader, writer) = pipe().unwrap();
    nonblocking(&reader);
    let mut send = Outgoing::new(writer, snapshot(vec![7; 1024]), 0, end).unwrap();
    let mut budget = Budget::new(17, 1);
    assert!(!send.poll(Instant::now(), &mut budget).unwrap());
    assert!(!send.poll(Instant::now(), &mut budget).unwrap());
    let mut bytes = [0; 1024];
    assert_eq!(read(&reader, &mut bytes).unwrap(), 17);
    assert_eq!(read(&reader, &mut bytes), Err(Errno::AGAIN));
    assert!(!send.poll(Instant::now(), &mut Budget::new(512, 0)).unwrap());
    assert_eq!(read(&reader, &mut bytes), Err(Errno::AGAIN));

    let (reader, writer) = pipe().unwrap();
    let mut receive = Incoming::new(reader, end).unwrap();
    write(&writer, b"abc").unwrap();
    drop(writer);
    let mut budget = Budget::new(1, 1);
    assert!(!receive.poll(Instant::now(), &mut budget).unwrap());
    assert!(!receive.poll(Instant::now(), &mut budget).unwrap());
    assert!(
        receive
            .poll(Instant::now(), &mut Budget::new(3, 2))
            .unwrap()
    );
    assert_eq!(receive.finish().unwrap(), b"abc");
}

#[test]
fn stalled_source_expires_and_releases_the_pipe_without_waiting() {
    let end = deadline();
    let (reader, writer) = pipe().unwrap();
    nonblocking(&reader);
    let mut send = Outgoing::new(writer, snapshot(vec![0x83; 1_048_576]), 0, end).unwrap();
    let mut budget = Budget::new(2_097_152, 32);
    assert!(!send.poll(Instant::now(), &mut budget).unwrap());
    assert!(!send.poll(Instant::now(), &mut budget).unwrap());
    assert_eq!(
        send.poll(end, &mut Budget::new(0, 0)),
        Err(ClipboardError::Unavailable)
    );
    assert_eq!(
        send.poll(end - Duration::from_secs(1), &mut Budget::new(8, 8)),
        Err(ClipboardError::Unavailable)
    );
    let mut bytes = [0; 8192];
    loop {
        if read(&reader, &mut bytes).unwrap() == 0 {
            break;
        }
    }
}

#[test]
fn receiving_progress_does_not_extend_the_absolute_deadline() {
    let end = deadline();
    let (reader, writer) = pipe().unwrap();
    let mut receive = Incoming::new(reader, end).unwrap();
    write(&writer, b"incomplete").unwrap();
    assert!(
        !receive
            .poll(end - Duration::from_millis(1), &mut Budget::new(20, 2))
            .unwrap()
    );
    assert_eq!(
        receive.poll(end, &mut Budget::new(20, 2)),
        Err(ClipboardError::Unavailable)
    );
    assert_eq!(write(&writer, b"too late"), Err(Errno::PIPE));
    assert_eq!(
        receive.poll(end - Duration::from_secs(1), &mut Budget::new(20, 2)),
        Err(ClipboardError::Unavailable)
    );
    assert_eq!(receive.finish(), Err(ClipboardError::Unavailable));
}

#[test]
fn peer_failure_and_drop_close_only_the_owned_transfer() {
    let end = deadline();
    let (reader, writer) = pipe().unwrap();
    let mut send = Outgoing::new(writer, snapshot(vec![1]), 0, end).unwrap();
    drop(reader);
    assert_eq!(
        send.poll(Instant::now(), &mut Budget::new(8, 8)),
        Err(ClipboardError::Platform)
    );
    assert_eq!(
        send.poll(Instant::now(), &mut Budget::new(8, 8)),
        Err(ClipboardError::Platform)
    );

    let (reader, writer) = pipe().unwrap();
    let receive = Incoming::new(reader, end).unwrap();
    drop(receive);
    assert_eq!(write(&writer, b"closed"), Err(Errno::PIPE));

    let (reader, writer) = pipe().unwrap();
    nonblocking(&reader);
    let send = Outgoing::new(writer, snapshot(vec![1]), 0, end).unwrap();
    drop(send);
    assert_eq!(read(&reader, &mut [0; 1]).unwrap(), 0);

    let (reader, writer) = pipe().unwrap();
    write(&writer, b"other").unwrap();
    let mut bytes = [0; 5];
    assert_eq!(read(&reader, &mut bytes).unwrap(), 5);
    assert_eq!(&bytes, b"other");
}

#[test]
fn invalid_shape_or_format_cannot_leave_an_open_sender() {
    let (reader, writer) = pipe().unwrap();
    assert!(matches!(
        Outgoing::new(writer, snapshot(vec![1]), 1, deadline()),
        Err(ClipboardError::InvalidData)
    ));
    assert_eq!(read(&reader, &mut [0; 1]).unwrap(), 0);

    let (reader, writer) = pipe().unwrap();
    assert!(matches!(
        Outgoing::new(
            writer,
            Arc::new(ClipboardData::borrowed(&[])),
            0,
            deadline()
        ),
        Err(ClipboardError::Unsupported)
    ));
    assert_eq!(read(&reader, &mut [0; 1]).unwrap(), 0);
}
