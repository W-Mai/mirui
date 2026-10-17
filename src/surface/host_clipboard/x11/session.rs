use std::{sync::Arc, time::Instant};

use x11rb::{
    connection::Connection,
    protocol::{Event, xproto::*},
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
    transfer: Read,
}

pub(super) struct Session<C: Connection> {
    connection: C,
    atoms: Atoms,
    root: Window,
    owner: Window,
    source: Source,
    read: Option<PendingRead>,
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
        let owner = create_window(&connection, root)?;
        let source = Source::new(&connection, owner, atoms)?;
        Ok(Self {
            connection,
            atoms,
            root,
            owner,
            source,
            read: None,
        })
    }

    pub(super) fn connection(&self) -> &C {
        &self.connection
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.source
            .deadline()
            .into_iter()
            .chain(self.read.as_ref().map(|read| read.transfer.deadline()))
            .min()
    }

    pub(super) fn request_time(&self) -> Result<(), ClipboardError> {
        platform(
            platform(self.connection.change_property32(
                PropMode::REPLACE,
                self.owner,
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

    pub(super) fn begin_read(
        &mut self,
        time: Timestamp,
        now: Instant,
    ) -> Result<(), ClipboardError> {
        if self.read.is_some() {
            return Err(ClipboardError::Unavailable);
        }
        let window = create_window(&self.connection, self.root)?;
        match Read::begin(
            &self.connection,
            self.atoms,
            window,
            self.atoms.CLIPBOARD,
            time,
            now,
        ) {
            Ok(transfer) => {
                self.read = Some(PendingRead { window, transfer });
                Ok(())
            }
            Err(error) => {
                self.destroy_window(window)?;
                Err(error)
            }
        }
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
        if matches!(event, Event::DestroyNotify(event) if event.window == read.window) {
            return self.finish_read(Err(ClipboardError::Unavailable));
        }
        match read.transfer.event(&self.connection, event, now) {
            Ok(Some(data)) => self.finish_read(Ok(data)),
            Ok(None) => Ok(None),
            Err(error) => self.finish_read(Err(error)),
        }
    }

    pub(super) fn expire(&mut self, now: Instant) -> Result<Completion, ClipboardError> {
        self.source.expire(&self.connection, now)?;
        if self
            .read
            .as_ref()
            .is_some_and(|read| read.transfer.expired(now))
        {
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
