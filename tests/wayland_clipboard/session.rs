use super::{
    Fixture, binary,
    session::{Completion, Session},
    ui::clipboard::ClipboardError,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use wayland_client::Proxy;
use wayland_server::{
    Resource,
    protocol::{wl_data_offer::WlDataOffer, wl_seat},
};

impl Fixture {
    fn session(&mut self) -> Session {
        let target = self
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&self.queue.handle(), ());
        self.pump();
        let mut session = Session::new(self.connection.clone(), target.id());
        for _ in 0..4 {
            self.session_pump(&mut session);
        }
        let id = self
            .server
            .globals
            .iter()
            .rev()
            .find(|(name, _)| name == "wl_seat")
            .unwrap()
            .1
            .clone();
        let seat = wl_seat::WlSeat::from_id(&self.display.handle(), id).unwrap();
        seat.capabilities(wl_seat::Capability::Keyboard | wl_seat::Capability::Pointer);
        for _ in 0..3 {
            self.session_pump(&mut session);
        }
        self.server
            .keyboard
            .as_ref()
            .unwrap()
            .enter(77, &self.server.surfaces[0], Vec::new());
        self.session_pump(&mut session);
        session
    }

    fn session_pump(
        &mut self,
        session: &mut Session,
    ) -> Option<Result<Completion, ClipboardError>> {
        self.pump();
        session.poll(Instant::now()).unwrap()
    }

    fn selection(&mut self, names: &[&str], relay: bool) -> WlDataOffer {
        let device = self.server.device.as_ref().unwrap();
        let source = relay.then(|| self.server.selection.clone().unwrap());
        let offer = device
            .client()
            .unwrap()
            .create_resource::<WlDataOffer, _, super::server::Server>(
                &self.display.handle(),
                3,
                source,
            )
            .unwrap();
        device.data_offer(&offer);
        for name in names {
            offer.offer((*name).to_owned());
        }
        device.selection(Some(&offer));
        offer
    }
}

#[test]
fn session_confirms_write_after_host_dispatch_and_reads_all_formats_once() {
    let mut f = Fixture::new();
    let mut session = f.session();
    let data = binary(7, 150000);
    session.write(Arc::clone(&data), Instant::now()).unwrap();
    assert_eq!(session.read(Instant::now()), Err(ClipboardError::Capacity));
    assert!(session.poll(Instant::now()).unwrap().is_none());
    assert!(matches!(
        f.session_pump(&mut session),
        Some(Ok(Completion::Written))
    ));
    assert_eq!(f.server.serials, [77]);
    let offer = f.selection(&["text/plain;charset=utf-8", "application/x-binary"], true);
    f.session_pump(&mut session);
    session.read(Instant::now()).unwrap();
    let mut completed = None;
    for _ in 0..1024 {
        if let Some(result) = f.session_pump(&mut session) {
            completed = Some(result);
            break;
        }
    }
    let Completion::Read(read) = completed.unwrap().unwrap() else {
        panic!("expected read");
    };
    for actual in read.items()[0].formats() {
        let expected = data.items()[0]
            .formats()
            .iter()
            .find(|format| format.mime() == actual.mime())
            .unwrap();
        assert_eq!(actual.bytes(), expected.bytes());
    }
    assert_eq!(read.items()[0].formats().len(), 2);
    assert!(session.poll(Instant::now()).unwrap().is_none());
    assert!(offer.is_alive());
}

