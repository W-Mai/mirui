use super::*;
use crate::{
    prelude::*,
    ui::{
        clipboard::{self, ClipboardRequest},
        widgets::{TextEditing, TextInput, TextSelection},
    },
};
use std::os::unix::net::UnixStream;
use wayland_client::{
    Dispatch, EventQueue, Proxy, QueueHandle,
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_server::{
    Display, Resource,
    protocol::{wl_data_offer::WlDataOffer, wl_seat},
};

mod server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/wayland_clipboard/server.rs"
    ));
}

#[derive(Default)]
struct Host {
    compositor: Option<wl_compositor::WlCompositor>,
}
impl Dispatch<wl_registry::WlRegistry, ()> for Host {
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
            && interface == "wl_compositor"
        {
            state.compositor = Some(registry.bind(name, version.min(6), queue, ()));
        }
    }
}
wayland_client::delegate_noop!(Host: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(Host: ignore wl_surface::WlSurface);

struct Protocol {
    display: Display<server::Server>,
    server: server::Server,
    connection: Connection,
    queue: EventQueue<Host>,
    host: Host,
}

impl Protocol {
    fn new() -> (Self, Bridge) {
        let display = Display::new().unwrap();
        let mut handle = display.handle();
        handle.create_global::<server::Server, wayland_server::protocol::wl_data_device_manager::WlDataDeviceManager, _>(3, ());
        handle.create_global::<server::Server, wl_seat::WlSeat, _>(7, ());
        handle.create_global::<server::Server, wayland_server::protocol::wl_compositor::WlCompositor, _>(6, ());
        let (client, server) = UnixStream::pair().unwrap();
        handle.insert_client(server, Arc::new(())).unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut protocol = Self {
            display,
            server: server::Server::default(),
            connection,
            queue,
            host: Host::default(),
        };
        protocol.pump();
        protocol.pump();
        let surface = protocol
            .host
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&protocol.queue.handle(), ());
        let mut bridge = Bridge::new(protocol.connection.clone(), surface.id());
        for _ in 0..4 {
            protocol.pump();
            assert!(bridge.poll().is_none());
        }
        let id = protocol
            .server
            .globals
            .iter()
            .find(|(name, _)| name == "wl_seat")
            .unwrap()
            .1
            .clone();
        let seat = wl_seat::WlSeat::from_id(&protocol.display.handle(), id).unwrap();
        seat.capabilities(wl_seat::Capability::Keyboard);
        for _ in 0..3 {
            protocol.pump();
            assert!(bridge.poll().is_none());
        }
        protocol.server.keyboard.as_ref().unwrap().enter(
            10,
            &protocol.server.surfaces[0],
            Vec::new(),
        );
        protocol.pump();
        assert!(bridge.poll().is_none());
        (protocol, bridge)
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
        self.queue.dispatch_pending(&mut self.host).unwrap();
    }

    fn offer(&mut self, relay: bool) {
        let device = self.server.device.as_ref().unwrap();
        let source = relay.then(|| self.server.selection.clone().unwrap());
        let offer = device
            .client()
            .unwrap()
            .create_resource::<WlDataOffer, _, server::Server>(&self.display.handle(), 3, source)
            .unwrap();
        device.data_offer(&offer);
        offer.offer("text/plain;charset=utf-8".to_owned());
        device.selection(Some(&offer));
    }

    fn response(&mut self, bridge: &mut Bridge) -> ClipboardResponse {
        for _ in 0..1024 {
            self.pump();
            if let Some(response) = bridge.poll() {
                return response;
            }
        }
        panic!("missing clipboard response");
    }
}

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn app() -> (TestApp, Entity) {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let editing = TextEditing::default().with_text("native").unwrap();
    let node = app.compose(root, |cx| {
        ui! { TextInput () [editing] }
    });
    app.world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .set_selection(TextSelection {
            anchor: 0,
            cursor: 6,
        })
        .unwrap();
    (app, node)
}

fn cut(app: &mut TestApp, node: Entity) -> ClipboardWrite {
    clipboard::cut(&mut app.world, node);
    let Some(ClipboardRequest::Write(request)) = clipboard::take_request(&mut app.world) else {
        panic!("missing write");
    };
    request
}

