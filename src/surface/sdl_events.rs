use core::mem::MaybeUninit;

use crate::surface::InputEvent;
use sdl2::event::Event;

mod text_input;
pub(crate) use text_input::NativeTextInput;

pub(crate) fn write_clipboard(
    video: &sdl2::VideoSubsystem,
    request: crate::ui::clipboard::ClipboardWrite,
) -> crate::ui::clipboard::ClipboardDispatch {
    use crate::ui::clipboard::{ClipboardDispatch, ClipboardError};
    let result = request.data().plain_text().and_then(|text| {
        if text.as_bytes().contains(&0) {
            return Err(ClipboardError::InvalidData);
        }
        video
            .clipboard()
            .set_clipboard_text(text)
            .map_err(|_| ClipboardError::Platform)
    });
    ClipboardDispatch::Complete(request.complete(result))
}

pub(crate) fn keyboard_input(event: &Event) -> Option<InputEvent> {
    use crate::input::event::input::*;
    use sdl2::keyboard::{Keycode, Mod};

    let (keycode, pressed, keymod, repeat) = match event {
        Event::KeyDown {
            keycode: Some(keycode),
            keymod,
            repeat,
            ..
        } => (*keycode, true, *keymod, *repeat),
        Event::KeyUp {
            keycode: Some(keycode),
            keymod,
            repeat,
            ..
        } => (*keycode, false, *keymod, *repeat),
        _ => return None,
    };
    let code = match keycode {
        Keycode::Backspace => KEY_BACKSPACE,
        Keycode::Delete => KEY_DELETE,
        Keycode::Left => KEY_LEFT,
        Keycode::Right => KEY_RIGHT,
        Keycode::Home => KEY_HOME,
        Keycode::End => KEY_END,
        Keycode::Return => KEY_RETURN,
        Keycode::Escape if pressed => return Some(InputEvent::Quit),
        _ => return None,
    };
    Some(InputEvent::Key {
        code,
        pressed,
        modifiers: Some(KeyModifiers {
            shift: keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD),
            control: keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD),
            alt: keymod.intersects(Mod::LALTMOD | Mod::RALTMOD),
            meta: keymod.intersects(Mod::LGUIMOD | Mod::RGUIMOD),
        }),
        repeat: Some(repeat),
    })
}

pub(crate) fn init() -> Result<sdl2::Sdl, String> {
    #[cfg(target_os = "macos")]
    sdl2::hint::set("SDL_MAC_SCROLL_MOMENTUM", "1");
    sdl2::init()
}

pub(crate) fn wheel_input(
    dx: f32,
    dy: f32,
    x: i32,
    y: i32,
) -> crate::input::event::input::InputEvent {
    use crate::types::Fixed;
    let step = crate::input::event::input::WHEEL_LINE_PIXELS.to_f32();
    crate::input::event::input::InputEvent::Wheel {
        dx: Fixed::from_f32(dx * step),
        dy: Fixed::from_f32(dy * step),
        x: Fixed::from(x),
        y: Fixed::from(y),
    }
}

pub(crate) fn host_window_input(
    event: sdl2::event::WindowEvent,
) -> Option<crate::surface::InputEvent> {
    use crate::surface::InputEvent;
    use sdl2::event::WindowEvent;
    match event {
        WindowEvent::FocusLost => Some(InputEvent::HostFocus { focused: false }),
        WindowEvent::FocusGained => Some(InputEvent::HostFocus { focused: true }),
        WindowEvent::Hidden | WindowEvent::Minimized => {
            Some(InputEvent::HostVisibility { visible: false })
        }
        WindowEvent::Shown | WindowEvent::Restored => {
            Some(InputEvent::HostVisibility { visible: true })
        }
        _ => None,
    }
}

pub(crate) fn pointer_window_input(event: sdl2::event::WindowEvent) -> Option<InputEvent> {
    use crate::surface::PointerKind;
    use sdl2::event::WindowEvent;
    match event {
        WindowEvent::Enter => Some(InputEvent::PointerEnter {
            kind: PointerKind::Mouse,
            id: 0,
        }),
        WindowEvent::Leave => Some(InputEvent::PointerLeave {
            kind: PointerKind::Mouse,
            id: 0,
        }),
        _ => None,
    }
}

fn supported_event_type(kind: u32) -> bool {
    // SDL2 defines only Window (0x200) and SysWM (0x201) in this range.
    !(0x202..0x300).contains(&kind)
}

