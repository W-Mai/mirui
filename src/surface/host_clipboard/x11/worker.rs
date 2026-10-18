use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use x11rb::{
    connection::Connection,
    rust_connection::{DefaultStream, RustConnection},
};

use crate::ui::clipboard::{
    ClipboardData, ClipboardDispatch, ClipboardError, ClipboardRead, ClipboardRequest,
    ClipboardResponse, ClipboardWrite,
};

use super::{
    protocol::{TIMEOUT, platform},
    session::Session,
    stream::{BoundedStream, Cancel},
};

#[derive(Default)]
struct Control {
    closed: bool,
    cancel: Option<Cancel>,
}

fn failed(request: ClipboardRequest, error: ClipboardError) -> ClipboardResponse {
    match request {
        ClipboardRequest::Write(request) => request.complete(Err(error)),
        ClipboardRequest::Read(request) => request.complete(Err(error)),
        #[cfg(test)]
        ClipboardRequest::DeferredRead(request, _) => request.complete(Err(error)),
    }
}

struct Work {
    request: Option<ClipboardRequest>,
    replies: SyncSender<ClipboardResponse>,
    deadline: Instant,
}

impl Work {
    fn respond(mut self, finish: impl FnOnce(ClipboardRequest) -> ClipboardResponse) {
        if let Some(request) = self.request.take() {
            let response = if Instant::now() >= self.deadline {
                failed(request, ClipboardError::Unavailable)
            } else {
                finish(request)
            };
            let _ = self.replies.try_send(response);
        }
    }

    fn fail(self, error: ClipboardError) {
        self.respond(|request| failed(request, error));
    }

    fn read(self, result: Result<ClipboardData, ClipboardError>) {
        self.respond(|request| match request {
            ClipboardRequest::Read(request) => request.complete(result),
            request => failed(request, ClipboardError::InvalidData),
        });
    }

    fn is_write(&self) -> bool {
        matches!(self.request, Some(ClipboardRequest::Write(_)))
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        if let Some(request) = self.request.take() {
            let error = if Instant::now() >= self.deadline {
                ClipboardError::Unavailable
            } else {
                ClipboardError::Platform
            };
            let _ = self.replies.try_send(failed(request, error));
        }
    }
}

pub(in super::super) struct Bridge {
    requests: SyncSender<Work>,
    replies: SyncSender<ClipboardResponse>,
    responses: Receiver<ClipboardResponse>,
    wake: UnixStream,
    control: Arc<Mutex<Control>>,
    thread: JoinHandle<()>,
    busy: bool,
}

impl Bridge {
    pub(in super::super) fn new(display: String, screen: usize) -> Result<Self, ClipboardError> {
        Self::spawn(screen, move |control, deadline| {
            connect(&display, screen, control, deadline)
        })
    }

    fn spawn(
        screen: usize,
        connect: impl FnOnce(
            &Mutex<Control>,
            Instant,
        ) -> Result<RustConnection<BoundedStream>, ClipboardError>
        + Send
        + 'static,
    ) -> Result<Self, ClipboardError> {
        let (wake, listener) = platform(UnixStream::pair())?;
        platform(wake.set_nonblocking(true))?;
        platform(listener.set_nonblocking(true))?;
        let (requests, receiver) = mpsc::sync_channel::<Work>(1);
        let (replies, responses) = mpsc::sync_channel(1);
        let control = Arc::new(Mutex::new(Control::default()));
        let worker_control = control.clone();
        let thread = platform(thread::Builder::new().name("mirui-clipboard".into()).spawn(
            move || {
                let Ok(first) = receiver.recv() else {
                    return;
                };
                match connect(&worker_control, first.deadline) {
                    Ok(connection) => {
                        let _ = run(
                            connection,
                            screen,
                            first,
                            receiver,
                            listener,
                            worker_control,
                        );
                    }
                    Err(error) => first.fail(error),
                }
            },
        ))?;
        Ok(Self {
            requests,
            replies,
            responses,
            wake,
            control,
            thread,
            busy: false,
        })
    }

    pub(in super::super) fn finished(&self) -> bool {
        self.thread.is_finished() && !self.busy
    }

    pub(in super::super) fn write(&mut self, request: ClipboardWrite) -> ClipboardDispatch {
        self.submit(ClipboardRequest::Write(request))
    }

