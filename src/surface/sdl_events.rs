use core::mem::MaybeUninit;

use sdl2::event::Event;

pub(crate) fn wheel_input(
    dx: f32,
    dy: f32,
    x: i32,
    y: i32,
) -> crate::input::event::input::InputEvent {
    use crate::types::Fixed;
    crate::input::event::input::InputEvent::Wheel {
        dx: Fixed::from_f32(dx),
        dy: Fixed::from_f32(dy),
        x: Fixed::from(x),
        y: Fixed::from(y),
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

    #[test]
    fn wheel_preserves_fractional_motion_and_logical_anchor() {
        use crate::input::event::input::InputEvent;
        use crate::types::Fixed;
        let InputEvent::Wheel { dx, dy, x, y } = wheel_input(0.125, -0.375, 42, 71) else {
            panic!("wheel")
        };
        assert_eq!(dx, Fixed::from_ratio(1, 8));
        assert_eq!(dy, Fixed::from_ratio(-3, 8));
        assert_eq!(x, Fixed::from_int(42));
        assert_eq!(y, Fixed::from_int(71));
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