#[test]
fn latest_authorized_seat_owns_requests_and_removed_seat_cannot_complete_write() {
    let mut f = Fixture::new();
    let mut session = f.session();
    let first_keyboard = f.server.keyboard.clone().unwrap();
    let target = f.server.surfaces[0].clone();
    let global = f
        .display
        .handle()
        .create_global::<super::server::Server, wl_seat::WlSeat, _>(7, ());
    for _ in 0..3 {
        f.session_pump(&mut session);
    }
    let id = f
        .server
        .globals
        .iter()
        .rev()
        .find(|(name, _)| name == "wl_seat")
        .unwrap()
        .1
        .clone();
    let seat = wl_seat::WlSeat::from_id(&f.display.handle(), id).unwrap();
    seat.capabilities(wl_seat::Capability::Keyboard);
    for _ in 0..3 {
        f.session_pump(&mut session);
    }
    f.server
        .keyboard
        .as_ref()
        .unwrap()
        .enter(99, &target, Vec::new());
    f.session_pump(&mut session);
    session.write(binary(1, 4), Instant::now()).unwrap();
    session.poll(Instant::now()).unwrap();
    assert!(matches!(
        f.session_pump(&mut session),
        Some(Ok(Completion::Written))
    ));
    assert_eq!(f.server.serials.last(), Some(&99));
    session.write(binary(2, 4), Instant::now()).unwrap();
    session.poll(Instant::now()).unwrap();
    f.display
        .handle()
        .remove_global::<super::server::Server>(global);
    assert!(matches!(
        f.session_pump(&mut session),
        Some(Err(ClipboardError::Unavailable))
    ));
    assert_eq!(
        session.write(binary(3, 4), Instant::now()),
        Err(ClipboardError::PermissionDenied)
    );
    first_keyboard.key(
        100,
        0,
        30,
        wayland_server::protocol::wl_keyboard::KeyState::Pressed,
    );
    f.session_pump(&mut session);
    session.write(binary(4, 4), Instant::now()).unwrap();
    session.poll(Instant::now()).unwrap();
    assert!(matches!(
        f.session_pump(&mut session),
        Some(Ok(Completion::Written))
    ));
    assert_eq!(f.server.serials.last(), Some(&100));
}

#[test]
fn cancellation_after_sync_but_before_delivery_rejects_the_write() {
    let mut f = Fixture::new();
    let mut session = f.session();
    session.write(binary(1, 4), Instant::now()).unwrap();
    session.poll(Instant::now()).unwrap();
    f.pump();
    f.server.selection.as_ref().unwrap().cancelled();
    f.pump();
    assert!(matches!(
        session.poll(Instant::now()).unwrap(),
        Some(Err(ClipboardError::Unavailable))
    ));
    assert!(session.poll(Instant::now()).unwrap().is_none());
}

#[test]
fn replacing_selection_aborts_read_and_old_sync_does_not_complete_new_write() {
    let mut f = Fixture::new();
    let mut session = f.session();
    f.selection(&["text/plain"], false);
    f.session_pump(&mut session);
    session.read(Instant::now()).unwrap();
    session.poll(Instant::now()).unwrap();
    f.pump();
    let old_pipe = f.server.pending.pop().unwrap();
    f.selection(&["application/x-new"], false);
    assert!(matches!(
        f.session_pump(&mut session),
        Some(Err(ClipboardError::Unavailable))
    ));
    assert_eq!(
        rustix::io::write(&old_pipe, b"old"),
        Err(rustix::io::Errno::PIPE)
    );
    session
        .write(binary(1, 4), Instant::now() - Duration::from_secs(20))
        .unwrap();
    assert!(matches!(
        session.poll(Instant::now()).unwrap(),
        Some(Err(ClipboardError::Unavailable))
    ));
    session.write(binary(2, 5), Instant::now()).unwrap();
    assert!(session.poll(Instant::now()).unwrap().is_none());
    assert!(matches!(
        f.session_pump(&mut session),
        Some(Ok(Completion::Written))
    ));
    assert!(session.poll(Instant::now()).unwrap().is_none());
}

#[test]
fn session_drop_cancels_transfers_without_closing_host_connection() {
    let mut f = Fixture::new();
    let mut session = f.session();
    let offer = f.selection(&["text/plain"], false);
    f.session_pump(&mut session);
    session.read(Instant::now()).unwrap();
    session.poll(Instant::now()).unwrap();
    f.pump();
    let pipe = f.server.pending.pop().unwrap();
    let device = f.server.device.clone().unwrap();
    let keyboard = f.server.keyboard.clone().unwrap();
    drop(session);
    f.pump();
    assert!(!offer.is_alive());
    assert!(!device.is_alive());
    assert!(!keyboard.is_alive());
    assert_eq!(
        rustix::io::write(&pipe, b"late"),
        Err(rustix::io::Errno::PIPE)
    );
    assert!(f.server.surfaces[0].is_alive());
    let _surface = f
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&f.queue.handle(), ());
    f.pump();
    assert_eq!(f.server.surfaces.len(), 2);
}
