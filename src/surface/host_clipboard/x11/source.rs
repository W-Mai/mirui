use alloc::{sync::Arc, vec::Vec};
use std::time::{Duration, Instant};

use x11rb::{
    connection::Connection,
    errors::ReplyError,
    protocol::{ErrorKind, Event, xproto::*},
    wrapper::ConnectionExt as _,
};

use crate::ui::clipboard::{ClipboardData, ClipboardError};

use super::transfer::Outgoing;

const TRANSFERS: usize = 16;
const MULTIPLE_PAIRS: u32 = 256;
const TIMEOUT: Duration = Duration::from_secs(10);

x11rb::atom_manager! {
    pub(super) Atoms: AtomsCookie {
        CLIPBOARD,
        TARGETS,
        TIMESTAMP,
        MULTIPLE,
        ATOM_PAIR,
        INCR,
        UTF8_STRING,
        TEXT,
    }
}

struct Offer {
    data: Arc<ClipboardData>,
    formats: Vec<(Atom, usize)>,
    targets: Vec<Atom>,
    time: Timestamp,
}

struct Transfer {
    window: Window,
    property: Atom,
    kind: Atom,
    bytes: Outgoing,
    deadline: Instant,
}

pub(super) struct Source {
    window: Window,
    atoms: Atoms,
    offer: Option<Offer>,
    transfers: Vec<Transfer>,
    chunk: usize,
}

fn platform<T, E>(result: Result<T, E>) -> Result<T, ClipboardError> {
    result.map_err(|_| ClipboardError::Platform)
}

fn forget_destroyed(result: Result<(), ReplyError>) -> Result<(), ClipboardError> {
    match result {
        Err(ReplyError::X11Error(error)) if error.error_kind == ErrorKind::Window => Ok(()),
        result => platform(result),
    }
}

fn precedes(time: Timestamp, reference: Timestamp) -> bool {
    time != x11rb::CURRENT_TIME && (time.wrapping_sub(reference) as i32) < 0
}

#[cfg(test)]
#[test]
fn timestamp_order_wraps_and_transfer_metadata_stays_bounded() {
    assert!(precedes(u32::MAX - 1, 2));
    assert!(!precedes(2, u32::MAX - 1));
    assert!(!precedes(x11rb::CURRENT_TIME, 2));
    assert!(!precedes(2, 2));
    let bytes = core::mem::size_of::<Transfer>() * TRANSFERS;
    assert!(bytes <= 2048);
    println!("X11 sender transfer metadata: {bytes} bytes");
}

impl Source {
    pub(super) fn new(
        connection: &impl Connection,
        window: Window,
    ) -> Result<Self, ClipboardError> {
        let atoms = platform(platform(Atoms::new(connection))?.reply())?;
        let chunk = connection
            .maximum_request_bytes()
            .checked_sub(32)
            .ok_or(ClipboardError::Capacity)?
            .min(64 * 1024);
        if chunk < 4 {
            return Err(ClipboardError::Capacity);
        }
        let mut transfers = Vec::new();
        transfers
            .try_reserve_exact(TRANSFERS)
            .map_err(|_| ClipboardError::Capacity)?;
        Ok(Self {
            window,
            atoms,
            offer: None,
            transfers,
            chunk,
        })
    }

