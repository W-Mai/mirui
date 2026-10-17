use alloc::{string::String, vec::Vec};
use std::time::Instant;

use x11rb::{
    connection::Connection,
    protocol::{Event, xproto::*},
};

use crate::ui::clipboard::{
    ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem, validate_mime,
};

use super::{
    protocol::{Atoms, TIMEOUT, platform, read_property},
    transfer::{Incoming, Property as Value},
};

enum Phase {
    Timestamp,
    Targets,
    Data(usize),
    Verify,
    Finished,
}

struct Format {
    atom: Atom,
    mime: String,
    priority: u8,
}

pub(super) struct Read {
    window: Window,
    property: Atom,
    atoms: Atoms,
    owner: Window,
    request_time: Timestamp,
    timestamp: Timestamp,
    phase: Phase,
    formats: Vec<Format>,
    item: Option<ClipboardItem>,
    bytes: usize,
    incoming: Option<Incoming>,
    deadline: Instant,
}

impl Read {
    pub(super) fn begin(
        connection: &impl Connection,
        atoms: Atoms,
        window: Window,
        property: Atom,
        time: Timestamp,
        now: Instant,
    ) -> Result<Self, ClipboardError> {
        if time == x11rb::CURRENT_TIME || property == x11rb::NONE {
            return Err(ClipboardError::InvalidData);
        }
        let owner =
            platform(platform(connection.get_selection_owner(atoms.CLIPBOARD))?.reply())?.owner;
        if owner == x11rb::NONE {
            return Err(ClipboardError::Unavailable);
        }
        let read = Self {
            window,
            property,
            atoms,
            owner,
            request_time: time,
            timestamp: 0,
            phase: Phase::Timestamp,
            formats: Vec::new(),
            item: None,
            bytes: 0,
            incoming: None,
            deadline: now + TIMEOUT,
        };
        read.request(connection)?;
        Ok(read)
    }

    pub(super) fn event(
        &mut self,
        connection: &impl Connection,
        event: &Event,
        now: Instant,
    ) -> Result<Option<ClipboardData>, ClipboardError> {
        let result = self.receive(connection, event, now);
        if result.is_err() {
            self.phase = Phase::Finished;
            self.incoming = None;
            self.item = None;
        }
        result
    }

