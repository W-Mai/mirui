use std::{
    cell::Cell,
    os::fd::{AsFd, OwnedFd},
    rc::Rc,
    time::Instant,
};

use wayland_client::protocol::wl_data_offer::WlDataOffer;

use super::transfer::{Budget, Incoming, TIMEOUT};
use crate::ui::clipboard::{
    ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem, validate_mime,
};

struct Format {
    native: String,
    mime: String,
    priority: u8,
}

pub(super) struct Offer {
    proxy: WlDataOffer,
    formats: Vec<Format>,
    valid: Cell<bool>,
}

impl Offer {
    pub(super) fn new(proxy: WlDataOffer) -> Self {
        Self {
            proxy,
            formats: Vec::new(),
            valid: Cell::new(true),
        }
    }

    pub(super) fn advertise(&mut self, native: String) -> Result<(), ClipboardError> {
        if !self.valid.get() {
            return Err(ClipboardError::Unavailable);
        }
        let name = native.to_ascii_lowercase();
        let (mime, priority) = match name.as_str() {
            "text/plain;charset=utf-8" => (String::from("text/plain"), 0),
            "text/plain" => (name, 1),
            "utf8_string" => (String::from("text/plain"), 2),
            _ if validate_mime(&name).is_ok() => (name, 0),
            _ => return Ok(()),
        };
        if let Some(old) = self.formats.iter_mut().find(|old| old.mime == mime) {
            if priority < old.priority {
                *old = Format {
                    native,
                    mime,
                    priority,
                };
            }
        } else {
            self.formats
                .try_reserve(1)
                .map_err(|_| ClipboardError::Capacity)?;
            self.formats.push(Format {
                native,
                mime,
                priority,
            });
        }
        Ok(())
    }

    pub(super) fn invalidate(&self) {
        if self.valid.replace(false) {
            self.proxy.destroy();
        }
    }
}

impl Drop for Offer {
    fn drop(&mut self) {
        self.invalidate();
    }
}

pub(super) struct Read {
    offer: Rc<Offer>,
    index: usize,
    incoming: Option<Incoming>,
    item: Option<ClipboardItem>,
    bytes: usize,
    deadline: Instant,
    finished: bool,
}

impl Read {
    pub(super) fn begin(offer: Rc<Offer>, now: Instant) -> Result<Self, ClipboardError> {
        if !offer.valid.get() {
            return Err(ClipboardError::Unavailable);
        }
        if offer.formats.is_empty() {
            return Err(ClipboardError::Unsupported);
        }
        let item = ClipboardItem::with_capacity(offer.formats.len())?;
        let mut read = Self {
            offer,
            index: 0,
            incoming: None,
            item: Some(item),
            bytes: 0,
            deadline: now + TIMEOUT,
            finished: false,
        };
        read.request()?;
        Ok(read)
    }

    fn request(&mut self) -> Result<(), ClipboardError> {
        let (reader, writer) = pipe()?;
        self.incoming = Some(Incoming::new(reader, self.deadline)?);
        self.offer.proxy.receive(
            self.offer.formats[self.index].native.clone(),
            writer.as_fd(),
        );
        Ok(())
    }

    pub(super) fn poll(
        &mut self,
        now: Instant,
        budget: &mut Budget,
    ) -> Result<Option<ClipboardData>, ClipboardError> {
        let result = self.receive(now, budget);
        if !matches!(result, Ok(None)) {
            self.finished = true;
            self.incoming = None;
            self.item = None;
        }
        result
    }

    fn receive(
        &mut self,
        now: Instant,
        budget: &mut Budget,
    ) -> Result<Option<ClipboardData>, ClipboardError> {
        if self.finished {
            return Err(ClipboardError::InvalidData);
        }
        if !self.offer.valid.get() || now >= self.deadline {
            return Err(ClipboardError::Unavailable);
        }
        let incoming = self.incoming.as_mut().ok_or(ClipboardError::InvalidData)?;
        if !incoming.poll(now, budget)? {
            return Ok(None);
        }
        let bytes = self.incoming.take().unwrap().finish()?;
        let format = &self.offer.formats[self.index];
        if format.mime == "text/plain" {
            core::str::from_utf8(&bytes).map_err(|_| ClipboardError::InvalidData)?;
        }
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or(ClipboardError::Capacity)?;
        self.item
            .as_mut()
            .unwrap()
            .push(ClipboardFormat::new(format.mime.clone(), bytes))?;
        self.index += 1;
        if self.index == self.offer.formats.len() {
            let mut data = ClipboardData::with_capacity(1, self.bytes)?;
            data.push(self.item.take().unwrap())?;
            Ok(Some(data))
        } else {
            self.request()?;
            Ok(None)
        }
    }
}

fn pipe() -> Result<(OwnedFd, OwnedFd), ClipboardError> {
    #[cfg(target_os = "linux")]
    let pair = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC)
        .map_err(|_| ClipboardError::Platform)?;
    #[cfg(target_os = "macos")]
    let pair = {
        let pair = rustix::pipe::pipe().map_err(|_| ClipboardError::Platform)?;
        for fd in [&pair.0, &pair.1] {
            rustix::io::fcntl_setfd(fd, rustix::io::FdFlags::CLOEXEC)
                .map_err(|_| ClipboardError::Platform)?;
        }
        pair
    };
    Ok(pair)
}
