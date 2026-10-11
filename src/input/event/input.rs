use super::contract::{AppSource, Event, EventKey, InputFallback, InputSource};
use crate::ecs::Component;
use crate::types::Fixed;

pub(crate) const WHEEL_LINE_PIXELS: Fixed = Fixed::from_int(20);

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

/// Keyboard modifiers active when a key event was produced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Projection from platform input to a concrete input callback payload.
#[doc(hidden)]
pub trait InputPayload: Event<Route = InputFallback> + sealed::InputPayload {
    fn project(event: &InputEvent) -> Option<Self>;
}

macro_rules! typed_inputs {
    ($($(#[$doc:meta])* $kind:ident($source:ty) { $($(#[$field_doc:meta])* $field:ident: $ty:ty),+ $(,)? }),+ $(,)?) => {
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
            HostFocus { focused: bool },
            HostVisibility { visible: bool },
            $($(#[$doc])* $kind { $($(#[$field_doc])* $field: $ty),+ },)+
            AppSuspend,
            AppResume,
            Quit,
        }

        $(
            /// A concrete input payload with its platform data.
            $(#[$doc])*
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub struct $kind {
                $($(#[$field_doc])* pub $field: $ty),+
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
    /// Wheel and trackpad displacement in logical pixels, including platform momentum.
    Wheel(AppSource) {
        /// Horizontal displacement; positive values increase the scroll offset.
        dx: Fixed,
        /// Vertical displacement; positive values decrease the scroll offset.
        dy: Fixed,
        /// Pointer anchor in logical surface coordinates.
        x: Fixed,
        /// Pointer anchor in logical surface coordinates.
        y: Fixed,
    },
    Key(InputSource) {
        code: u32,
        pressed: bool,
        /// Platform modifier snapshot, or `None` when unavailable.
        modifiers: Option<KeyModifiers>,
        /// Platform auto-repeat flag, or `None` when unavailable.
        repeat: Option<bool>,
    },
    CharInput(InputSource) { ch: char },
}

/// Focused keyboard selectors available on component nodes.
#[allow(non_upper_case_globals)]
pub trait InputSelectors: Component {
    const Key: EventKey<Key> = Key::EVENT;
    const CharInput: EventKey<CharInput> = CharInput::EVENT;
}

impl<T: Component> InputSelectors for T {}

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
            Self::AppSuspend
            | Self::AppResume
            | Self::Quit
            | Self::HostFocus { .. }
            | Self::HostVisibility { .. } => false,
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
                modifiers: Some(Default::default()),
                repeat: Some(false),
            },
            InputEvent::CharInput { ch: 'a' },
        ] {
            assert!(event.is_user_input());
        }
        for event in [
            InputEvent::AppSuspend,
            InputEvent::AppResume,
            InputEvent::HostFocus { focused: false },
            InputEvent::HostVisibility { visible: false },
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
            modifiers: Some(KeyModifiers {
                shift: true,
                control: true,
                alt: true,
                meta: true,
            }),
            repeat: Some(true),
        });
        round_trip(Key {
            code: KEY_RETURN,
            pressed: false,
            modifiers: None,
            repeat: None,
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
