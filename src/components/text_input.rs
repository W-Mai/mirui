use crate::types::Color;

pub const TEXT_INPUT_CAP: usize = 32;

/// Single-line ASCII text input with a fixed-capacity buffer.
///
/// `buffer[..len]` are the live characters; `cursor` is the insertion
/// point in `0..=len`. Non-ASCII / non-printable input is rejected by
/// the key handler (the 8×8 bitmap font only covers ASCII 32-126).
///
/// `focused` mirrors `FocusState` for fast read in the renderer; it's
/// updated by the gesture handler on Tap.
pub struct TextInput {
    pub buffer: [u8; TEXT_INPUT_CAP],
    pub len: u8,
    pub cursor: u8,
    pub focused: bool,
    pub text_color: Color,
    pub placeholder_color: Color,
    pub cursor_color: Color,
    pub focus_border_color: Color,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            buffer: [0u8; TEXT_INPUT_CAP],
            len: 0,
            cursor: 0,
            focused: false,
            text_color: Color::rgb(220, 220, 230),
            placeholder_color: Color::rgb(120, 120, 140),
            cursor_color: Color::rgb(220, 220, 230),
            focus_border_color: Color::rgb(88, 166, 255),
        }
    }

    pub fn as_str(&self) -> &str {
        // Buffer only ever holds ASCII (filtered by the key handler) so
        // utf8 validation is a runtime no-op; we still go through the
        // checked API to stay sound.
        core::str::from_utf8(&self.buffer[..self.len as usize]).unwrap_or("")
    }

    pub fn insert(&mut self, ch: u8) -> bool {
        if !(32..=126).contains(&ch) {
            return false;
        }
        if self.len as usize >= TEXT_INPUT_CAP {
            return false;
        }
        let pos = self.cursor as usize;
        let end = self.len as usize;
        if pos > end {
            return false;
        }
        // Shift right.
        let mut i = end;
        while i > pos {
            self.buffer[i] = self.buffer[i - 1];
            i -= 1;
        }
        self.buffer[pos] = ch;
        self.len += 1;
        self.cursor += 1;
        true
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let pos = self.cursor as usize - 1;
        let end = self.len as usize;
        let mut i = pos;
        while i + 1 < end {
            self.buffer[i] = self.buffer[i + 1];
            i += 1;
        }
        self.len -= 1;
        self.cursor -= 1;
        true
    }

    pub fn delete_forward(&mut self) -> bool {
        if (self.cursor as usize) >= (self.len as usize) {
            return false;
        }
        let pos = self.cursor as usize;
        let end = self.len as usize;
        let mut i = pos;
        while i + 1 < end {
            self.buffer[i] = self.buffer[i + 1];
            i += 1;
        }
        self.len -= 1;
        true
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if (self.cursor as usize) < (self.len as usize) {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.len;
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Optional placeholder text rendered when the buffer is empty. Stored
/// as a separate component so the common case (no placeholder) doesn't
/// pay 32 extra bytes inside `TextInput`.
pub struct Placeholder(pub &'static str);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_backspace() {
        let mut ti = TextInput::new();
        for ch in b"hello".iter() {
            assert!(ti.insert(*ch));
        }
        assert_eq!(ti.as_str(), "hello");
        assert_eq!(ti.cursor, 5);
        assert!(ti.backspace());
        assert!(ti.backspace());
        assert_eq!(ti.as_str(), "hel");
        assert_eq!(ti.cursor, 3);
    }

    #[test]
    fn arrow_keys_navigate() {
        let mut ti = TextInput::new();
        for ch in b"hello".iter() {
            ti.insert(*ch);
        }
        ti.move_left();
        ti.move_left();
        assert_eq!(ti.cursor, 3);
        ti.insert(b'X');
        assert_eq!(ti.as_str(), "helXlo");
        assert_eq!(ti.cursor, 4);
    }

    #[test]
    fn home_end() {
        let mut ti = TextInput::new();
        for ch in b"hi".iter() {
            ti.insert(*ch);
        }
        ti.move_home();
        assert_eq!(ti.cursor, 0);
        ti.move_end();
        assert_eq!(ti.cursor, 2);
    }

    #[test]
    fn delete_forward_removes_at_cursor() {
        let mut ti = TextInput::new();
        for ch in b"abc".iter() {
            ti.insert(*ch);
        }
        ti.move_home();
        assert!(ti.delete_forward());
        assert_eq!(ti.as_str(), "bc");
        assert_eq!(ti.cursor, 0);
    }

    #[test]
    fn rejects_non_printable_ascii() {
        let mut ti = TextInput::new();
        assert!(!ti.insert(0x07)); // bell
        assert!(!ti.insert(0xFF));
        assert_eq!(ti.len, 0);
    }

    #[test]
    fn full_buffer_rejects() {
        let mut ti = TextInput::new();
        for _ in 0..TEXT_INPUT_CAP {
            assert!(ti.insert(b'a'));
        }
        assert!(!ti.insert(b'b'));
        assert_eq!(ti.len as usize, TEXT_INPUT_CAP);
    }
}
