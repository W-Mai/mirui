use std::{sync::Arc, time::Instant};

use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xfixes::{self, ConnectionExt as _, SelectionEventMask},
        xproto::*,
    },
    wrapper::ConnectionExt as _,
};

use crate::ui::clipboard::{ClipboardData, ClipboardError};

use super::{
    protocol::{Atoms, forget_destroyed, platform},
    read::Read,
    source::Source,
};

type Completion = Option<Result<ClipboardData, ClipboardError>>;

struct PendingRead {
    window: Window,
    deadline: Instant,
    phase: ReadPhase,
}

enum ReadPhase {
    Starting,
    Reading(Read),
    Confirming(ClipboardData),
}

pub(super) struct Session<C: Connection> {
    connection: C,
    atoms: Atoms,
    root: Window,
    owner: Window,
    source: Source,
    read: Option<PendingRead>,
    selection_tracking: bool,
}

fn create_window(connection: &impl Connection, root: Window) -> Result<Window, ClipboardError> {
    let window = platform(connection.generate_id())?;
    platform(
        platform(
            connection.create_window(
                0,
                window,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new()
                    .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
            ),
        )?
        .check(),
    )?;
    Ok(window)
}

impl<C: Connection> Session<C> {
    pub(super) fn new(connection: C, screen: usize) -> Result<Self, ClipboardError> {
        let root = connection
            .setup()
            .roots
            .get(screen)
            .ok_or(ClipboardError::Unavailable)?
            .root;
        let atoms = platform(platform(Atoms::new(&connection))?.reply())?;
        let selection_tracking =
            if platform(connection.extension_information(xfixes::X11_EXTENSION_NAME))?.is_some() {
                platform(platform(connection.xfixes_query_version(1, 0))?.reply())?.major_version
                    >= 1
            } else {
                false
            };
        let owner = create_window(&connection, root)?;
        let source = Source::new(&connection, owner, atoms)?;
        Ok(Self {
            connection,
            atoms,
            root,
            owner,
            source,
            read: None,
            selection_tracking,
        })
    }

    pub(super) fn connection(&self) -> &C {
        &self.connection
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.source
            .deadline()
            .into_iter()
            .chain(self.read.as_ref().map(|read| read.deadline))
            .min()
    }

    pub(super) fn request_time(&self) -> Result<(), ClipboardError> {
        self.mark(self.owner)
    }

    fn mark(&self, window: Window) -> Result<(), ClipboardError> {
        platform(
            platform(self.connection.change_property32(
                PropMode::REPLACE,
                window,
                self.atoms.TIMESTAMP,
                AtomEnum::INTEGER,
                &[0],
            ))?
            .check(),
        )?;
        platform(self.connection.flush())
    }

    pub(super) fn timestamp(&self, event: &Event) -> Option<Timestamp> {
        match event {
            Event::PropertyNotify(event)
                if event.window == self.owner
                    && event.atom == self.atoms.TIMESTAMP
                    && event.state == Property::NEW_VALUE
                    && event.time != x11rb::CURRENT_TIME =>
            {
                Some(event.time)
            }
            _ => None,
        }
    }

    pub(super) fn write(
        &mut self,
        data: Arc<ClipboardData>,
        time: Timestamp,
    ) -> Result<(), ClipboardError> {
        self.source.claim(&self.connection, data, time)
    }

    pub(super) fn begin_read(&mut self, deadline: Instant) -> Result<(), ClipboardError> {
        if self.read.is_some() {
            return Err(ClipboardError::Unavailable);
        }
        let window = create_window(&self.connection, self.root)?;
        let prepare = || {
            if self.selection_tracking {
                platform(
                    platform(self.connection.xfixes_select_selection_input(
                        window,
                        self.atoms.CLIPBOARD,
                        SelectionEventMask::SET_SELECTION_OWNER
                            | SelectionEventMask::SELECTION_WINDOW_DESTROY
                            | SelectionEventMask::SELECTION_CLIENT_CLOSE,
                    ))?
                    .check(),
                )?;
            }
            self.mark(window)
        };
        if let Err(error) = prepare() {
            self.destroy_window(window)?;
            return Err(error);
        }
        self.read = Some(PendingRead {
            window,
            deadline,
            phase: ReadPhase::Starting,
        });
        Ok(())
    }

    pub(super) fn event(
        &mut self,
        event: &Event,
        now: Instant,
    ) -> Result<Completion, ClipboardError> {
        self.source.event(&self.connection, event, now)?;
        let Some(read) = &mut self.read else {
            return Ok(None);
        };
        if now >= read.deadline
            || matches!(event, Event::DestroyNotify(event) if event.window == read.window)
            || (self.selection_tracking
                && !matches!(read.phase, ReadPhase::Starting)
                && matches!(event, Event::XfixesSelectionNotify(event)
                    if event.window == read.window && event.selection == self.atoms.CLIPBOARD))
        {
            return self.finish_read(Err(ClipboardError::Unavailable));
        }
        let marker = match event {
            Event::PropertyNotify(event)
                if event.window == read.window
                    && event.atom == self.atoms.TIMESTAMP
                    && event.state == Property::NEW_VALUE
                    && event.time != x11rb::CURRENT_TIME =>
            {
                Some(event.time)
            }
            _ => None,
        };
        match &mut read.phase {
            ReadPhase::Starting => {
                if let Some(time) = marker {
                    match Read::begin(
                        &self.connection,
                        self.atoms,
                        read.window,
                        self.atoms.CLIPBOARD,
                        time,
                        read.deadline,
                        !self.selection_tracking,
                    ) {
                        Ok(transfer) => read.phase = ReadPhase::Reading(transfer),
                        Err(error) => return self.finish_read(Err(error)),
                    }
                }
            }
            ReadPhase::Reading(transfer) => match transfer.event(&self.connection, event, now) {
                Ok(Some(data)) if self.selection_tracking => {
                    let window = read.window;
                    read.phase = ReadPhase::Confirming(data);
                    if let Err(error) = self.mark(window) {
                        return self.finish_read(Err(error));
                    }
                }
                Ok(Some(data)) => return self.finish_read(Ok(data)),
                Ok(None) => {}
                Err(error) => return self.finish_read(Err(error)),
            },
            ReadPhase::Confirming(_) if marker.is_some() => {
                let ReadPhase::Confirming(data) =
                    core::mem::replace(&mut read.phase, ReadPhase::Starting)
                else {
                    unreachable!()
                };
                return self.finish_read(Ok(data));
            }
            ReadPhase::Confirming(_) => {}
        }
        Ok(None)
    }

    pub(super) fn expire(&mut self, now: Instant) -> Result<Completion, ClipboardError> {
        self.source.expire(&self.connection, now)?;
        if self.read.as_ref().is_some_and(|read| now >= read.deadline) {
            self.finish_read(Err(ClipboardError::Unavailable))
        } else {
            Ok(None)
        }
    }

    fn finish_read(
        &mut self,
        result: Result<ClipboardData, ClipboardError>,
    ) -> Result<Completion, ClipboardError> {
        if let Some(read) = self.read.take() {
            self.destroy_window(read.window)?;
        }
        Ok(Some(result))
    }

    fn destroy_window(&mut self, window: Window) -> Result<(), ClipboardError> {
        self.source.forget_window(window);
        forget_destroyed(platform(self.connection.destroy_window(window))?.check())
    }
}