fn text(app: &TestApp, node: Entity, expected: &str) {
    app.world
        .get::<TextEditing>(node)
        .unwrap()
        .read(|text| assert_eq!(text, expected));
}

#[test]
fn clipboard_cut_and_paste_complete_through_the_existing_app_service() {
    let (mut protocol, mut bridge) = Protocol::new();
    let (mut app, node) = app();
    assert!(matches!(
        bridge.write(cut(&mut app, node)),
        ClipboardDispatch::Pending
    ));
    text(&app, node, "native");
    assert!(bridge.poll().is_none());
    let response = protocol.response(&mut bridge);
    clipboard::complete(&mut app.world, &response);
    text(&app, node, "");
    protocol.offer(true);
    protocol.pump();
    assert!(bridge.poll().is_none());
    clipboard::paste(&mut app.world, node);
    let Some(ClipboardRequest::Read(request)) = clipboard::take_request(&mut app.world) else {
        panic!("missing read");
    };
    assert!(matches!(bridge.read(request), ClipboardDispatch::Pending));
    let response = protocol.response(&mut bridge);
    clipboard::complete(&mut app.world, &response);
    text(&app, node, "native");
    assert!(bridge.poll().is_none());
}

#[test]
fn clipboard_close_preserves_operation_identity_and_cannot_cut() {
    let (mut protocol, mut bridge) = Protocol::new();
    let (mut original, node) = app();
    assert!(matches!(
        bridge.write(cut(&mut original, node)),
        ClipboardDispatch::Pending
    ));
    let (mut other, other_node) = app();
    let other_write = cut(&mut other, other_node);
    let ClipboardDispatch::Complete(busy) = bridge.write(other_write) else {
        panic!("expected capacity rejection");
    };
    clipboard::complete(&mut other.world, &busy);
    text(&other, other_node, "native");
    let failed = bridge.close().unwrap();
    clipboard::complete(&mut other.world, &failed);
    clipboard::complete(&mut original.world, &failed);
    text(&original, node, "native");
    protocol.pump();
    assert!(bridge.poll().is_none());
    assert!(bridge.close().is_none());
    let ClipboardDispatch::Complete(failed) = bridge.write(cut(&mut original, node)) else {
        panic!("closed bridge accepted write");
    };
    clipboard::complete(&mut original.world, &failed);
    text(&original, node, "native");
}

#[test]
fn clipboard_pending_paste_rejects_changed_selection_and_close_cancels_pipe() {
    let (mut protocol, mut bridge) = Protocol::new();
    let (mut app, node) = app();
    protocol.offer(false);
    protocol.pump();
    assert!(bridge.poll().is_none());
    clipboard::paste(&mut app.world, node);
    let Some(ClipboardRequest::Read(request)) = clipboard::take_request(&mut app.world) else {
        panic!("missing read");
    };
    assert!(matches!(bridge.read(request), ClipboardDispatch::Pending));
    bridge.poll();
    protocol.pump();
    let pipe = protocol.server.pending.pop().unwrap();
    app.world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .set_selection(TextSelection {
            anchor: 1,
            cursor: 2,
        })
        .unwrap();
    assert_eq!(rustix::io::write(&pipe, b"changed").unwrap(), 7);
    drop(pipe);
    clipboard::complete(&mut app.world, &protocol.response(&mut bridge));
    text(&app, node, "native");
    clipboard::paste(&mut app.world, node);
    let Some(ClipboardRequest::Read(request)) = clipboard::take_request(&mut app.world) else {
        panic!("missing read");
    };
    assert!(matches!(bridge.read(request), ClipboardDispatch::Pending));
    bridge.poll();
    protocol.pump();
    let pipe = protocol.server.pending.pop().unwrap();
    clipboard::complete(&mut app.world, &bridge.close().unwrap());
    assert_eq!(
        rustix::io::write(&pipe, b"late"),
        Err(rustix::io::Errno::PIPE)
    );
    text(&app, node, "native");
}
