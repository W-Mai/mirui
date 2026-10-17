use std::os::fd::{AsFd, OwnedFd};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
    protocol::{wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_seat},
};

#[derive(Default)]
pub(super) struct Server {
    pub(super) selection: Option<wl_data_source::WlDataSource>,
    pub(super) offered: Vec<String>,
    pub(super) serials: Vec<u32>,
    pub(super) device: Option<wl_data_device::WlDataDevice>,
    pub(super) received: Vec<String>,
    pub(super) pending: Vec<OwnedFd>,
}

impl<I: Resource + 'static> GlobalDispatch<I, ()> for Server
where
    Self: Dispatch<I, ()>,
{
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<I>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }
}

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_data_device_manager::WlDataDeviceManager,
        request: wl_data_device_manager::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_data_device_manager::Request::CreateDataSource { id } => {
                init.init(id, ());
            }
            wl_data_device_manager::Request::GetDataDevice { id, .. } => {
                state.device = Some(init.init(id, ()));
            }
            _ => panic!("unexpected manager request"),
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, Option<wl_data_source::WlDataSource>> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_data_offer::WlDataOffer,
        request: wl_data_offer::Request,
        source: &Option<wl_data_source::WlDataSource>,
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_data_offer::Request::Receive { mime_type, fd } => {
                state.received.push(mime_type.clone());
                if let Some(source) = source {
                    source.send(mime_type, fd.as_fd());
                } else {
                    state.pending.push(fd);
                }
            }
            wl_data_offer::Request::Destroy => (),
            _ => panic!("unexpected clipboard offer request"),
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_data_device::WlDataDevice,
        request: wl_data_device::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_data_device::Request::SetSelection { source, serial } => {
                state.selection = source;
                state.serials.push(serial);
            }
            wl_data_device::Request::Release => (),
            _ => panic!("unexpected data device request"),
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_data_source::WlDataSource,
        request: wl_data_source::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_data_source::Request::Offer { mime_type } => state.offered.push(mime_type),
            wl_data_source::Request::Destroy => (),
            _ => panic!("unexpected data source request"),
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Server {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &wl_seat::WlSeat,
        request: wl_seat::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        assert!(matches!(request, wl_seat::Request::Release));
    }
}