    pub(super) fn claim(
        &mut self,
        connection: &impl Connection,
        data: Arc<ClipboardData>,
        time: Timestamp,
    ) -> Result<(), ClipboardError> {
        data.validate()?;
        let [item] = data.items() else {
            return Err(ClipboardError::Unsupported);
        };
        if time == x11rb::CURRENT_TIME
            || self
                .offer
                .as_ref()
                .is_some_and(|offer| time == offer.time || precedes(time, offer.time))
        {
            return Err(ClipboardError::InvalidData);
        }
        let count = item
            .formats()
            .len()
            .checked_add(5)
            .ok_or(ClipboardError::Capacity)?;
        if count.checked_mul(4).is_none_or(|bytes| bytes > self.chunk) {
            return Err(ClipboardError::Capacity);
        }
        let mut formats = Vec::new();
        let mut targets = Vec::new();
        formats
            .try_reserve_exact(count)
            .map_err(|_| ClipboardError::Capacity)?;
        targets
            .try_reserve_exact(count)
            .map_err(|_| ClipboardError::Capacity)?;
        targets.extend_from_slice(&[
            self.atoms.TARGETS,
            self.atoms.TIMESTAMP,
            self.atoms.MULTIPLE,
        ]);
        for (index, format) in item.formats().iter().enumerate() {
            u32::try_from(format.bytes().len()).map_err(|_| ClipboardError::Capacity)?;
            let name = format.mime().to_ascii_lowercase();
            let atom =
                platform(platform(connection.intern_atom(false, name.as_bytes()))?.reply())?.atom;
            formats.push((atom, index));
            targets.push(atom);
            if name == "text/plain" {
                core::str::from_utf8(format.bytes()).map_err(|_| ClipboardError::InvalidData)?;
                formats.extend_from_slice(&[
                    (self.atoms.UTF8_STRING, index),
                    (self.atoms.TEXT, index),
                ]);
                targets.extend_from_slice(&[self.atoms.UTF8_STRING, self.atoms.TEXT]);
            }
        }
        platform(
            platform(connection.set_selection_owner(self.window, self.atoms.CLIPBOARD, time))?
                .check(),
        )?;
        let owner =
            platform(platform(connection.get_selection_owner(self.atoms.CLIPBOARD))?.reply())?
                .owner;
        if owner != self.window {
            return Err(ClipboardError::Unavailable);
        }
        self.offer = Some(Offer {
            data,
            formats,
            targets,
            time,
        });
        Ok(())
    }

