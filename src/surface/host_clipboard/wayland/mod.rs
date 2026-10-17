mod read;
mod seat;
mod session;
mod source;
#[cfg(test)]
mod tests;
mod transfer;

use std::{sync::Arc, time::Instant};
use wayland_client::{Connection, backend::ObjectId};

use crate::ui::clipboard::{
    ClipboardDispatch, ClipboardError, ClipboardRead, ClipboardResponse, ClipboardWrite,
};
use session::{Completion, Session};

enum Request {
    Read(ClipboardRead),
    Write(ClipboardResponse),
}

impl Request {
    fn complete(self, result: Result<Completion, ClipboardError>) -> ClipboardResponse {
        match self {
            Self::Read(request) => request.complete(match result {
                Ok(Completion::Read(data)) => Ok(data),
                Ok(Completion::Written) => Err(ClipboardError::InvalidData),
                Err(error) => Err(error),
            }),
            Self::Write(response) => match result {
                Ok(Completion::Written) => response,
                Ok(Completion::Read(_)) => response.reject(ClipboardError::InvalidData),
                Err(error) => response.reject(error),
            },
        }
    }
}

pub(super) struct Bridge {
    session: Option<Session>,
    pending: Option<Request>,
    failure: ClipboardError,
}

impl Bridge {
    pub(super) fn new(connection: Connection, target: ObjectId) -> Self {
        Self {
            session: Some(Session::new(connection, target)),
            pending: None,
            failure: ClipboardError::Unavailable,
        }
    }

    fn available(&mut self) -> Result<&mut Session, ClipboardError> {
        if self.pending.is_some() {
            return Err(ClipboardError::Capacity);
        }
        self.session.as_mut().ok_or(self.failure)
    }

    pub(super) fn write(&mut self, request: ClipboardWrite) -> ClipboardDispatch {
        let session = match self.available() {
            Ok(session) => session,
            Err(error) => return ClipboardDispatch::Complete(request.complete(Err(error))),
        };
        let mut started = false;
        let response = request.complete_with(|data| {
            session.write(Arc::new(data), Instant::now())?;
            started = true;
            Ok(())
        });
        if !started {
            return ClipboardDispatch::Complete(response);
        }
        self.pending = Some(Request::Write(response));
        ClipboardDispatch::Pending
    }

    pub(super) fn read(&mut self, request: ClipboardRead) -> ClipboardDispatch {
        match self
            .available()
            .and_then(|session| session.read(Instant::now()))
        {
            Ok(()) => {
                self.pending = Some(Request::Read(request));
                ClipboardDispatch::Pending
            }
            Err(error) => ClipboardDispatch::Complete(request.complete(Err(error))),
        }
    }

    pub(super) fn poll(&mut self) -> Option<ClipboardResponse> {
        let result = match self.session.as_mut()?.poll(Instant::now()) {
            Ok(result) => result?,
            Err(error) => {
                self.failure = error;
                return self.close();
            }
        };
        Some(self.pending.take()?.complete(result))
    }

    pub(super) fn close(&mut self) -> Option<ClipboardResponse> {
        self.session = None;
        self.pending
            .take()
            .map(|request| request.complete(Err(self.failure)))
    }
}
