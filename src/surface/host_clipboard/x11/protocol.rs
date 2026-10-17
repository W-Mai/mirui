use std::time::Duration;

use x11rb::{connection::Connection, protocol::xproto::*};

use crate::ui::clipboard::ClipboardError;

use super::transfer::{Property, PropertyReader};

pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

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

pub(super) fn platform<T, E>(result: Result<T, E>) -> Result<T, ClipboardError> {
    result.map_err(|_| ClipboardError::Platform)
}

pub(super) fn read_property(
    connection: &impl Connection,
    window: Window,
    property: Atom,
) -> Result<Property, ClipboardError> {
    let mut reader = PropertyReader::default();
    while let Some(offset) = reader.next_offset()? {
        let reply = platform(
            platform(connection.get_property(
                false,
                window,
                property,
                AtomEnum::ANY,
                offset,
                16 * 1024,
            ))?
            .reply(),
        )?;
        reader.append(
            reply.type_,
            reply.format,
            reply.value_len,
            reply.bytes_after,
            &reply.value,
        )?;
    }
    reader.finish()
}
