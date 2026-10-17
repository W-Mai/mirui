#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::{
    io::{self, IoSlice, Read, Write},
    os::unix::net::UnixStream,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    protocol::xproto::{ConnectionExt, Setup},
    rust_connection::{DefaultStream, PollMode, RustConnection, Stream},
};

#[path = "../src/surface/host_clipboard/x11/stream.rs"]
mod stream;

use stream::{BoundedStream, Cancel};

fn pair() -> (BoundedStream, Cancel, UnixStream) {
    let (socket, peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (socket, _) = DefaultStream::from_unix_stream(socket).unwrap();
    let (socket, cancel) = BoundedStream::new(socket).unwrap();
    (socket, cancel, peer)
}

#[test]
fn cancellation_interrupts_x11_handshake_without_a_server_reply() {
    let (stream, cancel, mut peer) = pair();
    let (send, receive) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let result = RustConnection::connect_to_stream(stream, 0);
        send.send(result.is_err()).unwrap();
    });
    let mut header = [0; 12];
    peer.read_exact(&mut header).unwrap();
    assert_eq!(u16::from_ne_bytes([header[2], header[3]]), 11);
    drop(cancel);
    assert!(receive.recv_timeout(Duration::from_secs(2)).unwrap());
    worker.join().unwrap();
}

#[test]
fn cancellation_interrupts_a_pending_x11_reply() {
    let (stream, cancel, mut peer) = pair();
    let (send, receive) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let connection = RustConnection::for_connected_stream(
            stream,
            Setup {
                resource_id_base: 0x10000,
                resource_id_mask: 0xffff,
                maximum_request_length: u16::MAX,
                ..Default::default()
            },
        )
        .unwrap();
        let result = connection.get_input_focus().unwrap().reply();
        send.send(result.is_err()).unwrap();
    });
    let mut request = [0; 4];
    peer.read_exact(&mut request).unwrap();
    assert_eq!(request[0], x11rb::protocol::xproto::GET_INPUT_FOCUS_REQUEST);
    drop(cancel);
    assert!(receive.recv_timeout(Duration::from_secs(2)).unwrap());
    worker.join().unwrap();
}

#[test]
fn cancellation_interrupts_write_backpressure() {
    let (stream, cancel, _peer) = pair();
    let mut fds = Vec::new();
    loop {
        match stream.write(&[1; 65536], &mut fds) {
            Ok(length) => assert_ne!(length, 0),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            other => panic!("unexpected write result: {other:?}"),
        }
    }
    let (send, receive) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        send.send(stream.poll(PollMode::Writable).is_err()).unwrap();
    });
    drop(cancel);
    assert!(receive.recv_timeout(Duration::from_secs(2)).unwrap());
    worker.join().unwrap();
}

#[test]
fn deadline_bounds_poll_and_cannot_revive_a_failed_connection() {
    let (stream, _cancel, mut peer) = pair();
    let deadline = Instant::now() + Duration::from_millis(30);
    stream.set_deadline(Some(deadline));
    peer.write_all(b"x").unwrap();
    stream.poll(PollMode::Readable).unwrap();
    assert_eq!(stream.read(&mut [0; 1], &mut Vec::new()).unwrap(), 1);
    assert_eq!(
        stream.poll(PollMode::Readable).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(Instant::now() >= deadline);
    stream.set_deadline(None);
    peer.write_all(b"still pending").unwrap();
    assert_eq!(
        stream
            .read(&mut [0; 16], &mut Vec::new())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(
        stream.write(b"x", &mut Vec::new()).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(
        stream
            .write_vectored(&[IoSlice::new(b"x")], &mut Vec::new())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn vectored_data_and_descriptors_use_the_existing_transport() {
    let (stream, _cancel, peer) = pair();
    let (peer, _) = DefaultStream::from_unix_stream(peer).unwrap();
    let (sent, mut witness) = UnixStream::pair().unwrap();
    let mut descriptors = vec![sent.into()];
    assert_eq!(
        stream
            .write_vectored(&[IoSlice::new(b"a"), IoSlice::new(b"b")], &mut descriptors)
            .unwrap(),
        2
    );
    assert!(descriptors.is_empty());
    let mut bytes = [0; 2];
    assert_eq!(peer.read(&mut bytes, &mut descriptors).unwrap(), 2);
    assert_eq!(bytes, *b"ab");
    assert_eq!(descriptors.len(), 1);
    let mut received = UnixStream::from(descriptors.pop().unwrap());
    received.write_all(b"fd retained").unwrap();
    let mut message = [0; 11];
    witness.read_exact(&mut message).unwrap();
    assert_eq!(&message, b"fd retained");
}

#[test]
fn closing_one_owned_connection_does_not_cancel_another() {
    let (closed, cancel, _peer) = pair();
    let (live, _live_cancel, mut live_peer) = pair();
    drop(cancel);
    assert!(closed.poll(PollMode::ReadAndWritable).is_err());
    closed.set_deadline(None);
    assert!(closed.write(b"x", &mut Vec::new()).is_err());
    live_peer.write_all(b"live").unwrap();
    live.poll(PollMode::Readable).unwrap();
    let mut message = [0; 4];
    assert_eq!(live.read(&mut message, &mut Vec::new()).unwrap(), 4);
    assert_eq!(&message, b"live");
}

#[test]
fn connection_teardown_closes_the_socket_even_when_cancellation_is_retained() {
    let (stream, _cancel, mut peer) = pair();
    drop(stream);
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
}
