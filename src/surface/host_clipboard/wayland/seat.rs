use std::rc::Rc;

use wayland_client::{
    Dispatch, Proxy, QueueHandle,
    backend::ObjectId,
    protocol::{wl_keyboard, wl_pointer, wl_seat, wl_touch},
};

use super::read::Offer;
use crate::ui::clipboard::ClipboardError;

pub(super) struct Seat {
    proxy: wl_seat::WlSeat,
    target: ObjectId,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    touch: Option<wl_touch::WlTouch>,
    keyboard_focus: bool,
    generation: u64,
    pointer_focus: bool,
    serial: Option<u32>,
    offer: Option<Rc<Offer>>,
}

impl Seat {
    pub(super) fn proxy(&self) -> &wl_seat::WlSeat {
        &self.proxy
    }

    pub(super) fn id(&self) -> ObjectId {
        self.proxy.id()
    }

    pub(super) fn owns_input(&self, id: &ObjectId) -> bool {
        self.keyboard.as_ref().is_some_and(|p| p.id() == *id)
            || self.pointer.as_ref().is_some_and(|p| p.id() == *id)
            || self.touch.as_ref().is_some_and(|p| p.id() == *id)
    }

    pub(super) fn new(proxy: wl_seat::WlSeat, target: ObjectId) -> Self {
        Self {
            proxy,
            target,
            keyboard: None,
            pointer: None,
            touch: None,
            keyboard_focus: false,
            generation: 0,
            pointer_focus: false,
            serial: None,
            offer: None,
        }
    }

    pub(super) fn capabilities<State>(
        &mut self,
        capabilities: wl_seat::Capability,
        queue: &QueueHandle<State>,
    ) where
        State: Dispatch<wl_keyboard::WlKeyboard, ()>
            + Dispatch<wl_pointer::WlPointer, ()>
            + Dispatch<wl_touch::WlTouch, ()>
            + 'static,
    {
        if capabilities.contains(wl_seat::Capability::Keyboard) {
            if self.keyboard.is_none() {
                self.keyboard = Some(self.proxy.get_keyboard(queue, ()));
            }
        } else {
            self.lose_focus();
            self.select(None);
            if let Some(keyboard) = self.keyboard.take() {
                retire(keyboard, 3, wl_keyboard::WlKeyboard::release);
            }
        }
        if capabilities.contains(wl_seat::Capability::Pointer) {
            if self.pointer.is_none() {
                self.pointer = Some(self.proxy.get_pointer(queue, ()));
            }
        } else {
            self.pointer_focus = false;
            if let Some(pointer) = self.pointer.take() {
                retire(pointer, 3, wl_pointer::WlPointer::release);
            }
        }
        if capabilities.contains(wl_seat::Capability::Touch) {
            if self.touch.is_none() {
                self.touch = Some(self.proxy.get_touch(queue, ()));
            }
        } else if let Some(touch) = self.touch.take() {
            retire(touch, 3, wl_touch::WlTouch::release);
        }
    }

    pub(super) fn keyboard(
        &mut self,
        proxy: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
    ) -> bool {
        if self.keyboard.as_ref() != Some(proxy) {
            return false;
        }
        match event {
            wl_keyboard::Event::Enter {
                serial, surface, ..
            } => {
                if surface.id() == self.target {
                    self.keyboard_focus = true;
                    self.record(serial)
                } else {
                    self.lose_focus();
                    false
                }
            }
            wl_keyboard::Event::Leave { .. } => {
                self.lose_focus();
                false
            }
            wl_keyboard::Event::Key { serial, .. } => self.record(serial),
            _ => false,
        }
    }

    pub(super) fn pointer(
        &mut self,
        proxy: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
    ) -> bool {
        if self.pointer.as_ref() != Some(proxy) {
            return false;
        }
        match event {
            wl_pointer::Event::Enter { surface, .. } => {
                self.pointer_focus = surface.id() == self.target;
                false
            }
            wl_pointer::Event::Leave { .. } => {
                self.pointer_focus = false;
                false
            }
            wl_pointer::Event::Button { serial, .. } if self.pointer_focus => self.record(serial),
            _ => false,
        }
    }

    pub(super) fn touch(&mut self, proxy: &wl_touch::WlTouch, event: wl_touch::Event) -> bool {
        if self.touch.as_ref() != Some(proxy) {
            return false;
        }
        match event {
            wl_touch::Event::Down {
                serial, surface, ..
            } if surface.id() == self.target => self.record(serial),
            _ => false,
        }
    }

    fn record(&mut self, serial: u32) -> bool {
        if !self.keyboard_focus {
            return false;
        }
        self.serial = Some(serial);
        true
    }

    fn lose_focus(&mut self) {
        if self.keyboard_focus {
            self.generation = self.generation.saturating_add(1);
        }
        self.keyboard_focus = false;
        self.serial = None;
    }

    pub(super) fn focus_generation(&self) -> Result<u64, ClipboardError> {
        if !self.keyboard_focus {
            return Err(ClipboardError::PermissionDenied);
        }
        if self.generation == u64::MAX {
            return Err(ClipboardError::Unavailable);
        }
        Ok(self.generation)
    }

    pub(super) fn select(&mut self, offer: Option<Rc<Offer>>) {
        if let (Some(old), Some(new)) = (&self.offer, &offer)
            && Rc::ptr_eq(old, new)
        {
            return;
        }
        if let Some(old) = self.offer.take() {
            old.invalidate();
        }
        self.offer = offer;
    }

    pub(super) fn write_serial(&self) -> Result<u32, ClipboardError> {
        if !self.keyboard_focus {
            return Err(ClipboardError::PermissionDenied);
        }
        self.serial.ok_or(ClipboardError::PermissionDenied)
    }

    pub(super) fn read_offer(&self) -> Result<Rc<Offer>, ClipboardError> {
        if !self.keyboard_focus {
            return Err(ClipboardError::PermissionDenied);
        }
        self.offer
            .as_ref()
            .cloned()
            .ok_or(ClipboardError::Unavailable)
    }
}

impl Drop for Seat {
    fn drop(&mut self) {
        self.lose_focus();
        self.select(None);
        if let Some(keyboard) = self.keyboard.take() {
            retire(keyboard, 3, wl_keyboard::WlKeyboard::release);
        }
        if let Some(pointer) = self.pointer.take() {
            retire(pointer, 3, wl_pointer::WlPointer::release);
        }
        if let Some(touch) = self.touch.take() {
            retire(touch, 3, wl_touch::WlTouch::release);
        }
        retire(self.proxy.clone(), 5, wl_seat::WlSeat::release);
    }
}

pub(super) fn retire<P: Proxy>(proxy: P, since: u32, release: impl FnOnce(&P)) {
    if proxy.version() >= since {
        release(&proxy);
    } else if let Some(backend) = proxy.backend().upgrade() {
        let _ = backend.destroy_object(&proxy.id());
    }
}
