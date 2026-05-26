//! Headless SDL event queue regression tests.

#![cfg(feature = "sdl")]

use mirui::surface::sdl::SdlSurface;
use mirui::surface::{InputEvent, Surface};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Mod, Scancode};
use sdl2::mouse::MouseButton;

#[test]
fn poll_event_recovers_burst_and_window_leave() {
    // SAFETY: must run before SDL_Init.
    unsafe {
        std::env::set_var("SDL_VIDEODRIVER", "dummy");
    }
    // SDL init is process-global; keep all assertions in one test
    // so the integration binary never tries to init SDL twice.
    let mut surface = SdlSurface::new("poll", 100, 100);
    let sdl = sdl2::init().expect("sdl init");
    let ev = sdl.event().expect("event subsystem");

    ev.push_event(Event::MouseButtonDown {
        timestamp: 0,
        window_id: 0,
        which: 0,
        mouse_btn: MouseButton::Left,
        clicks: 1,
        x: 10,
        y: 20,
    })
    .expect("push MouseButtonDown");
    ev.push_event(Event::MouseMotion {
        timestamp: 0,
        window_id: 0,
        which: 0,
        mousestate: sdl2::mouse::MouseState::from_sdl_state(0),
        x: 12,
        y: 22,
        xrel: 2,
        yrel: 2,
    })
    .expect("push MouseMotion");
    ev.push_event(Event::MouseButtonUp {
        timestamp: 0,
        window_id: 0,
        which: 0,
        mouse_btn: MouseButton::Left,
        clicks: 1,
        x: 14,
        y: 24,
    })
    .expect("push MouseButtonUp");
    ev.push_event(Event::KeyDown {
        timestamp: 0,
        window_id: 0,
        keycode: Some(sdl2::keyboard::Keycode::Return),
        scancode: Some(Scancode::Return),
        keymod: Mod::NOMOD,
        repeat: false,
    })
    .expect("push KeyDown");

    let mut got_burst: Vec<&'static str> = Vec::new();
    while let Some(e) = surface.poll_event() {
        got_burst.push(match e {
            InputEvent::PointerDown { .. } => "Down",
            InputEvent::PointerMove { .. } => "Move",
            InputEvent::PointerUp { .. } => "Up",
            InputEvent::Key { .. } => "Key",
            InputEvent::Quit => "Quit",
            _ => "Other",
        });
    }
    // SDL may inject platform / window events; require the pushed
    // events as an ordered subsequence.
    let want = ["Down", "Move", "Up", "Key"];
    let mut wi = 0;
    for &e in &got_burst {
        if wi < want.len() && e == want[wi] {
            wi += 1;
        }
    }
    assert_eq!(
        wi,
        want.len(),
        "burst: expected subseq {want:?} in {got_burst:?}, only matched first {wi}"
    );

    ev.push_event(Event::Window {
        timestamp: 0,
        window_id: 0,
        win_event: WindowEvent::Leave,
    })
    .expect("push Window Leave");

    let mut leave_move: Option<(u8, i32, i32)> = None;
    while let Some(e) = surface.poll_event() {
        if let InputEvent::PointerMove { id, x, y } = e {
            if x.to_int() < -1000 {
                leave_move = Some((id, x.to_int(), y.to_int()));
            }
        }
    }
    let (id, x, y) =
        leave_move.expect("WindowEvent::Leave should produce a far-off-screen PointerMove");
    assert_eq!(id, 0, "leave PointerMove id");
    assert_eq!(x, i16::MIN as i32, "leave PointerMove x");
    assert_eq!(y, i16::MIN as i32, "leave PointerMove y");
}
