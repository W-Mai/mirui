//! Notifications from committed App lifecycle and host-interaction boundaries.

use super::contract::{AppNotification, AppSource, Event};

pub(crate) trait AppNotice:
    Event<Route = AppNotification, Source = AppSource, Options = ()>
{
}

macro_rules! notifications {
    ($($kind:ident),+ $(,)?) => {
        $(
            /// A non-consumable App notification.
            #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
            pub struct $kind;

            impl Event for $kind {
                type Route = AppNotification;
                type Source = AppSource;
                type Options = ();
            }

            impl AppNotice for $kind {}
        )+
    };
}

notifications!(
    Started,
    Suspended,
    Resumed,
    MemoryWarning,
    HostInteraction,
    Quit
);

/// A change to the surface's authoritative logical-to-physical mapping, before layout settles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportChanged {
    pub old: crate::types::Viewport,
    pub new: crate::types::Viewport,
}

impl Event for ViewportChanged {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

impl AppNotice for ViewportChanged {}

/// A change to the surface's safe-area insets, expressed in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SafeAreaChanged {
    pub old: crate::surface::SafeAreaInsets,
    pub new: crate::surface::SafeAreaInsets,
}

impl Event for SafeAreaChanged {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

impl AppNotice for SafeAreaChanged {}
