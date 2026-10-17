use std::os::fd::{AsFd, OwnedFd};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
    protocol::{
        wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
        wl_keyboard, wl_pointer, wl_seat, wl_surface, wl_touch,
    },
};

#[derive(Default)]
pub(super) struct Server {
    pub(super) selection: Option<wl_data_source::WlDataSource>,
    pub(super) offered: Vec<String>,
    pub(super) serials: Vec<u32>,
    pub(super) device: Option<wl_data_device::WlDataDevice>,
    pub(super) received: Vec<String>,
    pub(super) pending: Vec<OwnedFd>,
    pub(super) globals: Vec<(String, wayland_server::backend::ObjectId)>,
    pub(super) surfaces: Vec<wl_surface::WlSurface>,
    pub(super) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(super) pointer: Option<wl_pointer::WlPointer>,
    pub(super) touch: Option<wl_touch::WlTouch>,
}

impl<I: Resource + 'static> GlobalDispatch<I, ()> for Server
where
    Self: Dispatch<I, ()>,
{
    fn bind(
        state: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<I>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        let resource = init.init(resource, ());
        state
            .globals
            .push((I::interface().name.to_owned(), resource.id()));
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
        state: &mut Self,
        _: &Client,
        _: &wl_seat::WlSeat,
        request: wl_seat::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_seat::Request::GetKeyboard { id } => state.keyboard = Some(init.init(id, ())),
            wl_seat::Request::GetPointer { id } => state.pointer = Some(init.init(id, ())),
            wl_seat::Request::GetTouch { id } => state.touch = Some(init.init(id, ())),
            wl_seat::Request::Release => (),
            _ => panic!("unexpected seat request"),
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_compositor::WlCompositor,
        request: wl_compositor::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_compositor::Request::CreateSurface { id } => state.surfaces.push(init.init(id, ())),
            _ => panic!("unexpected compositor request"),
        }
    }
}

macro_rules! destroy_only {
    ($module:ident, $proxy:ident, $request:ident) => {
        impl Dispatch<$module::$proxy, ()> for Server {
            fn request(
                _: &mut Self,
                _: &Client,
                _: &$module::$proxy,
                request: $module::Request,
                _: &(),
                _: &DisplayHandle,
                _: &mut DataInit<'_, Self>,
            ) {
                assert!(matches!(request, $module::Request::$request));
            }
        }
    };
}
destroy_only!(wl_keyboard, WlKeyboard, Release);
destroy_only!(wl_pointer, WlPointer, Release);
destroy_only!(wl_touch, WlTouch, Release);
destroy_only!(wl_surface, WlSurface, Destroy);