    pub(super) fn event(
        &mut self,
        connection: &impl Connection,
        event: &Event,
        now: Instant,
    ) -> Result<(), ClipboardError> {
        self.expire(connection, now)?;
        match event {
            Event::SelectionRequest(request) if request.owner == self.window => {
                let property = if request.property == x11rb::NONE {
                    request.target
                } else {
                    request.property
                };
                let valid = request.selection == self.atoms.CLIPBOARD
                    && request.requestor != self.window
                    && self
                        .offer
                        .as_ref()
                        .is_some_and(|offer| !precedes(request.time, offer.time));
                let result = if !valid {
                    Err(ClipboardError::Unavailable)
                } else if request.target == self.atoms.MULTIPLE {
                    self.multiple(connection, request, now)
                } else {
                    self.convert(connection, request.requestor, request.target, property, now)
                };
                let notify = SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: request.time,
                    requestor: request.requestor,
                    selection: request.selection,
                    target: request.target,
                    property: if result.is_ok() {
                        property
                    } else {
                        x11rb::NONE
                    },
                };
                forget_destroyed(
                    platform(connection.send_event(
                        false,
                        request.requestor,
                        EventMask::NO_EVENT,
                        notify,
                    ))?
                    .check(),
                )?;
            }
            Event::SelectionClear(clear)
                if clear.owner == self.window && clear.selection == self.atoms.CLIPBOARD =>
            {
                if self
                    .offer
                    .as_ref()
                    .is_some_and(|offer| !precedes(clear.time, offer.time))
                    && platform(
                        platform(connection.get_selection_owner(self.atoms.CLIPBOARD))?.reply(),
                    )?
                    .owner
                        != self.window
                {
                    self.offer = None;
                }
            }
            Event::PropertyNotify(property) if property.state == Property::DELETE => {
                if let Some(index) = self.transfers.iter().position(|transfer| {
                    transfer.window == property.window && transfer.property == property.atom
                }) {
                    let transfer = &mut self.transfers[index];
                    let bytes = transfer.bytes.next().ok_or(ClipboardError::InvalidData)?;
                    let finished = bytes.is_empty();
                    let result = platform(connection.change_property8(
                        PropMode::REPLACE,
                        transfer.window,
                        transfer.property,
                        transfer.kind,
                        bytes,
                    ))?
                    .check();
                    if finished || result.is_err() {
                        self.remove(connection, index)?;
                    }
                    forget_destroyed(result)?;
                }
            }
            Event::DestroyNotify(destroy) => {
                self.transfers
                    .retain(|transfer| transfer.window != destroy.window);
            }
            _ => {}
        }
        platform(connection.flush())
    }

    fn convert(
        &mut self,
        connection: &impl Connection,
        window: Window,
        target: Atom,
        property: Atom,
        now: Instant,
    ) -> Result<(), ClipboardError> {
        if window == self.window
            || property == x11rb::NONE
            || self
                .transfers
                .iter()
                .any(|transfer| transfer.window == window && transfer.property == property)
        {
            return Err(ClipboardError::Unavailable);
        }
        let offer = self.offer.as_ref().ok_or(ClipboardError::Unavailable)?;
        if target == self.atoms.TARGETS {
            return platform(
                platform(connection.change_property32(
                    PropMode::REPLACE,
                    window,
                    property,
                    AtomEnum::ATOM,
                    &offer.targets,
                ))?
                .check(),
            );
        }
        if target == self.atoms.TIMESTAMP {
            return platform(
                platform(connection.change_property32(
                    PropMode::REPLACE,
                    window,
                    property,
                    AtomEnum::INTEGER,
                    &[offer.time],
                ))?
                .check(),
            );
        }
        let index = offer
            .formats
            .iter()
            .find_map(|(atom, index)| (*atom == target).then_some(*index))
            .ok_or(ClipboardError::Unsupported)?;
        let kind = if target == self.atoms.TEXT {
            self.atoms.UTF8_STRING
        } else {
            target
        };
        let bytes = offer.data.items()[0].formats()[index].bytes();
        if bytes.len() <= self.chunk {
            return platform(
                platform(connection.change_property8(
                    PropMode::REPLACE,
                    window,
                    property,
                    kind,
                    bytes,
                ))?
                .check(),
            );
        }
        if self.transfers.len() == TRANSFERS {
            return Err(ClipboardError::Capacity);
        }
        let bytes = Outgoing::new(offer.data.clone(), index, self.chunk)?;
        platform(
            platform(
                connection.change_window_attributes(
                    window,
                    &ChangeWindowAttributesAux::new()
                        .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
                ),
            )?
            .check(),
        )?;
        let result = platform(
            platform(connection.change_property32(
                PropMode::REPLACE,
                window,
                property,
                self.atoms.INCR,
                &[bytes.length()],
            ))?
            .check(),
        );
        self.transfers.push(Transfer {
            window,
            property,
            kind,
            bytes,
            deadline: now + TIMEOUT,
        });
        if result.is_err() {
            self.remove(connection, self.transfers.len() - 1)?;
        }
        result
    }

    fn multiple(
        &mut self,
        connection: &impl Connection,
        request: &SelectionRequestEvent,
        now: Instant,
    ) -> Result<(), ClipboardError> {
        if request.property == x11rb::NONE
            || self.transfers.iter().any(|transfer| {
                transfer.window == request.requestor && transfer.property == request.property
            })
        {
            return Err(ClipboardError::InvalidData);
        }
        let reply = platform(
            platform(connection.get_property(
                false,
                request.requestor,
                request.property,
                self.atoms.ATOM_PAIR,
                0,
                MULTIPLE_PAIRS * 2,
            ))?
            .reply(),
        )?;
        if reply.type_ != self.atoms.ATOM_PAIR
            || reply.format != 32
            || reply.bytes_after != 0
            || !reply.value_len.is_multiple_of(2)
        {
            return Err(ClipboardError::InvalidData);
        }
        let mut pairs = Vec::new();
        pairs
            .try_reserve_exact(reply.value_len as usize)
            .map_err(|_| ClipboardError::Capacity)?;
        pairs.extend(reply.value32().ok_or(ClipboardError::InvalidData)?);
        for index in (0..pairs.len()).step_by(2) {
            let target = pairs[index];
            let property = pairs[index + 1];
            if property == request.property
                || (1..index).step_by(2).any(|other| pairs[other] == property)
                || target == self.atoms.MULTIPLE
                || self
                    .convert(connection, request.requestor, target, property, now)
                    .is_err()
            {
                pairs[index + 1] = x11rb::NONE;
            }
        }
        platform(
            platform(connection.change_property32(
                PropMode::REPLACE,
                request.requestor,
                request.property,
                self.atoms.ATOM_PAIR,
                &pairs,
            ))?
            .check(),
        )
    }

    pub(super) fn expire(
        &mut self,
        connection: &impl Connection,
        now: Instant,
    ) -> Result<(), ClipboardError> {
        for index in (0..self.transfers.len()).rev() {
            if self.transfers[index].deadline <= now {
                self.remove(connection, index)?;
            }
        }
        Ok(())
    }

    fn remove(&mut self, connection: &impl Connection, index: usize) -> Result<(), ClipboardError> {
        let transfer = self.transfers.swap_remove(index);
        if !self
            .transfers
            .iter()
            .any(|other| other.window == transfer.window)
        {
            forget_destroyed(
                platform(connection.change_window_attributes(
                    transfer.window,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
                ))?
                .check(),
            )?;
        }
        Ok(())
    }
}
