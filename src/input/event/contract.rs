//! Typed event identity, source capabilities, options, and callback results.

use core::marker::PhantomData;

use crate::ecs::{Component, Entity, World};

use super::BubbleControl;

mod sealed {
    pub trait Route {}
    pub trait Source {}
    pub trait EntitySourceFor<W> {}
    pub trait Result<R> {}
}

/// A payload's routing, installation source, and selector options.
pub trait Event: Sized + 'static {
    type Route: EventRoute;
    type Source: EventSource<Self::Route>;
    type Options: EventOptions;

    const EVENT: EventKey<Self> = EventKey::new();
}

/// The callback decision required by an event route.
///
/// Route implementations are supplied by the library. Notifications require
/// `()`; consumable routes require a [`BubbleControl`] decision.
pub trait EventRoute: sealed::Route + 'static {
    type Decision;
}

/// A consumable event routed from a target through its ancestors.
pub struct EntityBubble;
/// A direct, non-consumable notification from a widget.
pub struct EntityNotification;
/// Input handled by the focus/scroll route before reaching App listeners.
pub struct InputFallback;
/// A non-consumable App lifecycle notification.
pub struct AppNotification;

macro_rules! routes {
    ($($route:ty => $decision:ty),+ $(,)?) => {
        $(
            impl sealed::Route for $route {}
            impl EventRoute for $route {
                type Decision = $decision;
            }
        )+
    };
}

routes!(
    EntityBubble => BubbleControl,
    EntityNotification => (),
    InputFallback => BubbleControl,
    AppNotification => (),
);

/// A route accepted by an entity event installer.
#[doc(hidden)]
pub trait EntityRoute: EventRoute {}
impl EntityRoute for EntityBubble {}
impl EntityRoute for EntityNotification {}
impl EntityRoute for InputFallback {}

/// A route accepted by an App event installer.
#[doc(hidden)]
pub trait AppRoute: EventRoute {}
impl AppRoute for InputFallback {}
impl AppRoute for AppNotification {}

/// A source that can produce events on `R`.
///
/// Only library-defined source/route pairs implement this capability.
pub trait EventSource<R: EventRoute>: sealed::Source + 'static {}

/// Gestures attached to entity nodes.
pub struct EntitySource;
/// Focused keyboard input with an App fallback.
pub struct InputSource;
/// App notifications or input with only an App listener.
pub struct AppSource;
/// Notifications emitted by the widget component `W`.
pub struct WidgetSource<W: Component>(PhantomData<fn() -> W>);

impl sealed::Source for EntitySource {}
impl sealed::Source for InputSource {}
impl sealed::Source for AppSource {}
impl<W: Component> sealed::Source for WidgetSource<W> {}

impl EventSource<EntityBubble> for EntitySource {}
impl EventSource<InputFallback> for InputSource {}
impl EventSource<InputFallback> for AppSource {}
impl EventSource<AppNotification> for AppSource {}
impl<W: Component> EventSource<EntityNotification> for WidgetSource<W> {}

/// A source checked against an entity's installed components.
#[doc(hidden)]
pub trait EntityEventSource: sealed::Source {
    #[doc(hidden)]
    fn present(world: &World, entity: Entity) -> bool;
}
impl EntityEventSource for EntitySource {
    fn present(_world: &World, _entity: Entity) -> bool {
        true
    }
}
impl EntityEventSource for InputSource {
    fn present(_world: &World, _entity: Entity) -> bool {
        true
    }
}
impl<W: Component> EntityEventSource for WidgetSource<W> {
    fn present(world: &World, entity: Entity) -> bool {
        world.get::<W>(entity).is_some()
    }
}

/// A source accepted by a typed widget installer for component `W`.
#[doc(hidden)]
pub trait EntitySourceFor<W: Component>: EntityEventSource + sealed::EntitySourceFor<W> {}
impl<W: Component> sealed::EntitySourceFor<W> for EntitySource {}
impl<W: Component> sealed::EntitySourceFor<W> for InputSource {}
impl<W: Component> sealed::EntitySourceFor<W> for WidgetSource<W> {}
impl<W: Component> EntitySourceFor<W> for EntitySource {}
impl<W: Component> EntitySourceFor<W> for InputSource {}
impl<W: Component> EntitySourceFor<W> for WidgetSource<W> {}

/// A source accepted by an App event installer.
#[doc(hidden)]
pub trait AppEventSource: sealed::Source {}
impl AppEventSource for InputSource {}
impl AppEventSource for AppSource {}

/// Installation-time options with a typed argument list and explicit errors.
pub trait EventOptions: Copy + 'static {
    type Arguments;
    type Error: core::fmt::Debug;

    const DEFAULT: Self;

    fn from_arguments(arguments: Self::Arguments) -> Result<Self, Self::Error>;
}

impl EventOptions for () {
    type Arguments = ();
    type Error = core::convert::Infallible;

    const DEFAULT: Self = ();

    fn from_arguments((): ()) -> Result<Self, Self::Error> {
        Ok(())
    }
}

/// An event selector containing only its options, not a payload or callback.
pub struct EventKey<E: Event> {
    options: E::Options,
    payload: PhantomData<fn() -> E>,
}

impl<E: Event> Copy for EventKey<E> {}

impl<E: Event> Clone for EventKey<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: Event> Default for EventKey<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Event> EventKey<E> {
    pub const fn new() -> Self {
        Self {
            options: E::Options::DEFAULT,
            payload: PhantomData,
        }
    }

