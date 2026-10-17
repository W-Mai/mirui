use std::time::{Duration, Instant};

use mirui::surface::{InputEvent, Surface};
use mirui::ui::clipboard::ClipboardInput;
use objc2::{MainThreadMarker, class, msg_send, rc::Retained, runtime::AnyObject, sel};
use objc2_app_kit::NSResponder;

#[cfg(feature = "wgpu")]
pub(super) fn verify_wgpu() {
    use mirui::surface::wgpu_surface::WgpuSurface;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let surface = WgpuSurface::new("Mirui menu contract", 240, 120).unwrap();
    let window = surface.state().unwrap().window.clone();
    let RawWindowHandle::AppKit(handle) = window.window_handle().unwrap().as_raw() else {
        panic!("AppKit window expected");
    };
    // The retained Window keeps its native view alive throughout this contract.
    let host = unsafe { Retained::<NSResponder>::retain(handle.ns_view.as_ptr().cast()) }.unwrap();
    verify_surface(surface, host, true, || {});
    drop(window);
}

#[cfg(feature = "sdl")]
fn sdl_view(expected_title: &str) -> Retained<NSResponder> {
    use objc2_foundation::{NSArray, NSString};

    let application: Retained<AnyObject> =
        unsafe { msg_send![class!(NSApplication), sharedApplication] };
    let windows: Retained<NSArray<NSResponder>> = unsafe { msg_send![&*application, windows] };
    for window in windows {
        let title: Retained<NSString> = unsafe { msg_send![&*window, title] };
        if title.to_string() == expected_title {
            let view: Option<Retained<NSResponder>> = unsafe { msg_send![&*window, contentView] };
            if let Some(view) = view {
                return view;
            }
        }
    }
    panic!("test SDL window missing");
}

#[cfg(feature = "sdl")]
pub(super) fn verify_sdl(system_lookup: bool) {
    for index in 0..2 {
        let title = format!("Mirui menu contract {index}");
        let mut surface = mirui::surface::sdl::SdlSurface::new(&title, 240, 120);
        let view = sdl_view(&title);
        verify_sdl_routes(&mut surface, &view);
        verify_surface(surface, view, system_lookup, || unsafe {
            sdl2::sys::SDL_StartTextInput()
        });
    }
}

#[cfg(feature = "sdl-gpu")]
pub(super) fn verify_sdl_gpu(system_lookup: bool) {
    for index in 0..2 {
        let title = format!("Mirui menu contract {index}");
        let mut surface = mirui::surface::sdl_gpu::SdlGpuSurface::new(&title, 240, 120).unwrap();
        let view = sdl_view(&title);
        verify_sdl_routes(&mut surface, &view);
        verify_surface(surface, view, system_lookup, || unsafe {
            sdl2::sys::SDL_StartTextInput()
        });
    }
}

#[cfg(feature = "sdl")]
fn verify_sdl_routes(surface: &mut impl Surface, host: &NSResponder) {
    use sdl2::sys::{
        SDL_Event, SDL_EventType, SDL_PeepEvents, SDL_PushEvent, SDL_RegisterEvents, SDL_UserEvent,
        SDL_eventaction,
    };

    assert!(unsafe { host.tryToPerform_with(sel!(copy:), None) });
    let mut raw = core::mem::MaybeUninit::<SDL_Event>::uninit();
    assert_eq!(
        unsafe {
            SDL_PeepEvents(
                raw.as_mut_ptr(),
                1,
                SDL_eventaction::SDL_GETEVENT,
                SDL_EventType::SDL_USEREVENT as u32,
                SDL_EventType::SDL_LASTEVENT as u32 - 1,
            )
        },
        1
    );
    let original = unsafe { raw.assume_init().user };
    let foreign_kind = unsafe { SDL_RegisterEvents(1) };
    assert_ne!(foreign_kind, u32::MAX);
    for user in [
        SDL_UserEvent {
            windowID: original.windowID.wrapping_add(1),
            ..original
        },
        SDL_UserEvent {
            type_: foreign_kind,
            ..original
        },
        SDL_UserEvent {
            code: 100,
            ..original
        },
        SDL_UserEvent {
            data1: core::ptr::dangling_mut(),
            ..original
        },
    ] {
        let mut raw = SDL_Event { user };
        assert_eq!(unsafe { SDL_PushEvent(&mut raw) }, 1);
    }
    while let Some(input) = surface.poll_event() {
        assert!(
            !matches!(input, InputEvent::ClipboardInput(_)),
            "foreign menu action"
        );
    }
}

