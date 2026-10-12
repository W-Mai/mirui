use super::InputEvent;
use crate::ui::text_input::TextInputState;
use core::ops::Range;
use sdl2::{EventSubsystem, VideoSubsystem, event::EventType, keyboard::TextInputUtil};

pub(crate) struct NativeTextInput {
    state: Option<TextInputState>,
    input: TextInputUtil,
    events: EventSubsystem,
    utf16: bool,
    pending_clear: Option<crate::ui::text_input::TextInputSession>,
}

impl NativeTextInput {
    pub fn new(video: &VideoSubsystem, events: EventSubsystem) -> Self {
        let input = video.text_input();
        input.stop();
        events.flush_event(EventType::TextEditing);
        events.flush_event(EventType::TextInput);
        // SDL2 Cocoa/Windows report UTF-16 units; SDL3 reports scalar indices.
        let version = sdl2::version::version();
        let sdl3 = (version.major, version.minor, version.patch) >= (2, 32, 50);
        let utf16 = cfg!(any(target_os = "macos", target_os = "windows")) && !sdl3;
        Self {
            state: None,
            input,
            events,
            utf16,
            pending_clear: None,
        }
    }

    pub fn set(&mut self, next: Option<TextInputState>) {
        if self.state == next {
            return;
        }
        if self.state.map(|state| state.session) != next.map(|state| state.session) {
            self.pending_clear = None;
            if self.state.is_some() {
                // The video subsystem is retained and this runs on SDL's event thread.
                unsafe { sdl2::sys::SDL_ClearComposition() };
            }
            self.input.stop();
            self.events.flush_event(EventType::TextEditing);
            self.events.flush_event(EventType::TextInput);
            if next.is_some() {
                self.input.start();
            }
        }
        if let Some(rect) = next.and_then(|state| state.caret) {
            self.input.set_rect(sdl2::rect::Rect::new(
                rect.x.to_int(),
                rect.y.to_int(),
                rect.w.to_int().max(1) as u32,
                rect.h.to_int().max(1) as u32,
            ));
        }
        self.state = next;
    }

    pub fn commit(&mut self, text: String) -> Option<InputEvent> {
        let session = self.state?.session;
        self.pending_clear = None;
        (!text.is_empty()).then(|| session.commit(text))
    }

    pub fn preedit(&mut self, text: String, start: i32, length: i32) -> Option<InputEvent> {
        let session = self.state?.session;
        self.pending_clear = text.is_empty().then_some(session);
        let selection = selection(&text, start, length, self.utf16);
        Some(session.preedit(text, selection))
    }

    pub fn idle(&mut self) -> Option<InputEvent> {
        // SDL sends an empty preview before commit as well as on cancellation.
        self.pending_clear.take().map(|session| session.cancel())
    }
}

fn selection(text: &str, start: i32, length: i32, utf16: bool) -> Option<Range<usize>> {
    let start = usize::try_from(start).ok()?;
    let end = start.checked_add(usize::try_from(length).ok()?)?;
    crate::surface::text_input_range::utf8_selection(text, start..end, utf16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_ranges_convert_to_utf8_without_splitting_characters() {
        assert_eq!(selection("a🙂你z", 1, 2, true), Some(1..5));
        assert_eq!(selection("a🙂你z", 1, 2, false), Some(1..8));
        assert_eq!(selection("a🙂你z", 2, 0, true), None);
        assert_eq!(selection("a🙂你z", 5, 0, true), Some(9..9));
        assert_eq!(selection("a🙂你z", 4, 0, false), Some(9..9));
        assert_eq!(selection("", 0, 0, false), Some(0..0));
        assert_eq!(selection("ni", -1, 0, false), None);
        assert_eq!(selection("ni", 0, 3, false), None);
    }
}
