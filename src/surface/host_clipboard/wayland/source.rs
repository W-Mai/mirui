use std::{os::fd::OwnedFd, sync::Arc, time::Instant};

use wayland_client::{
    Dispatch, QueueHandle,
    protocol::{
        wl_data_device::WlDataDevice,
        wl_data_device_manager::WlDataDeviceManager,
        wl_data_source::{self, WlDataSource},
    },
};

use super::transfer::{Budget, Outgoing, TIMEOUT};
use crate::ui::clipboard::{ClipboardData, ClipboardError};

const TRANSFERS: usize = 16;
const UTF8: &str = "text/plain;charset=utf-8";

struct Offer {
    proxy: WlDataSource,
    data: Arc<ClipboardData>,
}

impl Drop for Offer {
    fn drop(&mut self) {
        self.proxy.destroy();
    }
}

pub(super) struct Source {
    offer: Option<Offer>,
    transfers: [Option<Outgoing>; TRANSFERS],
    next: usize,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            offer: None,
            transfers: [const { None }; TRANSFERS],
            next: 0,
        }
    }
}

impl Source {
    pub(super) fn claim<State>(
        &mut self,
        manager: &WlDataDeviceManager,
        device: &WlDataDevice,
        serial: u32,
        data: Arc<ClipboardData>,
        queue: &QueueHandle<State>,
    ) -> Result<WlDataSource, ClipboardError>
    where
        State: Dispatch<WlDataSource, ()> + 'static,
    {
        data.validate()?;
        let [item] = data.items() else {
            return Err(ClipboardError::Unsupported);
        };
        for format in item.formats() {
            if format.mime().eq_ignore_ascii_case("text/plain") {
                core::str::from_utf8(format.bytes()).map_err(|_| ClipboardError::InvalidData)?;
            }
        }
        let proxy = manager.create_data_source(queue, ());
        for format in item.formats() {
            proxy.offer(format.mime().to_ascii_lowercase());
            if format.mime().eq_ignore_ascii_case("text/plain") {
                proxy.offer(UTF8.to_owned());
            }
        }
        device.set_selection(Some(&proxy), serial);
        self.offer = Some(Offer {
            proxy: proxy.clone(),
            data,
        });
        Ok(proxy)
    }

    pub(super) fn event(
        &mut self,
        proxy: &WlDataSource,
        event: wl_data_source::Event,
        now: Instant,
    ) -> Result<(), ClipboardError> {
        match event {
            wl_data_source::Event::Cancelled => {
                if self
                    .offer
                    .as_ref()
                    .is_some_and(|offer| offer.proxy == *proxy)
                {
                    self.offer = None;
                }
                Ok(())
            }
            wl_data_source::Event::Send { mime_type, fd } => self.send(proxy, &mime_type, fd, now),
            _ => Ok(()),
        }
    }

    fn send(
        &mut self,
        proxy: &WlDataSource,
        mime: &str,
        fd: OwnedFd,
        now: Instant,
    ) -> Result<(), ClipboardError> {
        let offer = self
            .offer
            .as_ref()
            .filter(|offer| offer.proxy == *proxy)
            .ok_or(ClipboardError::Unavailable)?;
        let mime = if mime.eq_ignore_ascii_case(UTF8) {
            "text/plain"
        } else {
            mime
        };
        let format = offer.data.items()[0]
            .formats()
            .iter()
            .position(|format| format.mime().eq_ignore_ascii_case(mime))
            .ok_or(ClipboardError::Unsupported)?;
        let slot = self
            .transfers
            .iter_mut()
            .find(|slot| slot.is_none())
            .ok_or(ClipboardError::Capacity)?;
        *slot = Some(Outgoing::new(
            fd,
            Arc::clone(&offer.data),
            format,
            now + TIMEOUT,
        )?);
        Ok(())
    }

    pub(super) fn poll(&mut self, now: Instant, budget: &mut Budget) -> Option<ClipboardError> {
        let mut failure = None;
        for step in 0..TRANSFERS {
            let slot = &mut self.transfers[(self.next + step) % TRANSFERS];
            if let Some(transfer) = slot {
                match transfer.poll(now, budget) {
                    Ok(false) => (),
                    result => {
                        if failure.is_none() {
                            failure = result.err();
                        }
                        *slot = None;
                    }
                }
            }
        }
        self.next = (self.next + 1) % TRANSFERS;
        failure
    }
}
