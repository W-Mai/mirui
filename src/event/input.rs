use crate::types::Fixed;

/// Encoder/Digital Crown press button code.
pub const KEY_ROTARY_PRESS: u32 = 0x0100;

/// Hardware button base code; concrete buttons are `KEY_HW_BUTTON_0..N`.
pub const KEY_HW_BUTTON_0: u32 = 0x0200;

/// Raw input event produced by the platform layer (Surface).
///
/// Pointer events carry an `id: u8` so multi-touch can be wired in
/// later; `id == 0` is the single-pointer / mouse path. Rotary covers
/// the encoder / Digital Crown family. Hardware buttons share the
/// `Key` variant via well-known codes.
#[derive(Clone, Debug)]
pub enum InputEvent {
    PointerDown { id: u8, x: Fixed, y: Fixed },
    PointerMove { id: u8, x: Fixed, y: Fixed },
    PointerUp { id: u8, x: Fixed, y: Fixed },
    Rotary { id: u8, delta: i16 },
    Key { code: u32, pressed: bool },
    CharInput { ch: char },
    Quit,
}
