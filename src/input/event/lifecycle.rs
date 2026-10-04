//! Notifications from committed App lifecycle and host-interaction boundaries.

use super::contract::{AppNotification, AppSource, Event};

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
