use super::contract::{AppSource, Event, InputFallback, InputSource};
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

mod sealed {
    pub trait InputPayload {}
}

/// Projection from platform input to a concrete input callback payload.
#[doc(hidden)]
pub trait InputPayload: Event<Route = InputFallback> + sealed::InputPayload {
    fn project(event: &InputEvent) -> Option<Self>;
}

macro_rules! typed_inputs {
    ($($kind:ident($source:ty) { $($field:ident: $ty:ty),+ $(,)? }),+ $(,)?) => {
        /// Raw input event produced by the platform layer.
        ///
        /// Pointer 0 is the first finger or mouse; other ids are additional
        /// fingers. Gestures are recognized from these pointer primitives.
        /// Suspend/resume/quit variants are requests, not state notifications.
        #[derive(Clone, Debug)]
        pub enum InputEvent {
            PointerDown { id: u8, x: Fixed, y: Fixed },
            PointerMove { id: u8, x: Fixed, y: Fixed },
            PointerUp { id: u8, x: Fixed, y: Fixed },
            PointerCancel { id: u8, x: Fixed, y: Fixed },
            $($kind { $($field: $ty),+ },)+
            AppSuspend,
            AppResume,
            Quit,
        }

        $(
            /// A concrete input payload with its platform data.
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub struct $kind {
                $(pub $field: $ty),+
            }

            impl Event for $kind {
                type Route = InputFallback;
                type Source = $source;
                type Options = ();
            }

            impl sealed::InputPayload for $kind {}

            impl InputPayload for $kind {
                fn project(event: &InputEvent) -> Option<Self> {
                    match event {
                        InputEvent::$kind { $($field),+ } => Some(Self {
                            $($field: *$field),+
                        }),
                        _ => None,
                    }
                }
            }

            impl From<$kind> for InputEvent {
                fn from(payload: $kind) -> Self {
                    Self::$kind { $($field: payload.$field),+ }
                }
            }
        )+
    };
}

typed_inputs! {
    Rotary(AppSource) { id: u8, delta: i16 },
    Wheel(AppSource) { dx: Fixed, dy: Fixed, x: Fixed, y: Fixed },
    Key(InputSource) { code: u32, pressed: bool },
    CharInput(InputSource) { ch: char },
}

impl InputEvent {
    pub(crate) fn is_user_input(&self) -> bool {
        match self {
            Self::PointerDown { .. }
            | Self::PointerMove { .. }
            | Self::PointerUp { .. }
            | Self::PointerCancel { .. }
            | Self::Rotary { .. }
            | Self::Wheel { .. }
            | Self::Key { .. }
            | Self::CharInput { .. } => true,
            Self::AppSuspend | Self::AppResume | Self::Quit => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_input_filter_does_not_include_host_lifecycle_requests() {
        for event in [
            InputEvent::PointerDown {
                id: 0,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            InputEvent::PointerMove {
                id: 0,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            InputEvent::PointerUp {
                id: 0,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            InputEvent::PointerCancel {
                id: 0,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            InputEvent::Rotary { id: 0, delta: 1 },
            InputEvent::Wheel {
                dx: Fixed::ONE,
                dy: Fixed::ONE,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            InputEvent::Key {
                code: KEY_RETURN,
                pressed: true,
            },
            InputEvent::CharInput { ch: 'a' },
        ] {
            assert!(event.is_user_input());
        }
        for event in [
            InputEvent::AppSuspend,
            InputEvent::AppResume,
            InputEvent::Quit,
        ] {
            assert!(!event.is_user_input());
        }
    }

    #[test]
    fn projections_preserve_input_fields_without_fabricating_targets() {
        fn round_trip<P>(payload: P)
        where
            P: InputPayload + Into<InputEvent> + Copy + PartialEq + core::fmt::Debug,
        {
            assert_eq!(P::project(&payload.into()), Some(payload));
        }

        round_trip(Rotary { id: 3, delta: -7 });
        round_trip(Wheel {
            dx: Fixed::from_int(-2),
            dy: Fixed::from_int(3),
            x: Fixed::from_int(4),
            y: Fixed::from_int(5),
        });
        round_trip(Key {
            code: KEY_BACKSPACE,
            pressed: true,
        });
        round_trip(Key {
            code: KEY_RETURN,
            pressed: false,
        });
        round_trip(CharInput { ch: '界' });
    }

    #[test]
    fn requests_and_pointer_primitives_do_not_project_as_callback_input() {
        for event in [
            InputEvent::PointerDown {
                id: 0,
                x: Fixed::ONE,
                y: Fixed::ZERO,
            },
            InputEvent::PointerMove {
                id: 0,
                x: Fixed::ONE,
                y: Fixed::ZERO,
            },
            InputEvent::PointerUp {
                id: 0,
                x: Fixed::ONE,
                y: Fixed::ZERO,
            },
            InputEvent::PointerCancel {
                id: 0,
                x: Fixed::ONE,
                y: Fixed::ZERO,
            },
            InputEvent::AppSuspend,
            InputEvent::AppResume,
            InputEvent::Quit,
        ] {
            assert!(Key::project(&event).is_none());
            assert!(CharInput::project(&event).is_none());
            assert!(Wheel::project(&event).is_none());
            assert!(Rotary::project(&event).is_none());
        }
    }
}
