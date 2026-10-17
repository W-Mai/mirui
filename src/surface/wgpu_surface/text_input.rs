use super::{Ime, InputEvent, Window};
use crate::ui::text_input::{TextInputSession, TextInputState};

#[derive(Default)]
pub(super) struct NativeTextInput {
    requested: Option<TextInputState>,
    source: Option<TextInputSession>,
    composing: bool,
    pending_clear: Option<TextInputSession>,
}

impl NativeTextInput {
    pub fn set(&mut self, next: Option<TextInputState>, window: Option<&Window>) {
        if self.requested == next {
            return;
        }
        let changed = self.requested.map(|state| state.session) != next.map(|state| state.session);
        if changed {
            self.pending_clear = None;
            self.composing = false;
            if let Some(window) = window {
                if self.requested.is_some() {
                    window.set_ime_allowed(false);
                }
                if next.is_some() {
                    window.set_ime_allowed(true);
                }
            }
        }
        self.requested = next;
        if let Some(window) = window {
            self.update_caret(window);
        }
    }

    fn update_caret(&self, window: &Window) {
        if let Some(caret) = self.requested.and_then(|state| state.caret) {
            window.set_ime_cursor_area(
                winit::dpi::LogicalPosition::new(caret.x.to_f32(), caret.y.to_f32()),
                winit::dpi::LogicalSize::new(caret.w.to_f32().max(1.0), caret.h.to_f32().max(1.0)),
            );
        }
    }

    pub fn composing(&self) -> bool {
        self.composing
    }

    pub fn keyboard_text(&mut self, text: &str) -> Option<InputEvent> {
        if self.composing || text.is_empty() {
            return None;
        }
        self.pending_clear = None;
        let text = alloc::string::String::from(text);
        Some(match self.requested {
            Some(state) => state.session.commit(text),
            None => crate::input::event::input::TextCommitted::new(text).into(),
        })
    }

    pub fn ime(&mut self, event: Ime) -> Option<InputEvent> {
        match event {
            Ime::Enabled => {
                self.source = self.requested.map(|state| state.session);
                self.composing = false;
                self.pending_clear = None;
                None
            }
            Ime::Preedit(text, selection) => {
                let source = self.source?;
                self.composing = !text.is_empty();
                self.pending_clear = text.is_empty().then_some(source);
                Some(source.preedit(text, selection.map(|(start, end)| start..end)))
            }
            Ime::Commit(text) => {
                let source = self.source?;
                self.composing = false;
                self.pending_clear = None;
                Some(source.commit(text))
            }
            Ime::Disabled => {
                self.composing = false;
                self.pending_clear = None;
                self.source.take().map(|source| source.cancel())
            }
        }
    }

    pub fn finish_batch(&mut self) -> Option<InputEvent> {
        let source = self.pending_clear.take()?;
        self.composing = false;
        Some(source.cancel())
    }
}
