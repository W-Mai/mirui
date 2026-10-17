use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::{PollMode, RustConnection, Stream};
use x11rb::utils::RawFdContainer;
use x11rb::x11_utils::Serialize;

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
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

impl State {
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
        match request[0] {
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
            QUERY_EXTENSION_REQUEST => self.reply(
                QueryExtensionReply {
                    sequence,
                    present: false,
                    ..Default::default()
                }
                .serialize(),
            ),
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
            CHANGE_PROPERTY_REQUEST => {
                assert_eq!(request[1], u8::from(PropMode::REPLACE));
                let format = request[16];
                let size = u32_at(request, 20) as usize * usize::from(format / 8);
                self.properties.insert(
                    (u32_at(request, 4), u32_at(request, 8)),
                    (u32_at(request, 12), format, request[24..24 + size].to_vec()),
                );
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
            SEND_EVENT_REQUEST => self.notifications.push(request[12..44].try_into().unwrap()),
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

pub fn connection() -> (RustConnection<Wire>, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State::default()));
    let connection = RustConnection::connect_to_stream(Wire(state.clone()), 0).unwrap();
    (connection, state)
}
