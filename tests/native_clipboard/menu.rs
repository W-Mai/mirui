use std::time::{Duration, Instant};

use mirui::surface::wgpu_surface::WgpuSurface;
use mirui::surface::{InputEvent, Surface};
use mirui::ui::clipboard::ClipboardInput;
use objc2::{MainThreadMarker, class, msg_send, rc::Retained, runtime::AnyObject, sel};
use objc2_app_kit::NSResponder;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(super) fn verify() {
    let mtm = MainThreadMarker::new().expect("menu contracts run on the main thread");
    let mut surface = WgpuSurface::new("Mirui menu contract", 240, 120).unwrap();
    let window = surface.state().unwrap().window.clone();
    let RawWindowHandle::AppKit(handle) = window.window_handle().unwrap().as_raw() else {
        panic!("AppKit window expected");
    };
    // The retained Window keeps its native view alive throughout this contract.
    let host = unsafe { Retained::<NSResponder>::retain(handle.ns_view.as_ptr().cast()) }.unwrap();
    let application: Retained<AnyObject> =
        unsafe { msg_send![class!(NSApplication), sharedApplication] };
    let native_window: Retained<AnyObject> = unsafe { msg_send![&*host, window] };
    unsafe {
        let _: () = msg_send![&*application, activateIgnoringOtherApps: true];
        let _: () = msg_send![&*native_window, makeKeyAndOrderFront: None::<&AnyObject>];
        let focused: bool = msg_send![&*native_window, makeFirstResponder: &*host];
        assert!(focused, "canvas must accept native focus");
    }
    let focus_deadline = Instant::now() + Duration::from_secs(2);
    while !unsafe { msg_send![&*native_window, isKeyWindow] } {
        surface.frame_end().unwrap();
        while surface.poll_event().is_some() {}
        assert!(
            Instant::now() < focus_deadline,
            "native window did not become key"
        );
        std::thread::sleep(Duration::from_millis(1));
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

    for (selector, expected) in [
        (sel!(copy:), ClipboardInput::copy()),
        (sel!(cut:), ClipboardInput::cut()),
        (sel!(paste:), ClipboardInput::paste_requested()),
        (sel!(selectAll:), ClipboardInput::select_all()),
    ] {
        // Nil target uses AppKit's normal menu target lookup, not direct bridge invocation.
        let sent: bool = unsafe {
            msg_send![&*application, sendAction: selector, to: None::<&AnyObject>, from: None::<&AnyObject>]
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

    // A queued action must not keep the Surface alive or interfere with chain retirement.
    assert!(unsafe { host.tryToPerform_with(sel!(paste:), None) });
    drop(surface);
    assert!(core::ptr::eq(
        &*unsafe { host.nextResponder() }.unwrap(),
        &*before
    ));
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
    drop(window);
    println!("macOS WGPU menu: native responder, event-loop delivery and scoped teardown passed");
}
