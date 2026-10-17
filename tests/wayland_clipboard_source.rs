#![cfg(any(target_os = "linux", target_os = "macos"))]

extern crate alloc;

pub mod ui {
    pub mod clipboard {
        pub use mirui::ui::clipboard::ClipboardError;
        mod data {
            include!("../src/ui/clipboard/data.rs");
        }
        pub use data::{ClipboardData, ClipboardFormat, ClipboardItem};
    }
}

#[path = "wayland_clipboard/server.rs"]
mod server;
#[path = "../src/surface/host_clipboard/wayland/source.rs"]
mod source;
#[path = "../src/surface/host_clipboard/wayland/transfer.rs"]
mod transfer;

use std::{
    os::fd::AsFd,
    os::unix::net::UnixStream,
    sync::Arc,
    time::{Duration, Instant},
};
use transfer::{Budget, Incoming};
use ui::clipboard::{ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    protocol::{wl_data_device, wl_data_device_manager, wl_data_source, wl_registry, wl_seat},
};
use wayland_server::{Display, Resource};

#[derive(Default)]
struct Client {
    manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    seat: Option<wl_seat::WlSeat>,
    source: source::Source,
    errors: Vec<ClipboardError>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_data_device_manager" => {
                    state.manager = Some(registry.bind(name, version.min(3), queue, ()))
                }
                "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), queue, ())),
                _ => (),
            }
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for Client {
    fn event(
        state: &mut Self,
        proxy: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Err(error) = state.source.event(proxy, event, Instant::now()) {
            state.errors.push(error);
        }
    }
}

wayland_client::delegate_noop!(Client: ignore wl_data_device_manager::WlDataDeviceManager);
wayland_client::delegate_noop!(Client: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(Client: ignore wl_data_device::WlDataDevice);

struct Fixture {
    display: Display<server::Server>,
    server: server::Server,
    connection: Connection,
    queue: EventQueue<Client>,
    client: Client,
    device: Option<wl_data_device::WlDataDevice>,
}

impl Fixture {
    fn new() -> Self {
        let display = Display::new().unwrap();
        let mut handle = display.handle();
        handle.create_global::<server::Server, wayland_server::protocol::wl_data_device_manager::WlDataDeviceManager, _>(3, ());
        handle.create_global::<server::Server, wayland_server::protocol::wl_seat::WlSeat, _>(7, ());
        let (client, server) = UnixStream::pair().unwrap();
        handle.insert_client(server, Arc::new(())).unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut fixture = Self {
            display,
            server: server::Server::default(),
            connection,
            queue,
            client: Client::default(),
            device: None,
        };
        for _ in 0..3 {
            fixture.pump();
        }
        fixture.device = Some(fixture.client.manager.as_ref().unwrap().get_data_device(
            fixture.client.seat.as_ref().unwrap(),
            &fixture.queue.handle(),
            (),
        ));
        fixture.pump();
        fixture
    }

    fn pump(&mut self) {
        self.connection.flush().unwrap();
        self.display.dispatch_clients(&mut self.server).unwrap();
        self.display.flush_clients().unwrap();
        if let Some(read) = self.connection.prepare_read() {
            match read.read() {
                Ok(_) => (),
                Err(wayland_client::backend::WaylandError::Io(error))
                    if error.kind() == std::io::ErrorKind::WouldBlock => {}
                error => panic!("unexpected transport result: {error:?}"),
            }
        }
        self.queue.dispatch_pending(&mut self.client).unwrap();
    }

    fn claim(&mut self, data: Arc<ClipboardData>, serial: u32) -> Result<(), ClipboardError> {
        self.client.source.claim(
            self.client.manager.as_ref().unwrap(),
            self.device.as_ref().unwrap(),
            serial,
            data,
            &self.queue.handle(),
        )?;
        self.pump();
        Ok(())
    }

    fn receive(&mut self, mime: &str) -> Incoming {
        let (reader, writer) = rustix::pipe::pipe().unwrap();
        self.server
            .selection
            .as_ref()
            .unwrap()
            .send(mime.to_owned(), writer.as_fd());
        self.display.flush_clients().unwrap();
        drop(writer);
        self.pump();
        Incoming::new(reader, Instant::now() + Duration::from_secs(20)).unwrap()
    }

    fn finish(&mut self, mut receive: Incoming) -> Vec<u8> {
        for _ in 0..1024 {
            let mut budget = Budget::new(65536, 32);
            assert!(
                self.client
                    .source
                    .poll(Instant::now(), &mut budget)
                    .is_none()
            );
            if receive.poll(Instant::now(), &mut budget).unwrap() {
                return receive.finish().unwrap();
            }
        }
        panic!("transfer did not reach EOF");
    }
}

fn binary(value: u8, length: usize) -> Arc<ClipboardData> {
    let mut item = ClipboardItem::with_capacity(2).unwrap();
    item.push(ClipboardFormat::new(
        "application/x-binary",
        vec![value; length],
    ))
    .unwrap();
    item.push(ClipboardFormat::borrowed("text/plain", b"sample\0text"))
        .unwrap();
    let mut data = ClipboardData::with_capacity(1, length + 11).unwrap();
    data.push(item).unwrap();
    Arc::new(data)
}

#[test]
fn source_announces_complete_representations_before_setting_selection() {
    let mut fixture = Fixture::new();
    fixture.claim(binary(9, 100000), 4321).unwrap();
    assert_eq!(fixture.server.serials, [4321]);
    assert_eq!(
        fixture.server.offered,
        [
            "application/x-binary",
            "text/plain",
            "text/plain;charset=utf-8"
        ]
    );
    let binary = fixture.receive("application/x-binary");
    assert_eq!(fixture.finish(binary), vec![9; 100000]);
    let text = fixture.receive("text/plain;charset=utf-8");
    assert_eq!(fixture.finish(text), b"sample\0text");
    assert!(fixture.client.errors.is_empty());
}

#[test]
fn replacement_preserves_accepted_transfers_and_retires_old_proxy() {
    let mut fixture = Fixture::new();
    let data = binary(2, 100000);
    let weak = Arc::downgrade(&data);
    fixture.claim(data, 42).unwrap();
    let old = fixture.server.selection.clone().unwrap();
    let reader = fixture.receive("application/x-binary");
    fixture.claim(binary(3, 3), 43).unwrap();
    assert!(!old.is_alive());
    assert!(weak.upgrade().is_some());
    assert_eq!(fixture.finish(reader), vec![2; 100000]);
    assert!(weak.upgrade().is_none());
    let reader = fixture.receive("application/x-binary");
    assert_eq!(fixture.finish(reader), [3; 3]);
    assert_eq!(fixture.server.serials, [42, 43]);
}

#[test]
fn cancellation_does_not_discard_already_accepted_bytes() {
    let mut fixture = Fixture::new();
    let data = binary(4, 100000);
    let weak = Arc::downgrade(&data);
    fixture.claim(data, 9).unwrap();
    let reader = fixture.receive("application/x-binary");
    let source = fixture.server.selection.clone().unwrap();
    source.cancelled();
    fixture.pump();
    fixture.pump();
    assert!(!source.is_alive());
    assert!(weak.upgrade().is_some());
    assert_eq!(fixture.finish(reader), vec![4; 100000]);
    assert!(weak.upgrade().is_none());
}

#[test]
fn unsupported_shape_or_text_does_not_replace_selection() {
    let mut fixture = Fixture::new();
    fixture.claim(binary(1, 1), 1).unwrap();
    let old = fixture.server.selection.clone();
    let item = |bytes: &'static [u8]| {
        let mut item = ClipboardItem::with_capacity(1).unwrap();
        item.push(ClipboardFormat::borrowed("text/plain", bytes))
            .unwrap();
        item
    };
    let mut invalid = ClipboardData::with_capacity(1, 1).unwrap();
    invalid.push(item(&[0xff])).unwrap();
    assert_eq!(
        fixture.claim(Arc::new(invalid), 2),
        Err(ClipboardError::InvalidData)
    );
    let mut multiple = ClipboardData::with_capacity(2, 2).unwrap();
    multiple.push(item(b"a")).unwrap();
    multiple.push(item(b"b")).unwrap();
    assert_eq!(
        fixture.claim(Arc::new(multiple), 3),
        Err(ClipboardError::Unsupported)
    );
    fixture.pump();
    assert_eq!(fixture.server.selection, old);
    assert_eq!(fixture.server.serials, [1]);
}

