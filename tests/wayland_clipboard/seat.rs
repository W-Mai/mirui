use super::{Fixture, read::Read, seat::Seat, ui::clipboard::ClipboardError};
use std::{rc::Rc, time::Instant};
use wayland_client::Proxy;
use wayland_server::{
    Resource,
    protocol::{wl_keyboard, wl_pointer, wl_seat, wl_surface},
};

impl Fixture {
    fn enable_input(&mut self) -> wl_seat::WlSeat {
        let compositor = self.client.compositor.as_ref().unwrap();
        let target = compositor.create_surface(&self.queue.handle(), ());
        let _other = compositor.create_surface(&self.queue.handle(), ());
        self.client.input = Some(Seat::new(self.client.seat.clone().unwrap(), target.id()));
        self.pump();
        let id = self
            .server
            .globals
            .iter()
            .find(|(name, _)| name == "wl_seat")
            .unwrap()
            .1
            .clone();
        let seat = wl_seat::WlSeat::from_id(&self.display.handle(), id).unwrap();
        seat.capabilities(
            wl_seat::Capability::Keyboard
                | wl_seat::Capability::Pointer
                | wl_seat::Capability::Touch,
        );
        self.pump();
        self.pump();
        seat
    }

    fn serial(&self) -> Result<u32, ClipboardError> {
        self.client.input.as_ref().unwrap().write_serial()
    }

    fn enter(&mut self, serial: u32, surface: &wl_surface::WlSurface) {
        self.server
            .keyboard
            .as_ref()
            .unwrap()
            .enter(serial, surface, Vec::new());
        self.pump();
    }
}

#[test]
fn only_target_focused_input_authorizes_selection_and_zero_serial_is_valid() {
    let mut f = Fixture::new();
    f.enable_input();
    let target = f.server.surfaces[0].clone();
    let other = f.server.surfaces[1].clone();
    let keyboard = f.server.keyboard.clone().unwrap();
    let pointer = f.server.pointer.clone().unwrap();
    let touch = f.server.touch.clone().unwrap();
    keyboard.modifiers(1, 0, 0, 0, 0);
    pointer.enter(2, &target, 0.0, 0.0);
    pointer.button(3, 0, 1, wl_pointer::ButtonState::Pressed);
    touch.down(4, 0, &target, 0, 0.0, 0.0);
    f.pump();
    assert_eq!(f.serial(), Err(ClipboardError::PermissionDenied));
    f.enter(100, &target);
    assert_eq!(f.serial(), Ok(100));
    keyboard.modifiers(101, 0, 0, 0, 0);
    pointer.enter(102, &other, 0.0, 0.0);
    pointer.button(103, 0, 1, wl_pointer::ButtonState::Pressed);
    touch.down(104, 0, &other, 1, 0.0, 0.0);
    f.pump();
    assert_eq!(f.serial(), Ok(100));
    assert_eq!(f.client.authorizations, 1);
    keyboard.key(105, 0, 30, wl_keyboard::KeyState::Pressed);
    f.pump();
    assert_eq!(f.serial(), Ok(105));
    pointer.enter(106, &target, 0.0, 0.0);
    pointer.button(107, 0, 1, wl_pointer::ButtonState::Pressed);
    f.pump();
    assert_eq!(f.serial(), Ok(107));
    touch.down(0, 0, &target, 2, 0.0, 0.0);
    f.pump();
    assert_eq!(f.serial(), Ok(0));
    pointer.leave(108, &target);
    pointer.button(109, 0, 1, wl_pointer::ButtonState::Pressed);
    f.pump();
    assert_eq!(f.serial(), Ok(0));
    f.enter(110, &other);
    assert_eq!(f.serial(), Err(ClipboardError::PermissionDenied));
}

#[test]
fn returning_within_the_client_preserves_offer_but_changes_read_generation() {
    let mut f = Fixture::new();
    f.enable_input();
    let target = f.server.surfaces[0].clone();
    let (offer, proxy) = f.offer(&["text/plain"], false);
    f.client
        .input
        .as_mut()
        .unwrap()
        .select(Some(Rc::clone(&offer)));
    assert!(matches!(
        f.client.input.as_ref().unwrap().read_offer(),
        Err(ClipboardError::PermissionDenied)
    ));
    f.enter(1, &target);
    let generation = f.client.input.as_ref().unwrap().focus_generation().unwrap();
    f.pump();
    let keyboard = f.server.keyboard.as_ref().unwrap();
    keyboard.leave(2, &target);
    keyboard.enter(3, &target, Vec::new());
    f.pump();
    f.pump();
    assert!(proxy.is_alive());
    assert_eq!(f.serial(), Ok(3));
    assert_ne!(
        f.client.input.as_ref().unwrap().focus_generation().unwrap(),
        generation
    );
    assert!(Rc::ptr_eq(
        &offer,
        &f.client.input.as_ref().unwrap().read_offer().unwrap()
    ));
    let (next, proxy) = f.offer(&["text/plain"], false);
    f.client
        .input
        .as_mut()
        .unwrap()
        .select(Some(Rc::clone(&next)));
    f.client
        .input
        .as_mut()
        .unwrap()
        .select(Some(Rc::clone(&next)));
    f.pump();
    assert!(proxy.is_alive());
    assert!(Read::begin(next, Instant::now()).is_ok());
}

#[test]
fn capability_removal_retires_subscriptions_and_queued_input_cannot_reauthorize() {
    let mut f = Fixture::new();
    let seat = f.enable_input();
    let target = f.server.surfaces[0].clone();
    f.enter(10, &target);
    let keyboard = f.server.keyboard.clone().unwrap();
    let pointer = f.server.pointer.clone().unwrap();
    let touch = f.server.touch.clone().unwrap();
    seat.capabilities(wl_seat::Capability::empty());
    keyboard.key(11, 0, 30, wl_keyboard::KeyState::Pressed);
    f.pump();
    f.pump();
    assert_eq!(f.serial(), Err(ClipboardError::PermissionDenied));
    assert_eq!(f.client.authorizations, 1);
    assert!(!keyboard.is_alive());
    assert!(!pointer.is_alive());
    assert!(!touch.is_alive());
    seat.capabilities(wl_seat::Capability::Keyboard);
    f.pump();
    f.pump();
    assert_ne!(f.server.keyboard.as_ref().unwrap().id(), keyboard.id());
    f.enter(12, &target);
    assert_eq!(f.serial(), Ok(12));
}

#[test]
fn seat_drop_retires_owned_objects_without_destroying_host_surfaces() {
    let mut f = Fixture::new();
    let seat = f.enable_input();
    let target = f.server.surfaces[0].clone();
    f.enter(1, &target);
    let (offer, proxy) = f.offer(&["text/plain"], false);
    f.client
        .input
        .as_mut()
        .unwrap()
        .select(Some(Rc::clone(&offer)));
    drop(f.client.input.take());
    f.pump();
    assert!(!seat.is_alive());
    assert!(!f.server.keyboard.as_ref().unwrap().is_alive());
    assert!(!f.server.pointer.as_ref().unwrap().is_alive());
    assert!(!f.server.touch.as_ref().unwrap().is_alive());
    assert!(!proxy.is_alive());
    assert!(f.server.surfaces.iter().all(Resource::is_alive));
    assert!(matches!(
        Read::begin(offer, Instant::now()),
        Err(ClipboardError::Unavailable)
    ));
}
