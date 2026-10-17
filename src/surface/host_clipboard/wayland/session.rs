use std::{rc::Rc, sync::Arc, time::Instant};

use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
    backend::ObjectId,
    protocol::{
        wl_callback, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
        wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_touch,
    },
};

use super::{
    read::{Offer, Read},
    seat::{Seat, retire},
    source::Source,
    transfer::{Budget, TIMEOUT},
};
use crate::ui::clipboard::{ClipboardData, ClipboardError};

struct Input {
    name: u32,
    seat: Seat,
    device: Option<wl_data_device::WlDataDevice>,
    source: Source,
}

impl Drop for Input {
    fn drop(&mut self) {
        if let Some(device) = self.device.take() {
            retire(device, 2, wl_data_device::WlDataDevice::release);
        }
    }
}

struct PendingOffer {
    device: ObjectId,
    proxy: wl_data_offer::WlDataOffer,
    offer: Offer,
}

enum Operation {
    Read {
        read: Read,
        seat: ObjectId,
        generation: u64,
    },
    Write {
        source: wl_data_source::WlDataSource,
        sync: wl_callback::WlCallback,
        result: Option<Result<(), ClipboardError>>,
        deadline: Instant,
    },
}

impl Drop for Operation {
    fn drop(&mut self) {
        if let Self::Write { sync, .. } = self
            && sync.is_alive()
            && let Some(backend) = sync.backend().upgrade()
        {
            let _ = backend.destroy_object(&sync.id());
        }
    }
}

pub(super) enum Completion {
    Written,
    Read(ClipboardData),
}

struct State {
    target: ObjectId,
    manager: Option<(u32, wl_data_device_manager::WlDataDeviceManager)>,
    inputs: Vec<Input>,
    offers: Vec<PendingOffer>,
    active: Option<ObjectId>,
    pending: Option<Operation>,
    failure: Option<ClipboardError>,
    next: usize,
}

pub(super) struct Session {
    state: State,
    registry: wl_registry::WlRegistry,
    queue: EventQueue<State>,
    connection: Connection,
}

impl Session {
    pub(super) fn new(connection: Connection, target: ObjectId) -> Self {
        let queue = connection.new_event_queue();
        let registry = connection.display().get_registry(&queue.handle(), ());
        Self {
            state: State {
                target,
                manager: None,
                inputs: Vec::new(),
                offers: Vec::new(),
                active: None,
                pending: None,
                failure: None,
                next: 0,
            },
            registry,
            queue,
            connection,
        }
    }

    pub(super) fn write(
        &mut self,
        data: Arc<ClipboardData>,
        now: Instant,
    ) -> Result<(), ClipboardError> {
        self.state.available()?;
        let manager = &self
            .state
            .manager
            .as_ref()
            .ok_or(ClipboardError::Unavailable)?
            .1;
        let active = self
            .state
            .active
            .as_ref()
            .ok_or(ClipboardError::PermissionDenied)?;
        let input = self
            .state
            .inputs
            .iter_mut()
            .find(|input| input.seat.id() == *active)
            .ok_or(ClipboardError::Unavailable)?;
        let serial = input.seat.write_serial()?;
        let device = input.device.as_ref().ok_or(ClipboardError::Unavailable)?;
        let source = input
            .source
            .claim(manager, device, serial, data, &self.queue.handle())?;
        let sync = self.connection.display().sync(&self.queue.handle(), ());
        self.state.pending = Some(Operation::Write {
            source,
            sync,
            result: None,
            deadline: now + TIMEOUT,
        });
        Ok(())
    }

    pub(super) fn read(&mut self, now: Instant) -> Result<(), ClipboardError> {
        self.state.available()?;
        let active = self
            .state
            .active
            .as_ref()
            .ok_or(ClipboardError::PermissionDenied)?;
        let input = self
            .state
            .inputs
            .iter()
            .find(|input| input.seat.id() == *active)
            .ok_or(ClipboardError::Unavailable)?;
        self.state.pending = Some(Operation::Read {
            generation: input.seat.focus_generation()?,
            seat: input.seat.id(),
            read: Read::begin(input.seat.read_offer()?, now)?,
        });
        Ok(())
    }

    /// Dispatches only events already read by the host's native event loop.
    pub(super) fn poll(
        &mut self,
        now: Instant,
    ) -> Result<Option<Result<Completion, ClipboardError>>, ClipboardError> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|_| ClipboardError::Unavailable)?;
        if let Some(error) = self.state.failure {
            return Err(error);
        }
        let mut budget = Budget::new(65536, 32);
        let count = self.state.inputs.len() + 1;
        let mut completion = None;
        for step in 0..count {
            let index = (self.state.next + step) % count;
            if index < self.state.inputs.len() {
                let _ = self.state.inputs[index].source.poll(now, &mut budget);
            } else if let Some(Operation::Read {
                read,
                seat,
                generation,
            }) = &mut self.state.pending
            {
                let valid = self.state.inputs.iter().any(|input| {
                    input.seat.id() == *seat && input.seat.focus_generation() == Ok(*generation)
                });
                completion = match if valid {
                    read.poll(now, &mut budget)
                } else {
                    Err(ClipboardError::Unavailable)
                } {
                    Ok(Some(data)) => Some(Ok(Completion::Read(data))),
                    Ok(None) => None,
                    Err(error) => Some(Err(error)),
                };
            }
        }
        self.state.next = (self.state.next + 1) % count;
        if let Some(Operation::Write {
            result, deadline, ..
        }) = &mut self.state.pending
        {
            completion = if now >= *deadline {
                Some(Err(ClipboardError::Unavailable))
            } else {
                result
                    .take()
                    .map(|result| result.map(|()| Completion::Written))
            };
        }
        match self.connection.flush() {
            Ok(()) => (),
            Err(wayland_client::backend::WaylandError::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Err(ClipboardError::Unavailable),
        }
        if completion.is_some() {
            self.state.pending = None;
        }
        Ok(completion)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.state.pending = None;
        self.state.offers.clear();
        self.state.inputs.clear();
        if let Some((_, manager)) = self.state.manager.take() {
            let _ = self.connection.backend().destroy_object(&manager.id());
        }
        let _ = self
            .connection
            .backend()
            .destroy_object(&self.registry.id());
        let _ = self.connection.flush();
    }
}

