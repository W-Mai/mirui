use crate::types::Fixed;

/// Encoder/Digital Crown press button code.
pub const KEY_ROTARY_PRESS: u32 = 0x0100;

/// Hardware button base code; concrete buttons are `KEY_HW_BUTTON_0..N`.
pub const KEY_HW_BUTTON_0: u32 = 0x0200;

// Editing keys for text input. Codes match SDL2 SDL_Keycode for the
// platform layer to map straight through; range is below the rotary
// (0x0100) and hw button (0x0200) bases to avoid collision.
pub const KEY_BACKSPACE: u32 = 0x0008;
pub const KEY_DELETE: u32 = 0x007F;
pub const KEY_LEFT: u32 = 0x0050;
pub const KEY_RIGHT: u32 = 0x0051;
pub const KEY_HOME: u32 = 0x0052;
pub const KEY_END: u32 = 0x0053;
pub const KEY_RETURN: u32 = 0x000D;
pub const KEY_ESCAPE: u32 = 0x001B;

/// Raw input event produced by the platform layer (Surface).
///
/// `id == 0` is the single-pointer / mouse path; non-zero ids are
/// reserved for multi-touch. `MultiGesture` carries the platform's
/// already-aggregated multi-finger delta; the recognizer hit-tests
/// it into `GestureEvent::Pinch` / `Rotate`.
#[derive(Clone, Debug)]
pub enum InputEvent {
    PointerDown {
        id: u8,
        x: Fixed,
        y: Fixed,
    },
    PointerMove {
        id: u8,
        x: Fixed,
        y: Fixed,
    },
    PointerUp {
        id: u8,
        x: Fixed,
        y: Fixed,
    },
    Rotary {
        id: u8,
        delta: i16,
    },
    Wheel {
        dx: Fixed,
        dy: Fixed,
        x: Fixed,
        y: Fixed,
    },
    MultiGesture {
        d_theta: Fixed,
        d_dist: Fixed,
        x: Fixed,
        y: Fixed,
        num_fingers: u8,
    },
    Key {
        code: u32,
        pressed: bool,
    },
    CharInput {
        ch: char,
    },
    Quit,
}