#[test]
fn source_bounds_slots_and_rotates_progress_without_extending_deadlines() {
    let metadata = core::mem::size_of::<source::Source>();
    assert!(metadata <= 2048);
    println!("Wayland source metadata: {metadata} bytes");
    let mut fixture = Fixture::new();
    fixture.claim(binary(5, 100000), 1).unwrap();
    let mut readers: Vec<_> = (0..16)
        .map(|_| fixture.receive("application/x-binary"))
        .collect();
    let extra = fixture.receive("application/x-binary");
    assert_eq!(fixture.client.errors, [ClipboardError::Capacity]);
    let mut extra = extra;
    assert!(extra.poll(Instant::now(), &mut Budget::new(1, 1)).unwrap());
    assert!(extra.finish().unwrap().is_empty());
    for reader in &mut readers {
        assert!(
            fixture
                .client
                .source
                .poll(Instant::now(), &mut Budget::new(1, 1))
                .is_none()
        );
        assert!(!reader.poll(Instant::now(), &mut Budget::new(2, 2)).unwrap());
    }
    let future = Instant::now() + Duration::from_secs(11);
    assert_eq!(
        fixture.client.source.poll(future, &mut Budget::new(0, 0)),
        Some(ClipboardError::Unavailable)
    );
    for mut reader in readers {
        assert!(reader.poll(Instant::now(), &mut Budget::new(2, 2)).unwrap());
        assert_eq!(reader.finish().unwrap(), [5]);
    }
    let reader = fixture.receive("application/x-binary");
    assert_eq!(fixture.finish(reader), vec![5; 100000]);
}

#[test]
fn unknown_format_and_close_release_descriptors_and_snapshot() {
    let mut fixture = Fixture::new();
    let data = binary(7, 100000);
    let weak = Arc::downgrade(&data);
    fixture.claim(data, 6).unwrap();
    let mut unknown = fixture.receive("application/x-absent");
    assert_eq!(fixture.client.errors, [ClipboardError::Unsupported]);
    assert!(
        unknown
            .poll(Instant::now(), &mut Budget::new(1, 1))
            .unwrap()
    );
    assert!(unknown.finish().unwrap().is_empty());
    let mut reader = fixture.receive("application/x-binary");
    drop(std::mem::take(&mut fixture.client.source));
    fixture.pump();
    assert!(!fixture.server.selection.as_ref().unwrap().is_alive());
    assert!(weak.upgrade().is_none());
    assert!(reader.poll(Instant::now(), &mut Budget::new(1, 1)).unwrap());
}