impl State {
    fn available(&self) -> Result<(), ClipboardError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.pending.is_some() {
            return Err(ClipboardError::Capacity);
        }
        Ok(())
    }

    fn devices(&mut self, queue: &QueueHandle<Self>) {
        if let Some((_, manager)) = &self.manager {
            for input in &mut self.inputs {
                if input.device.is_none() {
                    input.device = Some(manager.get_data_device(input.seat.proxy(), queue, ()));
                }
            }
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                match interface.as_str() {
                    "wl_data_device_manager" if state.manager.is_none() => {
                        state.manager =
                            Some((name, registry.bind(name, version.min(3), queue, ())));
                    }
                    "wl_seat" => {
                        if state.inputs.try_reserve(1).is_err() {
                            state.failure = Some(ClipboardError::Capacity);
                            return;
                        }
                        let proxy = registry.bind(name, version.min(7), queue, ());
                        state.inputs.push(Input {
                            name,
                            seat: Seat::new(proxy, state.target.clone()),
                            device: None,
                            source: Source::default(),
                        });
                    }
                    _ => (),
                }
                state.devices(queue);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if state.manager.as_ref().is_some_and(|(id, _)| *id == name) {
                    state.failure = Some(ClipboardError::Unavailable);
                }
                if let Some(index) = state.inputs.iter().position(|input| input.name == name) {
                    let input = state.inputs.remove(index);
                    state.offers.retain(|offer| {
                        input
                            .device
                            .as_ref()
                            .is_none_or(|device| offer.device != device.id())
                    });
                    if let Some(Operation::Write { source, result, .. }) = &mut state.pending
                        && input.source.current() == Some(source)
                    {
                        *result = Some(Err(ClipboardError::Unavailable));
                    }
                    if state.active.as_ref() == Some(&input.seat.id()) {
                        state.active = None;
                    }
                }
            }
            _ => (),
        }
    }
}

wayland_client::delegate_noop!(State: ignore wl_data_device_manager::WlDataDeviceManager);

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
            && let Some(input) = state
                .inputs
                .iter_mut()
                .find(|input| input.seat.id() == proxy.id())
        {
            input.seat.capabilities(capabilities, queue);
        }
    }
}

macro_rules! input_dispatch {
    ($module:ident, $proxy:ident, $method:ident) => {
        impl Dispatch<$module::$proxy, ()> for State {
            fn event(
                state: &mut Self,
                proxy: &$module::$proxy,
                event: $module::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                if let Some(input) = state
                    .inputs
                    .iter_mut()
                    .find(|input| input.seat.owns_input(&proxy.id()))
                    && input.seat.$method(proxy, event)
                {
                    state.active = Some(input.seat.id());
                }
            }
        }
    };
}
input_dispatch!(wl_keyboard, WlKeyboard, keyboard);
input_dispatch!(wl_pointer, WlPointer, pointer);
input_dispatch!(wl_touch, WlTouch, touch);

impl Dispatch<wl_data_source::WlDataSource, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_data_source::Event::Cancelled)
            && let Some(Operation::Write { source, result, .. }) = &mut state.pending
            && source == proxy
        {
            *result = Some(Err(ClipboardError::Unavailable));
        }
        if let Some(input) = state
            .inputs
            .iter_mut()
            .find(|input| input.source.current() == Some(proxy))
        {
            let _ = input.source.event(proxy, event, Instant::now());
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(Operation::Write { sync, result, .. }) = &mut state.pending
            && sync == proxy
            && result.is_none()
        {
            *result = Some(Ok(()));
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                if state.offers.try_reserve(1).is_err() {
                    id.destroy();
                    state.failure = Some(ClipboardError::Capacity);
                    return;
                }
                state.offers.push(PendingOffer {
                    device: proxy.id(),
                    offer: Offer::new(id.clone()),
                    proxy: id,
                });
            }
            wl_data_device::Event::Selection { id } => {
                let offer = id.and_then(|id| {
                    state
                        .offers
                        .iter()
                        .position(|offer| offer.proxy == id && offer.device == proxy.id())
                        .map(|index| Rc::new(state.offers.remove(index).offer))
                });
                if let Some(input) = state
                    .inputs
                    .iter_mut()
                    .find(|input| input.device.as_ref() == Some(proxy))
                {
                    input.seat.select(offer);
                }
            }
            wl_data_device::Event::Enter { id: Some(id), .. } => {
                state.offers.retain(|offer| offer.proxy != id);
            }
            _ => (),
        }
    }
    wayland_client::event_created_child!(State, wl_data_device::WlDataDevice, [0 => (wl_data_offer::WlDataOffer, ())]);
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event
            && let Some(offer) = state.offers.iter_mut().find(|offer| offer.proxy == *proxy)
            && let Err(error) = offer.offer.advertise(mime_type)
        {
            state.failure = Some(error);
        }
    }
}
