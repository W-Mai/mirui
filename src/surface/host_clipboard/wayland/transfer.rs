use std::{os::fd::OwnedFd, sync::Arc, time::Instant};

use rustix::{
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    io::{Errno, read, write},
};

use crate::ui::clipboard::{ClipboardData, ClipboardError};

pub(super) struct Budget {
    bytes: usize,
    calls: usize,
}

impl Budget {
    pub(super) fn new(bytes: usize, calls: usize) -> Self {
        Self { bytes, calls }
    }

    fn next(&mut self, maximum: usize) -> Option<usize> {
        if self.bytes == 0 || self.calls == 0 {
            return None;
        }
        self.calls -= 1;
        Some(self.bytes.min(maximum))
    }

    fn advance(&mut self, bytes: usize) {
        self.bytes -= bytes;
    }
}

struct Transfer {
    fd: Option<OwnedFd>,
    deadline: Instant,
    result: Result<bool, ClipboardError>,
}

impl Transfer {
    fn new(fd: OwnedFd, deadline: Instant) -> Result<Self, ClipboardError> {
        let flags = fcntl_getfl(&fd).map_err(|_| ClipboardError::Platform)?;
        fcntl_setfl(&fd, flags | OFlags::NONBLOCK).map_err(|_| ClipboardError::Platform)?;
        Ok(Self {
            fd: Some(fd),
            deadline,
            result: Ok(false),
        })
    }

    fn poll(
        &mut self,
        now: Instant,
        progress: impl FnOnce(&OwnedFd) -> Result<bool, ClipboardError>,
    ) -> Result<bool, ClipboardError> {
        let Some(fd) = &self.fd else {
            return self.result;
        };
        self.result = if now >= self.deadline {
            Err(ClipboardError::Unavailable)
        } else {
            progress(fd)
        };
        if self.result != Ok(false) {
            self.fd = None;
        }
        self.result
    }
}

pub(super) struct Outgoing {
    io: Transfer,
    snapshot: Arc<ClipboardData>,
    format: usize,
    offset: usize,
}

impl Outgoing {
    pub(super) fn new(
        fd: OwnedFd,
        snapshot: Arc<ClipboardData>,
        format: usize,
        deadline: Instant,
    ) -> Result<Self, ClipboardError> {
        let [item] = snapshot.items() else {
            return Err(ClipboardError::Unsupported);
        };
        if item.formats().get(format).is_none() {
            return Err(ClipboardError::InvalidData);
        }
        Ok(Self {
            io: Transfer::new(fd, deadline)?,
            snapshot,
            format,
            offset: 0,
        })
    }

    pub(super) fn poll(
        &mut self,
        now: Instant,
        budget: &mut Budget,
    ) -> Result<bool, ClipboardError> {
        let bytes = self.snapshot.items()[0].formats()[self.format].bytes();
        self.io.poll(now, |fd| {
            while self.offset < bytes.len() {
                let Some(length) = budget.next(bytes.len() - self.offset) else {
                    return Ok(false);
                };
                match write(fd, &bytes[self.offset..self.offset + length]) {
                    Ok(0) => return Err(ClipboardError::Platform),
                    Ok(written) => {
                        self.offset += written;
                        budget.advance(written);
                    }
                    Err(Errno::INTR) => continue,
                    Err(Errno::AGAIN) => return Ok(false),
                    Err(_) => return Err(ClipboardError::Platform),
                }
            }
            Ok(true)
        })
    }
}

pub(super) struct Incoming {
    io: Transfer,
    bytes: Vec<u8>,
}

impl Incoming {
    pub(super) fn new(fd: OwnedFd, deadline: Instant) -> Result<Self, ClipboardError> {
        Ok(Self {
            io: Transfer::new(fd, deadline)?,
            bytes: Vec::new(),
        })
    }

    pub(super) fn poll(
        &mut self,
        now: Instant,
        budget: &mut Budget,
    ) -> Result<bool, ClipboardError> {
        self.io.poll(now, |fd| {
            let mut buffer = [0; 8192];
            while let Some(length) = budget.next(buffer.len()) {
                match read(fd, &mut buffer[..length]) {
                    Ok(0) => return Ok(true),
                    Ok(received) => {
                        budget.advance(received);
                        self.bytes
                            .try_reserve(received)
                            .map_err(|_| ClipboardError::Capacity)?;
                        self.bytes.extend_from_slice(&buffer[..received]);
                    }
                    Err(Errno::INTR) => continue,
                    Err(Errno::AGAIN) => return Ok(false),
                    Err(_) => return Err(ClipboardError::Platform),
                }
            }
            Ok(false)
        })
    }

    pub(super) fn finish(self) -> Result<Vec<u8>, ClipboardError> {
        if self.io.result? {
            Ok(self.bytes)
        } else {
            Err(ClipboardError::Unavailable)
        }
    }
}
