use super::contract::{AppSource, Event, EventKey, InputFallback, InputSource};
use crate::ecs::Component;
use crate::types::Fixed;
use alloc::borrow::Cow;

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

/// Platform-reported pointer device category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerKind {
    Mouse,
    Touch,
    Pen,
    #[default]
    Unknown,
}

/// A typed platform input payload.
#[doc(hidden)]
pub trait InputPayload: Event<Route = InputFallback> + sealed::InputPayload {}

/// One complete text fragment delivered by an input source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextCommitted {
    text: Cow<'static, str>,
}

impl TextCommitted {
    /// Move owned text or borrow a static string without copying it.
    pub fn new(text: impl Into<Cow<'static, str>>) -> Self {
        Self { text: text.into() }
    }

    /// Borrow the complete UTF-8 fragment for this callback.
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Event for TextCommitted {
    type Route = InputFallback;
    type Source = InputSource;
    type Options = ();
}

impl sealed::InputPayload for TextCommitted {}
impl InputPayload for TextCommitted {}

impl From<TextCommitted> for InputEvent {
    fn from(text: TextCommitted) -> Self {
        Self::TextCommitted(text)
    }
}

macro_rules! typed_inputs {
    ($($(#[$doc:meta])* $kind:ident($source:ty) { $($(#[$field_doc:meta])* $field:ident: $ty:ty),+ $(,)? }),+ $(,)?) => {
        /// Raw input event produced by the platform layer.
        ///
        /// Active pointers are identified by their device category and slot id.
        /// Gestures are recognized from these pointer primitives.
        /// Suspend/resume/quit variants are requests, not state notifications.
        #[derive(Clone, Debug)]
        pub enum InputEvent {
            PointerDown { kind: PointerKind, id: u8, x: Fixed, y: Fixed },
            PointerMove { kind: PointerKind, id: u8, x: Fixed, y: Fixed },
            PointerUp { kind: PointerKind, id: u8, x: Fixed, y: Fixed },
            PointerCancel { kind: PointerKind, id: u8, x: Fixed, y: Fixed },
            PointerEnter { kind: PointerKind, id: u8 },
            PointerLeave { kind: PointerKind, id: u8 },
            HostFocus { focused: bool },
            HostVisibility { visible: bool },
            TextCommitted(TextCommitted),
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

            impl InputPayload for $kind {}

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
}

/// Focused keyboard selectors available on component nodes.
#[allow(non_upper_case_globals)]
pub trait InputSelectors: Component {
    const CompositionStarted: EventKey<crate::ui::text_input::CompositionStarted> =
        crate::ui::text_input::CompositionStarted::EVENT;
    const CompositionUpdated: EventKey<crate::ui::text_input::CompositionUpdated> =
        crate::ui::text_input::CompositionUpdated::EVENT;
    const CompositionEnded: EventKey<crate::ui::text_input::CompositionEnded> =
        crate::ui::text_input::CompositionEnded::EVENT;
    const Key: EventKey<Key> = Key::EVENT;
    const TextCommitted: EventKey<TextCommitted> = TextCommitted::EVENT;
}

impl<T: Component> InputSelectors for T {}

impl InputEvent {
    pub(crate) fn pointer_identity(&self) -> Option<(PointerKind, u8)> {
        match self {
            Self::PointerDown { kind, id, .. }
            | Self::PointerMove { kind, id, .. }
            | Self::PointerUp { kind, id, .. }
            | Self::PointerCancel { kind, id, .. } => Some((*kind, *id)),
            Self::PointerEnter { kind, id } | Self::PointerLeave { kind, id } => Some((*kind, *id)),
            _ => None,
        }
    }

    pub(crate) fn is_user_input(&self) -> bool {
        match self {
            Self::PointerDown { .. }
            | Self::PointerMove { .. }
            | Self::PointerUp { .. }
            | Self::PointerCancel { .. }
            | Self::PointerEnter { .. }
            | Self::PointerLeave { .. }
            | Self::Rotary { .. }
            | Self::Wheel { .. }
            | Self::Key { .. }
            | Self::TextCommitted(_) => true,
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
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            InputEvent::PointerMove {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            InputEvent::PointerUp {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            InputEvent::PointerCancel {
                kind: crate::input::event::input::PointerKind::Mouse,
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
            InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new("a")),
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
    fn conversion_preserves_keyboard_metadata() {
        let key = Key {
            code: KEY_BACKSPACE,
            pressed: true,
            modifiers: Some(KeyModifiers {
                shift: true,
                control: true,
                alt: true,
                meta: true,
            }),
            repeat: Some(true),
        };
        let InputEvent::Key {
            code,
            pressed,
            modifiers,
            repeat,
        } = key.into()
        else {
            panic!("key input")
        };
        assert_eq!(
            Key {
                code,
                pressed,
                modifiers,
                repeat
            },
            key
        );
    }

    #[test]
    fn text_keeps_owned_and_static_storage_without_copying() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<InputEvent>();
        let owned = alloc::string::String::from("A中🙂");
        let pointer = owned.as_ptr();
        let event: InputEvent = TextCommitted::new(owned).into();
        let InputEvent::TextCommitted(text) = event else {
            panic!("text input")
        };
        assert_eq!(text.text(), "A中🙂");
        assert_eq!(text.text().as_ptr(), pointer);
        let allocations = crate::test_support::tracked_allocations(|| {
            let text = TextCommitted::new("静态🙂");
            assert_eq!(text.text(), "静态🙂");
        });
        assert_eq!(allocations, 0);
    }
}
