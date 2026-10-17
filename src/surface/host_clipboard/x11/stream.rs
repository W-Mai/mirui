use std::{
    cell::Cell,
    io::{self, IoSlice},
    os::fd::{AsFd, OwnedFd},
    time::Instant,
};

use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    io::Errno,
    net::{Shutdown, shutdown},
};
use x11rb::{
    rust_connection::{DefaultStream, PollMode, Stream},
    utils::RawFdContainer,
};

pub(super) struct Cancel(OwnedFd);

impl Drop for Cancel {
    fn drop(&mut self) {
        let _ = shutdown(&self.0, Shutdown::Both);
    }
}

pub(super) struct BoundedStream {
    inner: DefaultStream,
    deadline: Cell<Option<Instant>>,
    failure: Cell<Option<io::ErrorKind>>,
}

impl BoundedStream {
    pub(super) fn new(inner: DefaultStream) -> io::Result<(Self, Cancel)> {
        let cancel = Cancel(inner.as_fd().try_clone_to_owned()?);
        Ok((
            Self {
                inner,
                deadline: Cell::new(None),
                failure: Cell::new(None),
            },
            cancel,
        ))
    }

    pub(super) fn set_deadline(&self, deadline: Option<Instant>) {
        self.deadline.set(deadline);
    }

    fn fail<T>(&self, kind: io::ErrorKind) -> io::Result<T> {
        self.failure.set(Some(kind));
        Err(kind.into())
    }

    fn timeout(&self) -> io::Result<Option<Timespec>> {
        if let Some(kind) = self.failure.get() {
            return Err(kind.into());
        }
        self.deadline
            .get()
            .map(|deadline| {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return self.fail(io::ErrorKind::TimedOut);
                }
                Timespec::try_from(remaining).map_err(|_| io::ErrorKind::InvalidInput.into())
            })
            .transpose()
    }

    fn transfer(&self, operation: impl FnOnce() -> io::Result<usize>) -> io::Result<usize> {
        self.timeout()?;
        match operation() {
            Err(error) if error.kind() != io::ErrorKind::WouldBlock => {
                self.failure.set(Some(error.kind()));
                Err(error)
            }
            result => result,
        }
    }
}

impl Stream for BoundedStream {
    fn poll(&self, mode: PollMode) -> io::Result<()> {
        let mut flags = PollFlags::empty();
        if mode.readable() {
            flags |= PollFlags::IN;
        }
        if mode.writable() {
            flags |= PollFlags::OUT;
        }
        let mut fds = [PollFd::new(&self.inner, flags)];
        loop {
            let timeout = self.timeout()?;
            match poll(&mut fds, timeout.as_ref()) {
                Ok(0) | Err(Errno::INTR) => continue,
                Ok(_) => {
                    if fds[0]
                        .revents()
                        .intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL)
                    {
                        return self.fail(io::ErrorKind::ConnectionAborted);
                    }
                    self.timeout()?;
                    return Ok(());
                }
                Err(error) => return self.fail(io::Error::from(error).kind()),
            }
        }
    }

    fn read(&self, buf: &mut [u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        let length = self.transfer(|| self.inner.read(buf, fds))?;
        if length == 0 && !buf.is_empty() {
            return self.fail(io::ErrorKind::UnexpectedEof);
        }
        Ok(length)
    }

    fn write(&self, buf: &[u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.transfer(|| self.inner.write(buf, fds))
    }

    fn write_vectored(
        &self,
        bufs: &[IoSlice<'_>],
        fds: &mut Vec<RawFdContainer>,
    ) -> io::Result<usize> {
        self.transfer(|| self.inner.write_vectored(bufs, fds))
    }
}