    pub const fn options(&self) -> E::Options {
        self.options
    }

    /// Validate selector arguments without narrowing casts or hidden storage.
    pub fn configure(
        self,
        arguments: <E::Options as EventOptions>::Arguments,
    ) -> Result<Self, <E::Options as EventOptions>::Error> {
        Ok(Self {
            options: E::Options::from_arguments(arguments)?,
            payload: PhantomData,
        })
    }
}

/// A callback result permitted by route `R`.
///
/// Notifications accept only `()`. Consumable events accept `()`, `bool`,
/// or [`BubbleControl`]; `()` and `true` stop propagation.
pub trait EventResult<R: EventRoute>: sealed::Result<R> {
    fn into_decision(self) -> R::Decision;
}

macro_rules! consumable_results {
    ($($route:ty),+ $(,)?) => {
        $(
            impl sealed::Result<$route> for () {}
            impl EventResult<$route> for () {
                fn into_decision(self) -> BubbleControl {
                    BubbleControl::Prevent
                }
            }

            impl sealed::Result<$route> for bool {}
            impl EventResult<$route> for bool {
                fn into_decision(self) -> BubbleControl {
                    if self { BubbleControl::Prevent } else { BubbleControl::Allow }
                }
            }

            impl sealed::Result<$route> for BubbleControl {}
            impl EventResult<$route> for BubbleControl {
                fn into_decision(self) -> BubbleControl {
                    self
                }
            }
        )+
    };
}

consumable_results!(EntityBubble, InputFallback);

macro_rules! notification_results {
    ($($route:ty),+ $(,)?) => {
        $(
            impl sealed::Result<$route> for () {}
            impl EventResult<$route> for () {
                fn into_decision(self) {}
            }
        )+
    };
}

notification_results!(EntityNotification, AppNotification);

macro_rules! widget_events {
    (
        $widget:ty;
        $(#[$attribute:meta])*
        pub enum $carrier:ident {
            $($(#[$variant_attribute:meta])* $kind:ident
                $({ $($(#[$field_attribute:meta])* $field:ident: $ty:ty),+ $(,)? })?
            ),+ $(,)?
        }
    ) => {
        $(#[$attribute])*
        pub enum $carrier {
            $($(#[$variant_attribute])* $kind
                $({ $($(#[$field_attribute])* $field: $ty),+ })?
            ),+
        }

        $(
            $crate::input::event::contract::widget_events!(
                @payload
                $(#[$variant_attribute])*
                $kind $({ $($(#[$field_attribute])* $field: $ty),+ })?
            );

            impl $crate::input::event::contract::Event for $kind {
                type Route = $crate::input::event::contract::EntityNotification;
                type Source = $crate::input::event::contract::WidgetSource<$widget>;
                type Options = ();
            }
        )+

        impl $widget {
            $(
                #[allow(non_upper_case_globals)]
                pub const $kind: $crate::input::event::contract::EventKey<$kind> =
                    <$kind as $crate::input::event::contract::Event>::EVENT;
            )+
        }
    };
    (@payload $(#[$attribute:meta])* $kind:ident { $($(#[$field_attribute:meta])* $field:ident: $ty:ty),+ }) => {
        $(#[$attribute])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $kind {
            $($(#[$field_attribute])* pub $field: $ty),+
        }
    };
    (@payload $(#[$attribute:meta])* $kind:ident) => {
        $(#[$attribute])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $kind;
    };
}

pub(crate) use widget_events;

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    struct Notice;
    impl Event for Notice {
        type Route = AppNotification;
        type Source = AppSource;
        type Options = ();
    }

    #[derive(Clone, Copy)]
    struct Options(u8);

    impl EventOptions for Options {
        type Arguments = u32;
        type Error = ();
        const DEFAULT: Self = Self(1);

        fn from_arguments(count: u32) -> Result<Self, Self::Error> {
            match u8::try_from(count) {
                Ok(count) if count != 0 => Ok(Self(count)),
                _ => Err(()),
            }
        }
    }

    struct Configured;
    impl Event for Configured {
        type Route = EntityBubble;
        type Source = EntitySource;
        type Options = Options;
    }

    #[test]
    fn selectors_only_store_validated_options_without_owning_the_payload() {
        fn requires_copy<T: Copy>(_: T) {}
        requires_copy(Notice::EVENT);
        requires_copy(Configured::EVENT);
        assert_eq!(size_of::<EventKey<Notice>>(), 0);
        assert_eq!(size_of::<EventKey<Configured>>(), size_of::<Options>());
        assert_eq!(Configured::EVENT.options().0, 1);
        assert_eq!(Configured::EVENT.configure(255).unwrap().options().0, 255);
        for invalid in [0, 256, u32::MAX] {
            assert!(Configured::EVENT.configure(invalid).is_err());
        }
    }

    #[test]
    fn result_policy_follows_the_route() {
        assert_eq!(
            <() as EventResult<EntityBubble>>::into_decision(()),
            BubbleControl::Prevent
        );
        assert_eq!(
            <bool as EventResult<InputFallback>>::into_decision(false),
            BubbleControl::Allow
        );
        assert_eq!(
            <bool as EventResult<EntityBubble>>::into_decision(true),
            BubbleControl::Prevent
        );
        assert_eq!(
            <BubbleControl as EventResult<InputFallback>>::into_decision(BubbleControl::Allow),
            BubbleControl::Allow
        );
        <() as EventResult<EntityNotification>>::into_decision(());
        <() as EventResult<AppNotification>>::into_decision(());
    }
}
