use super::{
    Fixture, binary,
    read::{Offer, Read},
    transfer::Budget,
    ui::clipboard::ClipboardError,
};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
use wayland_server::{Resource, protocol::wl_data_offer::WlDataOffer};

impl Fixture {
    pub(super) fn offer(&mut self, names: &[&str], relay: bool) -> (Rc<Offer>, WlDataOffer) {
        let device = self.server.device.as_ref().unwrap();
        let source = relay.then(|| self.server.selection.clone().unwrap());
        let proxy = device
            .client()
            .unwrap()
            .create_resource::<WlDataOffer, _, super::server::Server>(
                &self.display.handle(),
                3,
                source,
            )
            .unwrap();
        device.data_offer(&proxy);
        for mime in names {
            proxy.offer((*mime).to_owned());
        }
        self.pump();
        let offer = self.client.offers.pop().unwrap().1;
        (Rc::new(offer), proxy)
    }

    fn supply(&mut self, bytes: &[u8]) {
        self.pump();
        let fd = self.server.pending.pop().unwrap();
        assert!(
            !rustix::fs::fcntl_getfl(&fd)
                .unwrap()
                .contains(rustix::fs::OFlags::NONBLOCK)
        );
        assert_eq!(rustix::io::write(&fd, bytes).unwrap(), bytes.len());
    }
}

#[test]
fn reads_complete_binary_snapshot_through_native_offer_and_source() {
    let mut fixture = Fixture::new();
    fixture.claim(binary(0, 100000), 10).unwrap();
    let (offer, proxy) = fixture.offer(
        &[
            "text/plain",
            "application/x-binary",
            "UTF8_STRING",
            "text/plain;charset=utf-8",
            "APPLICATION/X-BINARY",
            "not-a-mime",
        ],
        true,
    );
    let mut read = Read::begin(Rc::clone(&offer), Instant::now()).unwrap();
    let mut result = None;
    for _ in 0..1024 {
        fixture.pump();
        let mut budget = Budget::new(65536, 32);
        assert!(
            fixture
                .client
                .source
                .poll(Instant::now(), &mut budget)
                .is_none()
        );
        result = read.poll(Instant::now(), &mut budget).unwrap();
        if result.is_some() {
            break;
        }
    }
    let data = result.expect("all representations complete");
    assert_eq!(
        fixture.server.received,
        ["text/plain;charset=utf-8", "application/x-binary"]
    );
    assert_eq!(data.items()[0].formats().len(), 2);
    assert_eq!(data.items()[0].formats()[0].bytes(), b"sample\0text");
    assert_eq!(data.items()[0].formats()[1].bytes(), vec![0; 100000]);
    assert!(matches!(
        read.poll(Instant::now(), &mut Budget::new(1, 1)),
        Err(ClipboardError::InvalidData)
    ));
    drop(read);
    assert!(proxy.is_alive());
    drop(offer);
    fixture.pump();
    assert!(!proxy.is_alive());
}

#[test]
fn invalidation_is_permanent_and_discards_a_partial_snapshot() {
    let mut fixture = Fixture::new();
    let (offer, proxy) = fixture.offer(&["image/png", "text/plain"], false);
    let now = Instant::now();
    let mut read = Read::begin(Rc::clone(&offer), now).unwrap();
    fixture.supply(b"binary");
    assert!(read.poll(now, &mut Budget::new(100, 4)).unwrap().is_none());
    fixture.pump();
    let pending = fixture.server.pending.pop().unwrap();
    offer.invalidate();
    offer.invalidate();
    fixture.pump();
    assert!(!proxy.is_alive());
    assert!(matches!(
        read.poll(now, &mut Budget::new(100, 4)),
        Err(ClipboardError::Unavailable)
    ));
    assert_eq!(
        rustix::io::write(&pending, b"stale"),
        Err(rustix::io::Errno::PIPE)
    );
    assert!(matches!(
        Read::begin(Rc::clone(&offer), now),
        Err(ClipboardError::Unavailable)
    ));
    assert!(matches!(
        read.poll(now, &mut Budget::new(100, 4)),
        Err(ClipboardError::InvalidData)
    ));
    let (new, _) = fixture.offer(&["text/plain"], false);
    assert!(Read::begin(new, now).is_ok());
}

#[test]
fn invalid_text_rejects_the_whole_snapshot_without_rewriting_bytes() {
    let mut fixture = Fixture::new();
    let (offer, _) = fixture.offer(&["application/x-first", "text/plain"], false);
    let mut read = Read::begin(offer, Instant::now()).unwrap();
    fixture.supply(b"valid first representation");
    assert!(
        read.poll(Instant::now(), &mut Budget::new(100, 4))
            .unwrap()
            .is_none()
    );
    fixture.supply(&[0xff]);
    assert!(matches!(
        read.poll(Instant::now(), &mut Budget::new(100, 4)),
        Err(ClipboardError::InvalidData)
    ));
    assert!(matches!(
        read.poll(Instant::now(), &mut Budget::new(100, 4)),
        Err(ClipboardError::InvalidData)
    ));
}

#[test]
fn all_formats_share_one_deadline_and_timeout_closes_pending_fd() {
    let mut fixture = Fixture::new();
    let (offer, _) = fixture.offer(&["image/png", "text/plain"], false);
    let now = Instant::now();
    let mut read = Read::begin(offer, now).unwrap();
    fixture.supply(&[]);
    assert!(
        read.poll(now + Duration::from_secs(9), &mut Budget::new(100, 4))
            .unwrap()
            .is_none()
    );
    fixture.pump();
    let pending = fixture.server.pending.pop().unwrap();
    assert!(matches!(
        read.poll(now + Duration::from_secs(10), &mut Budget::new(100, 4)),
        Err(ClipboardError::Unavailable)
    ));
    assert_eq!(
        rustix::io::write(&pending, b"late"),
        Err(rustix::io::Errno::PIPE)
    );
}

#[test]
fn unsupported_offers_do_not_start_transfers_and_drop_cancels_pending_reads() {
    let mut fixture = Fixture::new();
    let (offer, _) = fixture.offer(&["metadata", "text/plain;charset=unknown"], false);
    assert!(matches!(
        Read::begin(offer, Instant::now()),
        Err(ClipboardError::Unsupported)
    ));
    fixture.pump();
    assert!(fixture.server.received.is_empty());
    let (offer, proxy) = fixture.offer(&["application/x-binary"], false);
    let read = Read::begin(offer, Instant::now()).unwrap();
    fixture.pump();
    let pending = fixture.server.pending.pop().unwrap();
    drop(read);
    fixture.pump();
    assert!(!proxy.is_alive());
    assert_eq!(
        rustix::io::write(&pending, b"cancelled"),
        Err(rustix::io::Errno::PIPE)
    );
}