    pub(in super::super) fn read(&mut self, request: ClipboardRead) -> ClipboardDispatch {
        self.submit(ClipboardRequest::Read(request))
    }

    pub(in super::super) fn poll(&mut self) -> Option<ClipboardResponse> {
        let response = self.responses.try_recv().ok()?;
        self.busy = false;
        Some(response)
    }

    fn submit(&mut self, request: ClipboardRequest) -> ClipboardDispatch {
        if self.busy {
            return ClipboardDispatch::Complete(failed(request, ClipboardError::Unavailable));
        }
        let work = Work {
            request: Some(request),
            replies: self.replies.clone(),
            deadline: Instant::now() + TIMEOUT,
        };
        match self.requests.try_send(work) {
            Ok(()) => {
                self.busy = true;
                if let Err(error) = self.wake.write_all(&[1])
                    && error.kind() != io::ErrorKind::WouldBlock
                {
                    let _ = self.wake.shutdown(std::net::Shutdown::Both);
                }
                ClipboardDispatch::Pending
            }
            Err(TrySendError::Full(mut work) | TrySendError::Disconnected(mut work)) => {
                let request = work.request.take().expect("unsent clipboard request");
                ClipboardDispatch::Complete(failed(request, ClipboardError::Unavailable))
            }
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let cancel = {
            let mut control = self
                .control
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            control.closed = true;
            control.cancel.take()
        };
        drop(cancel);
        let _ = self.wake.shutdown(std::net::Shutdown::Both);
    }
}

fn connect(
    display: &str,
    screen: usize,
    control: &Mutex<Control>,
    deadline: Instant,
) -> Result<RustConnection<BoundedStream>, ClipboardError> {
    use x11rb::reexports::x11rb_protocol::{parse_display::parse_display, xauth::get_auth};
    let display = platform(parse_display(Some(display)))?;
    for address in display.connect_instruction() {
        if control
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closed
            || Instant::now() >= deadline
        {
            return Err(ClipboardError::Unavailable);
        }
        let Ok((stream, (family, address))) = DefaultStream::connect(&address) else {
            continue;
        };
        let (stream, cancel) = platform(BoundedStream::new(stream))?;
        stream.set_deadline(Some(deadline));
        {
            let mut control = control.lock().unwrap_or_else(|error| error.into_inner());
            if control.closed {
                return Err(ClipboardError::Unavailable);
            }
            control.cancel = Some(cancel);
        }
        let (auth_name, auth_data) = get_auth(family, &address, display.display)
            .unwrap_or_default()
            .unwrap_or_default();
        return platform(RustConnection::connect_to_stream_with_auth_info(
            stream, screen, auth_name, auth_data,
        ));
    }
    Err(ClipboardError::Unavailable)
}

enum Phase {
    Clock { probe: Option<Instant> },
    Reading,
}

struct Pending {
    work: Work,
    phase: Phase,
}

fn expire_request(pending: &mut Option<Pending>, now: Instant) -> bool {
    if pending
        .as_ref()
        .is_some_and(|pending| now >= pending.work.deadline)
    {
        pending
            .take()
            .unwrap()
            .work
            .fail(ClipboardError::Unavailable);
        true
    } else {
        false
    }
}

fn drain_wake(mut wake: &UnixStream) -> Result<(), ClipboardError> {
    loop {
        match wake.read(&mut [0; 32]) {
            Ok(0) => return Err(ClipboardError::Unavailable),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(_) => return Err(ClipboardError::Platform),
        }
    }
}

fn wait(
    connection: &RustConnection<BoundedStream>,
    wake: &UnixStream,
    deadline: Option<Instant>,
) -> Result<(), ClipboardError> {
    let mut fds = [
        PollFd::new(connection.stream(), PollFlags::IN),
        PollFd::new(wake, PollFlags::IN),
    ];
    loop {
        let timeout = deadline
            .map(|deadline| Timespec::try_from(deadline.saturating_duration_since(Instant::now())))
            .transpose()
            .map_err(|_| ClipboardError::Platform)?;
        match poll(&mut fds, timeout.as_ref()) {
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return Err(ClipboardError::Platform),
            Ok(_) => return Ok(()),
        }
    }
}

fn run(
    connection: RustConnection<BoundedStream>,
    screen: usize,
    first: Work,
    requests: Receiver<Work>,
    wake: UnixStream,
    control: Arc<Mutex<Control>>,
) -> Result<(), ClipboardError> {
    if Instant::now() >= first.deadline {
        first.fail(ClipboardError::Unavailable);
        return Ok(());
    }
    let mut session = Session::new(connection, screen)?;
    let mut queued = Some(first);
    let mut pending: Option<Pending> = None;
    let mut last_write = None;
    let mut ready_event = None;
    loop {
        if control
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closed
        {
            return Ok(());
        }
        if expire_request(&mut pending, Instant::now()) {
            return Ok(());
        }
        session.connection().stream().set_deadline(Some(
            pending
                .as_ref()
                .map_or_else(|| Instant::now() + TIMEOUT, |pending| pending.work.deadline),
        ));
        drain_wake(&wake)?;
        if pending.is_none() {
            let work = match queued.take() {
                Some(work) => Some(work),
                None => match requests.try_recv() {
                    Ok(work) => Some(work),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return Ok(()),
                },
            };
            if let Some(work) = work {
                if Instant::now() >= work.deadline {
                    work.fail(ClipboardError::Unavailable);
                    return Ok(());
                }
                session
                    .connection()
                    .stream()
                    .set_deadline(Some(work.deadline));
                session.request_time()?;
                pending = Some(Pending {
                    work,
                    phase: Phase::Clock { probe: None },
                });
            }
        }
        let mut count = 0;
        while count < 64 {
            if pending.is_none() {
                session
                    .connection()
                    .stream()
                    .set_deadline(Some(Instant::now() + TIMEOUT));
            }
            let event = match ready_event.take() {
                Some(event) => Some(event),
                None => platform(session.connection().poll_for_event())?,
            };
            let Some(event) = event else {
                break;
            };
            count += 1;
            let now = Instant::now();
            if expire_request(&mut pending, now) {
                return Ok(());
            }
            if let Some(result) = session.event(&event, now)? {
                if let Some(pending) = pending.take() {
                    pending.work.read(result);
                }
            }
            if let Some(time) = session.timestamp(&event)
                && pending.as_ref().is_some_and(|pending| {
                    matches!(pending.phase, Phase::Clock { probe: None, .. })
                })
            {
                let mut current = pending.take().expect("pending timestamp request");
                if now >= current.work.deadline {
                    current.work.fail(ClipboardError::Unavailable);
                } else if current.work.is_write() && last_write == Some(time) {
                    if let Phase::Clock { probe } = &mut current.phase {
                        *probe = Some(now + Duration::from_millis(1));
                    }
                    pending = Some(current);
                } else if current.work.is_write() {
                    let deadline = current.work.deadline;
                    current.work.respond(|request| match request {
                        ClipboardRequest::Write(request) => request.complete_with(|data| {
                            session.write(Arc::new(data), time)?;
                            if Instant::now() >= deadline {
                                return Err(ClipboardError::Unavailable);
                            }
                            last_write = Some(time);
                            Ok(())
                        }),
                        request => failed(request, ClipboardError::InvalidData),
                    });
                } else {
                    match session.begin_read(time, current.work.deadline) {
                        Ok(()) => {
                            current.phase = Phase::Reading;
                            pending = Some(current);
                        }
                        Err(error) => current.work.fail(error),
                    }
                }
            }
        }
        let now = Instant::now();
        if expire_request(&mut pending, now) {
            return Ok(());
        }
        if let Some(result) = session.expire(now)? {
            if let Some(pending) = pending.take() {
                pending.work.read(result);
            }
        }
        let mut deadline = session.deadline();
        if let Some(current) = &mut pending
            && let Phase::Clock { probe } = &mut current.phase
        {
            if probe.is_some_and(|probe| now >= probe) {
                session.request_time()?;
                *probe = None;
                continue;
            }
            deadline = deadline
                .into_iter()
                .chain(Some(current.work.deadline))
                .chain(*probe)
                .min();
        }
        if count < 64 {
            ready_event = platform(session.connection().poll_for_event())?;
            if ready_event.is_some() {
                continue;
            }
            session.connection().stream().set_deadline(None);
            wait(session.connection(), &wake, deadline)?;
        }
    }
}

#[cfg(test)]
mod tests;