fn verify_surface(
    mut surface: impl Surface,
    host: Retained<NSResponder>,
    system_lookup: bool,
    start_text: impl Fn(),
) {
    let mtm = MainThreadMarker::new().expect("menu contracts run on the main thread");
    let application: Retained<AnyObject> =
        unsafe { msg_send![class!(NSApplication), sharedApplication] };
    let native_window: Retained<AnyObject> = unsafe { msg_send![&*host, window] };
    surface.frame_end().unwrap();
    while let Some(input) = surface.poll_event() {
        assert!(
            !matches!(input, InputEvent::ClipboardInput(_)),
            "retired action"
        );
    }
    if system_lookup {
        unsafe {
            let _: () = msg_send![&*application, activateIgnoringOtherApps: true];
            let _: () = msg_send![&*native_window, makeKeyAndOrderFront: None::<&AnyObject>];
            let _: () = msg_send![&*native_window, makeKeyWindow];
            let focused: bool = msg_send![&*native_window, makeFirstResponder: &*host];
            assert!(focused, "canvas must accept native focus");
        }
        let focus_deadline = Instant::now() + Duration::from_secs(5);
        while !unsafe { msg_send![&*native_window, isKeyWindow] } {
            surface.frame_end().unwrap();
            while let Some(input) = surface.poll_event() {
                assert!(
                    !matches!(input, InputEvent::ClipboardInput(_)),
                    "retired action"
                );
            }
            if Instant::now() >= focus_deadline {
                let active: bool = unsafe { msg_send![&*application, isActive] };
                let visible: bool = unsafe { msg_send![&*native_window, isVisible] };
                let can_key: bool = unsafe { msg_send![&*native_window, canBecomeKeyWindow] };
                panic!(
                    "native window did not become key: active={active}, visible={visible}, can_key={can_key}"
                );
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let bridge = unsafe { host.nextResponder() }.expect("installed menu responder");
    let original = unsafe { bridge.nextResponder() };
    let before = NSResponder::new(mtm);
    let after = NSResponder::new(mtm);
    unsafe {
        before.setNextResponder(Some(&bridge));
        host.setNextResponder(Some(&before));
        after.setNextResponder(original.as_deref());
        bridge.setNextResponder(Some(&after));
    }

    for editing in [false, true] {
        if editing && system_lookup {
            start_text();
        }
        for (selector, expected) in [
            (sel!(copy:), ClipboardInput::copy()),
            (sel!(cut:), ClipboardInput::cut()),
            (sel!(paste:), ClipboardInput::paste_requested()),
            (sel!(selectAll:), ClipboardInput::select_all()),
        ] {
            // Nil target uses AppKit's normal menu target lookup, not direct bridge invocation.
            let sent: bool = if system_lookup {
                unsafe {
                    msg_send![&*application, sendAction: selector, to: None::<&AnyObject>, from: None::<&AnyObject>]
                }
            } else {
                unsafe { host.tryToPerform_with(selector, None) }
            };
            assert!(sent, "AppKit could not find the menu action target");
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                surface.frame_end().unwrap();
                if let Some(InputEvent::ClipboardInput(input)) = surface.poll_event() {
                    assert_eq!(format!("{input:?}"), format!("{expected:?}"));
                    break;
                }
                assert!(Instant::now() < deadline, "native action was not delivered");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        surface.frame_end().unwrap();
        while let Some(input) = surface.poll_event() {
            assert!(
                !matches!(input, InputEvent::ClipboardInput(_)),
                "duplicate action"
            );
        }
    }

    // A queued action must not keep the Surface alive or interfere with chain retirement.
    assert!(unsafe { host.tryToPerform_with(sel!(paste:), None) });
    drop(surface);
    // SDL may close the native window and clear the view link after detaching the bridge.
    if let Some(next) = unsafe { host.nextResponder() } {
        assert!(core::ptr::eq(&*next, &*before));
    }
    assert!(core::ptr::eq(
        &*unsafe { before.nextResponder() }.unwrap(),
        &*after
    ));
    assert!(unsafe { bridge.nextResponder() }.is_none());
    unsafe {
        host.setNextResponder(original.as_deref());
        before.setNextResponder(None);
        after.setNextResponder(None);
    }
    drop(bridge);
    println!(
        "macOS menu: native responder, event-loop delivery and scoped teardown passed (system target lookup: {system_lookup})"
    );
}