pub(crate) fn poll() -> Option<Event> {
    loop {
        let mut raw = MaybeUninit::uninit();
        if unsafe { sdl2::sys::SDL_PollEvent(raw.as_mut_ptr()) } != 1 {
            return None;
        }
        let raw = unsafe { raw.assume_init() };
        let kind = unsafe { raw.type_ };
        if supported_event_type(kind) {
            return Some(Event::from_ll(raw));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::input::*;
    use sdl2::keyboard::{Keycode, Mod};

    fn key_event(keycode: Keycode, pressed: bool) -> Event {
        if pressed {
            Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(keycode),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat: false,
            }
        } else {
            Event::KeyUp {
                timestamp: 0,
                window_id: 0,
                keycode: Some(keycode),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat: false,
            }
        }
    }

    #[test]
    fn keyboard_translation_preserves_pressed_and_released_states() {
        for (key, expected) in [
            (Keycode::Backspace, KEY_BACKSPACE),
            (Keycode::Delete, KEY_DELETE),
            (Keycode::Left, KEY_LEFT),
            (Keycode::Right, KEY_RIGHT),
            (Keycode::Home, KEY_HOME),
            (Keycode::End, KEY_END),
            (Keycode::Return, KEY_RETURN),
        ] {
            for pressed in [true, false] {
                assert!(matches!(keyboard_input(&key_event(key, pressed)),
                    Some(InputEvent::Key { code, pressed: actual, .. })
                    if code == expected && actual == pressed));
            }
        }
    }

    #[test]
    fn window_boundaries_do_not_synthesize_motion_or_release() {
        use sdl2::event::WindowEvent;
        assert!(matches!(
            pointer_window_input(WindowEvent::Enter),
            Some(InputEvent::PointerEnter {
                kind: PointerKind::Mouse,
                id: 0
            })
        ));
        assert!(matches!(
            pointer_window_input(WindowEvent::Leave),
            Some(InputEvent::PointerLeave {
                kind: PointerKind::Mouse,
                id: 0
            })
        ));
        assert!(pointer_window_input(WindowEvent::FocusLost).is_none());
    }

    #[test]
    fn keyboard_translation_does_not_duplicate_quit_or_text() {
        assert!(matches!(
            keyboard_input(&key_event(Keycode::Escape, true)),
            Some(InputEvent::Quit)
        ));
        assert!(keyboard_input(&key_event(Keycode::Escape, false)).is_none());
        for pressed in [true, false] {
            assert!(keyboard_input(&key_event(Keycode::A, pressed)).is_none());
        }
    }

    #[test]
    fn keyboard_metadata_preserves_each_modifier_and_repeat() {
        for (mods, expected) in [
            (
                Mod::LSHIFTMOD,
                KeyModifiers {
                    shift: true,
                    ..Default::default()
                },
            ),
            (
                Mod::RSHIFTMOD,
                KeyModifiers {
                    shift: true,
                    ..Default::default()
                },
            ),
            (
                Mod::LCTRLMOD | Mod::RCTRLMOD,
                KeyModifiers {
                    control: true,
                    ..Default::default()
                },
            ),
            (
                Mod::LALTMOD | Mod::RALTMOD,
                KeyModifiers {
                    alt: true,
                    ..Default::default()
                },
            ),
            (
                Mod::LGUIMOD | Mod::RGUIMOD,
                KeyModifiers {
                    meta: true,
                    ..Default::default()
                },
            ),
            (Mod::NOMOD, KeyModifiers::default()),
        ] {
            for pressed in [true, false] {
                let mut event = key_event(Keycode::Left, pressed);
                match &mut event {
                    Event::KeyDown { keymod, repeat, .. } | Event::KeyUp { keymod, repeat, .. } => {
                        *keymod = mods;
                        *repeat = pressed;
                    }
                    _ => unreachable!(),
                }
                let Some(InputEvent::Key {
                    modifiers, repeat, ..
                }) = keyboard_input(&event)
                else {
                    panic!("key")
                };
                assert_eq!(modifiers, Some(expected));
                assert_eq!(repeat, Some(pressed));
            }
        }
    }

    #[test]
    fn focus_and_visibility_are_not_suspend_requests() {
        use crate::surface::InputEvent;
        use sdl2::event::WindowEvent;
        assert!(matches!(
            host_window_input(WindowEvent::FocusLost),
            Some(InputEvent::HostFocus { focused: false })
        ));
        assert!(matches!(
            host_window_input(WindowEvent::FocusGained),
            Some(InputEvent::HostFocus { focused: true })
        ));
        for event in [WindowEvent::Hidden, WindowEvent::Minimized] {
            assert!(matches!(
                host_window_input(event),
                Some(InputEvent::HostVisibility { visible: false })
            ));
        }
        for event in [WindowEvent::Shown, WindowEvent::Restored] {
            assert!(matches!(
                host_window_input(event),
                Some(InputEvent::HostVisibility { visible: true })
            ));
        }
        assert!(host_window_input(WindowEvent::Leave).is_none());
    }

    #[test]
    fn wheel_preserves_fractional_motion_and_logical_anchor() {
        use crate::input::event::input::InputEvent;
        use crate::types::Fixed;
        let InputEvent::Wheel { dx, dy, x, y } = wheel_input(0.125, -0.375, 42, 71) else {
            panic!("wheel")
        };
        assert_eq!(dx, Fixed::from_ratio(5, 2));
        assert_eq!(dy, Fixed::from_ratio(-15, 2));
        assert_eq!(x, Fixed::from_int(42));
        assert_eq!(y, Fixed::from_int(71));
        let InputEvent::Wheel { dy, .. } = wheel_input(0.0, 0.001, 42, 71) else {
            panic!("wheel")
        };
        assert_eq!(dy, Fixed::from_f32(0.02));
    }

    #[test]
    fn unknown_window_subtypes_do_not_reach_the_sdl2_enum() {
        assert!(supported_event_type(0x200));
        assert!(supported_event_type(0x201));
        assert!(!supported_event_type(0x202));
        assert!(!supported_event_type(0x207));
        assert!(!supported_event_type(0x2ff));
        assert!(supported_event_type(0x300));
    }
}
