use core::ptr;

use sdl2::{EventSubsystem, event::Event, raw_window_handle::SDL_SysWMinfo, video::Window};

use super::{Action, MainThreadMarker, NSResponder, Registration, Retained, msg_send};
use crate::surface::InputEvent;

pub(crate) struct SdlMenu {
    _registration: Registration,
    _events: EventSubsystem,
    kind: u32,
    window: u32,
}

impl SdlMenu {
    pub(crate) fn new(window: &Window, events: EventSubsystem) -> Result<Option<Self>, String> {
        if window.subsystem().current_video_driver() != "cocoa" {
            return Ok(None);
        }
        let mtm = MainThreadMarker::new().ok_or("SDL menu requires the main thread")?;
        // Reuse SDL's platform-specific WM layout; the generated sys union is host-independent.
        const { assert!(size_of::<SDL_SysWMinfo>() == size_of::<sdl2::sys::SDL_SysWMinfo>()) };
        const { assert!(align_of::<SDL_SysWMinfo>() == align_of::<sdl2::sys::SDL_SysWMinfo>()) };
        let mut info: SDL_SysWMinfo = unsafe { core::mem::zeroed() };
        unsafe {
            sdl2::sys::SDL_GetVersion(&mut info.version);
            if sdl2::sys::SDL_GetWindowWMInfo(
                window.raw(),
                (&mut info as *mut SDL_SysWMinfo).cast(),
            ) != sdl2::sys::SDL_bool::SDL_TRUE
            {
                return Err(sdl2::get_error());
            }
        }
        if info.subsystem != sdl2::raw_window_handle::SDL_SYSWM_TYPE::SDL_SYSWM_COCOA {
            return Err("SDL menu requires a Cocoa window".into());
        }
        let native = unsafe { Retained::<NSResponder>::retain(info.info.cocoa.window.cast()) }
            .ok_or("SDL native window unavailable")?;
        let view: Option<Retained<NSResponder>> = unsafe { msg_send![&*native, contentView] };
        let view = view.ok_or("SDL content view unavailable")?;
        let kind = unsafe { sdl2::sys::SDL_RegisterEvents(1) };
        if kind == u32::MAX {
            return Err("SDL user event types exhausted".into());
        }
        let id = window.id();
        let sender = events.event_sender();
        let registration = Registration::new(
            &view,
            move |action| {
                let code = match action {
                    Action::Copy => 0,
                    Action::Cut => 1,
                    Action::Paste => 2,
                    Action::SelectAll => 3,
                };
                if let Err(error) = sender.push_event(Event::User {
                    timestamp: 0,
                    window_id: id,
                    type_: kind,
                    code,
                    data1: ptr::null_mut(),
                    data2: ptr::null_mut(),
                }) {
                    crate::warn!(target: "mirui::clipboard", "native menu delivery failed: {}", error);
                }
            },
            mtm,
        );
        Ok(Some(Self {
            _registration: registration,
            _events: events,
            kind,
            window: id,
        }))
    }

    pub(crate) fn input(&self, event: &Event) -> Option<InputEvent> {
        let Event::User {
            type_,
            window_id,
            code,
            data1,
            data2,
            ..
        } = event
        else {
            return None;
        };
        if *type_ != self.kind || *window_id != self.window || !data1.is_null() || !data2.is_null()
        {
            return None;
        }
        let action = match code {
            0 => Action::Copy,
            1 => Action::Cut,
            2 => Action::Paste,
            3 => Action::SelectAll,
            _ => return None,
        };
        Some(InputEvent::ClipboardInput(action.input()))
    }
}

impl Drop for SdlMenu {
    fn drop(&mut self) {
        // This event type belongs only to this registration and never owns payload pointers.
        unsafe { sdl2::sys::SDL_FlushEvent(self.kind) };
    }
}
