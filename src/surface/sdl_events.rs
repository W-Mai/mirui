use core::mem::MaybeUninit;

use sdl2::event::Event;

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
    fn unknown_window_subtypes_do_not_reach_the_sdl2_enum() {
        assert!(supported_event_type(0x200));
        assert!(supported_event_type(0x201));
        assert!(!supported_event_type(0x202));
        assert!(!supported_event_type(0x207));
        assert!(!supported_event_type(0x2ff));
        assert!(supported_event_type(0x300));
    }
}
