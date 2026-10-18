use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};
use x11rb::protocol::Event;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::{PollMode, Stream};
use x11rb::utils::RawFdContainer;
use x11rb::x11_utils::{Serialize, TryParse};

#[derive(Clone)]
pub struct Wire(Arc<Mutex<State>>);

#[derive(Default)]
pub struct State {
    handshake: bool,
    input: Vec<u8>,
    output: VecDeque<u8>,
    sequence: u16,
    pub atoms: HashMap<Vec<u8>, u32>,
    pub properties: HashMap<(u32, u32), (u32, u8, Vec<u8>)>,
    pub notifications: Vec<[u8; 32]>,
    pub masks: HashMap<u32, u32>,
    pub owner: u32,
    pub conversions: Vec<(u32, u32, u32, u32, u32)>,
    pub events: VecDeque<Event>,
    pub windows: Vec<u32>,
    pub destroyed: Vec<u32>,
    pub socket_events: bool,
    pub time: Option<u32>,
    pub xfixes: bool,
    pub xfixes_failure: Option<u8>,
    pub selection_masks: HashMap<u32, u32>,
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

impl State {
    fn emit(&mut self, event: Event) {
        if self.socket_events {
            let mut bytes = [0; 32];
            let mut copy = |event: &[u8]| bytes[..event.len()].copy_from_slice(event);
            match event {
                Event::PropertyNotify(event) => copy(&event.serialize()),
                Event::SelectionNotify(event) => copy(&event.serialize()),
                Event::SelectionRequest(event) => copy(&event.serialize()),
                Event::DestroyNotify(event) => copy(&event.serialize()),
                Event::XfixesSelectionNotify(event) => copy(&event.serialize()),
                _ => panic!("unsupported fixture event"),
            }
            bytes[2..4].copy_from_slice(&self.sequence.to_ne_bytes());
            self.output.extend(bytes);
        } else {
            self.events.push_back(event);
        }
    }
    fn property_event(&mut self, window: u32, atom: u32, state: Property) {
        if self
            .masks
            .get(&window)
            .is_some_and(|mask| mask & u32::from(EventMask::PROPERTY_CHANGE) != 0)
        {
            self.emit(Event::PropertyNotify(PropertyNotifyEvent {
                response_type: PROPERTY_NOTIFY_EVENT,
                sequence: self.sequence,
                window,
                atom,
                time: self.time.unwrap_or(2000),
                state,
            }));
        }
    }
    fn reply(&mut self, bytes: impl AsRef<[u8]>) {
        let bytes = bytes.as_ref();
        self.output.extend(bytes);
        self.output.extend(core::iter::repeat_n(
            0,
            (32 + u32_at(bytes, 4) as usize * 4).saturating_sub(bytes.len()),
        ));
    }
    fn process(&mut self, request: &[u8]) {
        self.sequence = self.sequence.wrapping_add(1);
        let sequence = self.sequence;
        if request[0] == 200 && self.xfixes_failure == Some(request[1]) {
            let mut error = [0; 32];
            error[1] = 10;
            error[2..4].copy_from_slice(&sequence.to_ne_bytes());
            error[8..10].copy_from_slice(&u16::from(request[1]).to_ne_bytes());
            error[10] = 200;
            self.output.extend(error);
            return;
        }
        match request[0] {
            CREATE_WINDOW_REQUEST => {
                let window = u32_at(request, 4);
                assert_eq!(u32_at(request, 8), 1);
                assert_eq!(request[1], 0);
                assert_eq!(u16::from_ne_bytes(request[22..24].try_into().unwrap()), 2);
                assert_eq!(u32_at(request, 28), u32::from(CW::EVENT_MASK));
                assert!(!self.windows.contains(&window));
                self.windows.push(window);
                self.masks.insert(window, u32_at(request, 32));
            }
            DESTROY_WINDOW_REQUEST => {
                let window = u32_at(request, 4);
                self.windows.retain(|id| *id != window);
                self.destroyed.push(window);
                self.masks.remove(&window);
                self.selection_masks.remove(&window);
                self.properties.retain(|(id, _), _| *id != window);
                self.emit(Event::DestroyNotify(DestroyNotifyEvent {
                    response_type: DESTROY_NOTIFY_EVENT,
                    sequence,
                    event: window,
                    window,
                }));
            }
            INTERN_ATOM_REQUEST => {
                let length = u16::from_ne_bytes(request[4..6].try_into().unwrap()) as usize;
                let next = 1000 + self.atoms.len() as u32;
                let atom = *self
                    .atoms
                    .entry(request[8..8 + length].to_vec())
                    .or_insert(next);
                self.reply(
                    InternAtomReply {
                        sequence,
                        atom,
                        ..Default::default()
                    }
                    .serialize(),
                );
            }
            QUERY_EXTENSION_REQUEST => {
                let length = u16::from_ne_bytes(request[4..6].try_into().unwrap()) as usize;
                let present = self.xfixes && &request[8..8 + length] == b"XFIXES";
                self.reply(
                    QueryExtensionReply {
                        sequence,
                        present,
                        major_opcode: if present { 200 } else { 0 },
                        first_event: if present { 90 } else { 0 },
                        ..Default::default()
                    }
                    .serialize(),
                );
            }
            200 => match request[1] {
                0 => self.reply(
                    x11rb::protocol::xfixes::QueryVersionReply {
                        sequence,
                        major_version: 1,
                        minor_version: 0,
                        ..Default::default()
                    }
                    .serialize(),
                ),
                2 => {
                    self.selection_masks
                        .insert(u32_at(request, 4), u32_at(request, 12));
                }
                opcode => panic!("unexpected XFixes opcode {opcode}"),
            },
            GET_ATOM_NAME_REQUEST => {
                let id = u32_at(request, 4);
                let name = self
                    .atoms
                    .iter()
                    .find_map(|(name, atom)| (*atom == id).then_some(name.clone()))
                    .unwrap();
                self.reply(
                    GetAtomNameReply {
                        sequence,
                        length: name.len().div_ceil(4) as u32,
                        name,
                    }
                    .serialize(),
                );
            }
            DELETE_PROPERTY_REQUEST => {
                let window = u32_at(request, 4);
                let atom = u32_at(request, 8);
                if self.properties.remove(&(window, atom)).is_some() {
                    self.property_event(window, atom, Property::DELETE);
                }
            }
            CONVERT_SELECTION_REQUEST => {
                self.conversions.push((
                    u32_at(request, 4),
                    u32_at(request, 8),
                    u32_at(request, 12),
                    u32_at(request, 16),
                    u32_at(request, 20),
                ));
                if self.socket_events {
                    self.emit(Event::SelectionRequest(SelectionRequestEvent {
                        response_type: SELECTION_REQUEST_EVENT,
                        sequence,
                        owner: self.owner,
                        requestor: u32_at(request, 4),
                        selection: u32_at(request, 8),
                        target: u32_at(request, 12),
                        property: u32_at(request, 16),
                        time: u32_at(request, 20),
                    }));
                }
            }
            GET_INPUT_FOCUS_REQUEST => self.reply(
                GetInputFocusReply {
                    sequence,
                    ..Default::default()
                }
                .serialize(),
            ),
            SET_SELECTION_OWNER_REQUEST => self.owner = u32_at(request, 4),
            GET_SELECTION_OWNER_REQUEST => self.reply(
                GetSelectionOwnerReply {
                    sequence,
                    owner: self.owner,
                    ..Default::default()
                }
                .serialize(),
            ),
            CHANGE_WINDOW_ATTRIBUTES_REQUEST => {
                self.masks.insert(u32_at(request, 4), u32_at(request, 12));
            }
            GET_WINDOW_ATTRIBUTES_REQUEST => {
                self.reply(
                    GetWindowAttributesReply {
                        sequence,
                        length: 3,
                        your_event_mask: (*self.masks.get(&u32_at(request, 4)).unwrap_or(&0))
                            .into(),
                        ..Default::default()
                    }
                    .serialize(),
                );
            }
            CHANGE_PROPERTY_REQUEST => {
                assert_eq!(request[1], u8::from(PropMode::REPLACE));
                let format = request[16];
                let size = u32_at(request, 20) as usize * usize::from(format / 8);
                self.properties.insert(
                    (u32_at(request, 4), u32_at(request, 8)),
                    (u32_at(request, 12), format, request[24..24 + size].to_vec()),
                );
                self.property_event(u32_at(request, 4), u32_at(request, 8), Property::NEW_VALUE);
            }
            GET_PROPERTY_REQUEST => {
                let value = self
                    .properties
                    .get(&(u32_at(request, 4), u32_at(request, 8)))
                    .unwrap();
                let offset = u32_at(request, 16) as usize * 4;
                let end = value.2.len().min(offset + u32_at(request, 20) as usize * 4);
                let bytes = value.2[offset..end].to_vec();
                self.reply(
                    GetPropertyReply {
                        sequence,
                        format: value.1,
                        type_: value.0,
                        length: bytes.len().div_ceil(4) as u32,
                        bytes_after: (value.2.len() - end) as u32,
                        value_len: bytes.len() as u32 / u32::from(value.1 / 8),
                        value: bytes,
                    }
                    .serialize(),
                );
            }
            SEND_EVENT_REQUEST => {
                self.notifications.push(request[12..44].try_into().unwrap());
                self.emit(Event::SelectionNotify(
                    SelectionNotifyEvent::try_parse(&request[12..44]).unwrap().0,
                ));
            }
            opcode => panic!("unexpected request {opcode}"),
        }
    }
}

impl Stream for Wire {
    fn poll(&self, mode: PollMode) -> io::Result<()> {
        assert!(mode.writable() || !self.0.lock().unwrap().output.is_empty());
        Ok(())
    }
    fn read(&self, buf: &mut [u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        assert!(fds.is_empty());
        let mut state = self.0.lock().unwrap();
        if state.output.is_empty() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let length = buf.len().min(state.output.len());
        for byte in &mut buf[..length] {
            *byte = state.output.pop_front().unwrap();
        }
        Ok(length)
    }
    fn write(&self, bytes: &[u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        assert!(fds.is_empty());
        let mut state = self.0.lock().unwrap();
        state.input.extend_from_slice(bytes);
        if !state.handshake {
            if state.input.len() < 12 {
                return Ok(bytes.len());
            }
            assert_eq!(&state.input[6..10], &[0, 0, 0, 0]);
            state.input.drain(..12);
            state.handshake = true;
            let mut setup = Setup {
                status: 1,
                protocol_major_version: 11,
                resource_id_base: 0x10000,
                resource_id_mask: 0xffff,
                maximum_request_length: u16::MAX,
                roots: vec![Screen {
                    root: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }
            .serialize();
            let length = ((setup.len() - 8) / 4) as u16;
            setup[6..8].copy_from_slice(&length.to_ne_bytes());
            state.output.extend(setup);
        }
        while state.input.len() >= 4 {
            let length = usize::from(u16::from_ne_bytes(state.input[2..4].try_into().unwrap())) * 4;
            assert!(length >= 4);
            if state.input.len() < length {
                break;
            }
            let request: Vec<_> = state.input.drain(..length).collect();
            state.process(&request);
        }
        Ok(bytes.len())
    }
}

pub fn transport() -> (Wire, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State::default()));
    (Wire(state.clone()), state)
}
