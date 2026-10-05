use core::num::NonZeroU8;

use crate::ecs::{Component, Entity};
use crate::input::event::contract::{EntityBubble, EntitySource, Event, EventKey, EventOptions};
use crate::types::{Fixed, Fixed64};

mod sealed {
    pub trait GesturePayload {}
}

/// A gesture selector matching any tap or one validated multiplicity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapOptions {
    count: Option<NonZeroU8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapCountError {
    pub count: u32,
}

impl core::fmt::Display for TapCountError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "tap count must be between 1 and 255, got {}", self.count)
    }
}

impl core::error::Error for TapCountError {}

impl TapOptions {
    pub fn new(count: u32) -> Result<Self, TapCountError> {
        let validated = u8::try_from(count).ok().and_then(NonZeroU8::new);
        validated
            .map(|count| Self { count: Some(count) })
            .ok_or(TapCountError { count })
    }

    pub const fn count(self) -> Option<u8> {
        match self.count {
            Some(count) => Some(count.get()),
            None => None,
        }
    }

    pub fn matches(self, observed: u8) -> bool {
        observed != 0 && self.count().is_none_or(|count| count == observed)
    }
}

impl EventOptions for TapOptions {
    type Arguments = (u32,);
    type Error = TapCountError;

    const DEFAULT: Self = Self { count: None };

    fn from_arguments((count,): Self::Arguments) -> Result<Self, Self::Error> {
        Self::new(count)
    }
}

/// Projection from the recognizer's carrier to one concrete callback payload.
#[doc(hidden)]
pub trait GesturePayload:
    Event<Route = EntityBubble, Source = EntitySource> + sealed::GesturePayload
{
    fn project(event: &GestureEvent) -> Option<Self>;
    fn matches_options(options: Self::Options, tap_count: u8) -> bool;
}

trait GestureOptions {
    fn matches(self, tap_count: u8) -> bool;
}

impl GestureOptions for TapOptions {
    fn matches(self, tap_count: u8) -> bool {
        self.matches(tap_count)
    }
}

impl GestureOptions for () {
    fn matches(self, _: u8) -> bool {
        true
    }
}

macro_rules! gestures {
    ($($kind:ident($options:ty) { $($field:ident: $ty:ty),+ $(,)? }),+ $(,)?) => {
        /// Gesture carrier emitted by the recognizer.
        #[derive(Clone, Debug)]
        pub enum GestureEvent {
            $($kind { $($field: $ty),+ }),+
        }

        impl GestureEvent {
            pub fn target(&self) -> Entity {
                match self {
                    $(Self::$kind { target, .. } => *target),+
                }
            }
        }

        $(
            /// A concrete gesture payload preserving the recognizer's target.
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub struct $kind {
                $(pub $field: $ty),+
            }

            impl Event for $kind {
                type Route = EntityBubble;
                type Source = EntitySource;
                type Options = $options;
            }

            impl sealed::GesturePayload for $kind {}

            impl GesturePayload for $kind {
                fn matches_options(options: Self::Options, tap_count: u8) -> bool {
                    GestureOptions::matches(options, tap_count)
                }

                fn project(event: &GestureEvent) -> Option<Self> {
                    match event {
                        GestureEvent::$kind { $($field),+ } => Some(Self {
                            $($field: *$field),+
                        }),
                        _ => None,
                    }
                }
            }

            impl From<$kind> for GestureEvent {
                fn from(payload: $kind) -> Self {
                    Self::$kind { $($field: payload.$field),+ }
                }
            }
        )+

        /// Default gesture selectors for component nodes.
        #[allow(non_upper_case_globals)]
        pub trait GestureSelectors: Component {
            $(const $kind: EventKey<$kind> = $kind::EVENT;)+
        }

        impl<W: Component> GestureSelectors for W {}
    };
}

gestures! {
    Tap(TapOptions) { x: Fixed, y: Fixed, target: Entity },
    LongPress(()) { x: Fixed, y: Fixed, target: Entity },
    DragStart(()) { x: Fixed, y: Fixed, target: Entity },
    DragMove(()) { x: Fixed, y: Fixed, dx: Fixed, dy: Fixed, target: Entity },
    DragEnd(()) { x: Fixed, y: Fixed, vx: Fixed, vy: Fixed, target: Entity },
    DragCancel(()) { x: Fixed, y: Fixed, target: Entity },
    Pinch(()) { x: Fixed, y: Fixed, scale_delta: Fixed64, target: Entity },
    Rotate(()) { x: Fixed, y: Fixed, angle: Fixed, target: Entity },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projections_preserve_every_gesture_field_and_target_generation() {
        let target = Entity {
            id: 7,
            generation: 3,
        };
        let x = Fixed::from_int(1);
        let y = Fixed::from_int(2);

        fn round_trip<P>(payload: P, target: Entity)
        where
            P: GesturePayload + Into<GestureEvent> + Copy + PartialEq + core::fmt::Debug,
        {
            let carrier = payload.into();
            assert_eq!(carrier.target(), target);
            assert_eq!(P::project(&carrier), Some(payload));
        }

        round_trip(Tap { x, y, target }, target);
        round_trip(LongPress { x, y, target }, target);
        round_trip(DragStart { x, y, target }, target);
        round_trip(
            DragMove {
                x,
                y,
                dx: -x,
                dy: -y,
                target,
            },
            target,
        );
        round_trip(
            DragEnd {
                x,
                y,
                vx: -y,
                vy: -x,
                target,
            },
            target,
        );
        round_trip(DragCancel { x, y, target }, target);
        round_trip(
            Pinch {
                x,
                y,
                scale_delta: Fixed64::from_int(3),
                target,
            },
            target,
        );
        round_trip(
            Rotate {
                x,
                y,
                angle: -y,
                target,
            },
            target,
        );

        assert!(Rotate::project(&GestureEvent::from(Tap { x, y, target })).is_none());
        assert!(
            Tap::project(&GestureEvent::from(Rotate {
                x,
                y,
                angle: y,
                target
            }))
            .is_none()
        );
    }

    #[test]
    fn tap_options_reject_zero_and_overflow_without_narrowing() {
        assert_eq!(Tap::EVENT.options().count(), None);
        for count in [1, 2, 255] {
            assert_eq!(
                Tap::EVENT.configure((count,)).unwrap().options().count(),
                Some(u8::try_from(count).unwrap())
            );
        }
        for count in [0, 256, u32::MAX] {
            assert_eq!(TapOptions::new(count), Err(TapCountError { count }));
            assert!(matches!(Tap::EVENT.configure((count,)), Err(error) if error.count == count));
        }
        assert_eq!(core::mem::size_of::<EventKey<Tap>>(), 1);
        assert_eq!(core::mem::size_of::<EventKey<Rotate>>(), 0);
    }

    #[test]
    fn component_selectors_reuse_payload_contracts() {
        struct Node;
        impl Component for Node {}

        fn tap(_: EventKey<Tap>) {}
        fn rotate(_: EventKey<Rotate>) {}

        tap(Node::Tap);
        rotate(Node::Rotate);
        assert_eq!(Node::Tap.options().count(), None);
    }

    #[test]
    fn default_and_explicit_single_tap_are_distinct_selectors() {
        let any = Tap::EVENT.options();
        let single = TapOptions::new(1).unwrap();
        assert_ne!(any, single);
        for count in 1..=255 {
            assert!(Tap::matches_options(any, count));
            assert_eq!(Tap::matches_options(single, count), count == 1);
            assert!(Rotate::matches_options((), count));
        }
        assert!(!any.matches(0));
    }
}