    pub(super) fn expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }

    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }

    fn target(&self) -> Result<Atom, ClipboardError> {
        match self.phase {
            Phase::Timestamp | Phase::Verify => Ok(self.atoms.TIMESTAMP),
            Phase::Targets => Ok(self.atoms.TARGETS),
            Phase::Data(index) => Ok(self.formats[index].atom),
            Phase::Finished => Err(ClipboardError::InvalidData),
        }
    }

    fn verify_owner(&self, connection: &impl Connection) -> Result<(), ClipboardError> {
        let owner =
            platform(platform(connection.get_selection_owner(self.atoms.CLIPBOARD))?.reply())?
                .owner;
        if owner != self.owner {
            return Err(ClipboardError::Unavailable);
        }
        Ok(())
    }

    fn request(&self, connection: &impl Connection) -> Result<(), ClipboardError> {
        self.verify_owner(connection)?;
        platform(platform(connection.delete_property(self.window, self.property))?.check())?;
        platform(
            platform(connection.convert_selection(
                self.window,
                self.atoms.CLIPBOARD,
                self.target()?,
                self.property,
                self.request_time,
            ))?
            .check(),
        )?;
        platform(connection.flush())
    }

    fn receive(
        &mut self,
        connection: &impl Connection,
        event: &Event,
        now: Instant,
    ) -> Result<Option<ClipboardData>, ClipboardError> {
        if self.expired(now) {
            return Err(ClipboardError::Unavailable);
        }
        match event {
            Event::SelectionNotify(notify)
                if notify.requestor == self.window
                    && notify.selection == self.atoms.CLIPBOARD
                    && notify.target == self.target()?
                    && notify.time == self.request_time
                    && self.incoming.is_none() =>
            {
                self.verify_owner(connection)?;
                if notify.property == x11rb::NONE {
                    return Err(ClipboardError::Unsupported);
                }
                if notify.property != self.property {
                    return Ok(None);
                }
                let value = read_property(connection, self.window, self.property)?;
                if value.kind == self.atoms.INCR {
                    if value.format != 32 || value.bytes.len() != 4 {
                        return Err(ClipboardError::InvalidData);
                    }
                    self.incoming = Some(Incoming::new(u32::from_ne_bytes(
                        value.bytes[..].try_into().unwrap(),
                    )));
                    platform(
                        platform(connection.delete_property(self.window, self.property))?.check(),
                    )?;
                    platform(connection.flush())?;
                    Ok(None)
                } else {
                    self.accept(connection, value)
                }
            }
            Event::PropertyNotify(property)
                if property.window == self.window
                    && property.atom == self.property
                    && property.state == Property::NEW_VALUE
                    && self.incoming.is_some() =>
            {
                self.verify_owner(connection)?;
                let value = read_property(connection, self.window, self.property)?;
                let complete = self.incoming.as_mut().unwrap().append(value)?;
                platform(
                    platform(connection.delete_property(self.window, self.property))?.check(),
                )?;
                platform(connection.flush())?;
                if complete {
                    let value = self.incoming.take().unwrap().finish()?;
                    self.accept(connection, value)
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn accept(
        &mut self,
        connection: &impl Connection,
        value: Value,
    ) -> Result<Option<ClipboardData>, ClipboardError> {
        self.verify_owner(connection)?;
        match self.phase {
            Phase::Timestamp | Phase::Verify => {
                if value.kind != u32::from(AtomEnum::INTEGER)
                    || value.format != 32
                    || value.bytes.len() != 4
                {
                    return Err(ClipboardError::InvalidData);
                }
                let timestamp = u32::from_ne_bytes(value.bytes[..].try_into().unwrap());
                if timestamp == x11rb::CURRENT_TIME {
                    return Err(ClipboardError::InvalidData);
                }
                if matches!(self.phase, Phase::Verify) {
                    if timestamp != self.timestamp {
                        return Err(ClipboardError::Unavailable);
                    }
                    platform(
                        platform(connection.delete_property(self.window, self.property))?.check(),
                    )?;
                    let mut data = ClipboardData::with_capacity(1, self.bytes)?;
                    data.push(self.item.take().ok_or(ClipboardError::InvalidData)?)?;
                    self.phase = Phase::Finished;
                    return Ok(Some(data));
                }
                self.timestamp = timestamp;
                self.phase = Phase::Targets;
            }
            Phase::Targets => {
                if value.kind != u32::from(AtomEnum::ATOM)
                    || value.format != 32
                    || !value.bytes.len().is_multiple_of(4)
                {
                    return Err(ClipboardError::InvalidData);
                }
                for bytes in value.bytes.chunks_exact(4) {
                    let atom = u32::from_ne_bytes(bytes.try_into().unwrap());
                    self.add_format(connection, atom)?;
                }
                if self.formats.is_empty() {
                    return Err(ClipboardError::Unsupported);
                }
                self.item = Some(ClipboardItem::with_capacity(self.formats.len())?);
                self.phase = Phase::Data(0);
            }
            Phase::Data(index) => {
                let format = &self.formats[index];
                if value.format != 8 || value.kind != format.atom {
                    return Err(ClipboardError::InvalidData);
                }
                let bytes = if format.atom == u32::from(AtomEnum::STRING) {
                    let mut bytes = Vec::new();
                    let length = value.bytes.iter().try_fold(0usize, |length, byte| {
                        length
                            .checked_add(if *byte < 128 { 1 } else { 2 })
                            .ok_or(ClipboardError::Capacity)
                    })?;
                    bytes
                        .try_reserve_exact(length)
                        .map_err(|_| ClipboardError::Capacity)?;
                    for byte in value.bytes {
                        let mut buffer = [0; 2];
                        bytes.extend_from_slice(
                            char::from(byte).encode_utf8(&mut buffer).as_bytes(),
                        );
                    }
                    bytes
                } else {
                    value.bytes
                };
                if format.mime == "text/plain" {
                    core::str::from_utf8(&bytes).map_err(|_| ClipboardError::InvalidData)?;
                }
                self.bytes = self
                    .bytes
                    .checked_add(bytes.len())
                    .ok_or(ClipboardError::Capacity)?;
                let mime = core::mem::take(&mut self.formats[index].mime);
                self.item
                    .as_mut()
                    .unwrap()
                    .push(ClipboardFormat::new(mime, bytes))?;
                self.phase = if index + 1 < self.formats.len() {
                    Phase::Data(index + 1)
                } else {
                    Phase::Verify
                };
            }
            Phase::Finished => return Err(ClipboardError::InvalidData),
        }
        self.request(connection)?;
        Ok(None)
    }

    fn add_format(
        &mut self,
        connection: &impl Connection,
        atom: Atom,
    ) -> Result<(), ClipboardError> {
        if [
            self.atoms.TARGETS,
            self.atoms.TIMESTAMP,
            self.atoms.MULTIPLE,
            self.atoms.ATOM_PAIR,
            self.atoms.INCR,
            self.atoms.TEXT,
        ]
        .contains(&atom)
        {
            return Ok(());
        }
        let (mime, priority) = if atom == self.atoms.UTF8_STRING {
            (String::from("text/plain"), 2)
        } else if atom == u32::from(AtomEnum::STRING) {
            (String::from("text/plain"), 3)
        } else {
            if atom == x11rb::NONE {
                return Err(ClipboardError::InvalidData);
            }
            let name = platform(platform(connection.get_atom_name(atom))?.reply())?.name;
            let Ok(name) = String::from_utf8(name) else {
                return Ok(());
            };
            if name.eq_ignore_ascii_case("text/plain;charset=utf-8") {
                (String::from("text/plain"), 1)
            } else if validate_mime(&name).is_ok() {
                (name.to_ascii_lowercase(), 0)
            } else {
                return Ok(());
            }
        };
        if let Some(existing) = self.formats.iter_mut().find(|format| format.mime == mime) {
            if existing.priority > priority {
                *existing = Format {
                    atom,
                    mime,
                    priority,
                };
            }
        } else {
            self.formats
                .try_reserve(1)
                .map_err(|_| ClipboardError::Capacity)?;
            self.formats.push(Format {
                atom,
                mime,
                priority,
            });
        }
        Ok(())
    }
}
