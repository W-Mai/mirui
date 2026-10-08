use crate::ecs::payload::PayloadRequirements;
use crate::ecs::{CommandCapacity, CommandError};

use super::contract::{Event, EventRoute};
use super::notification::{NotificationCapacity, NotificationError};

/// Application-wide storage requirements for event callbacks and notifications.
///
/// Notification counts describe combined storage requirements, not individual runtime quotas. A nonzero notification requirement needs an explicit flush limit.
/// [`Default`] supplies the bounded UI preset used by [`crate::app::App`]; [`Self::new`] starts an explicit declaration without that preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventCapacity {
    notifications: Option<NotificationRequirements>,
    commands: Option<CommandCapacity>,
    flush_limit: Option<usize>,
    error: Option<EventCapacityError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NotificationRequirements {
    records: usize,
    payload: PayloadRequirements,
}

impl EventCapacity {
    /// Declare no business notification or callback command requirements.
    pub const fn new() -> Self {
        Self {
            notifications: None,
            commands: None,
            flush_limit: None,
            error: None,
        }
    }

    /// Add pending notifications of one payload type to the storage requirements.
    pub const fn notifications<E>(mut self, count: usize) -> Self
    where
        E: Event,
        E::Route: EventRoute<Decision = ()>,
    {
        if self.error.is_some() {
            return self;
        }
        let requirements = match self.notifications {
            Some(requirements) => requirements,
            None => NotificationRequirements {
                records: 0,
                payload: PayloadRequirements::new(),
            },
        };
        let Some(records) = requirements.records.checked_add(count) else {
            self.error = Some(EventCapacityError::SizeOverflow);
            return self;
        };
        let Some(payload) = requirements.payload.include::<E>(count) else {
            self.error = Some(EventCapacityError::SizeOverflow);
            return self;
        };
        if payload.limits().is_none() {
            self.error = Some(EventCapacityError::SizeOverflow);
            return self;
        }
        self.notifications = Some(NotificationRequirements { records, payload });
        self
    }

    /// Set the command requirements used by event callbacks.
    pub const fn commands(mut self, capacity: CommandCapacity) -> Self {
        if self.error.is_none() {
            match capacity.validate() {
                Ok(()) => self.commands = Some(capacity),
                Err(error) => self.error = Some(EventCapacityError::Command(error)),
            }
        }
        self
    }

    /// Set the maximum number of business notifications delivered by one outer flush.
    pub const fn flush_limit(mut self, limit: usize) -> Self {
        if self.error.is_none() {
            self.flush_limit = Some(limit);
        }
        self
    }

    pub(crate) const fn resolve(self) -> Result<EventLimits, EventCapacityError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let notifications = match self.notifications {
            Some(requirements) => {
                if requirements.records > 0 && self.flush_limit.is_none() {
                    return Err(EventCapacityError::MissingFlushLimit);
                }
                let Some((payload_bytes, payload_alignment)) = requirements.payload.limits() else {
                    return Err(EventCapacityError::SizeOverflow);
                };
                Some(NotificationCapacity {
                    records: requirements.records,
                    payload_bytes,
                    payload_alignment,
                })
            }
            None => None,
        };
        Ok(EventLimits {
            notifications,
            commands: self.commands,
            flush_limit: self.flush_limit,
        })
    }
}

impl Default for EventCapacity {
    /// 8 notification records per buffer, 4 commands per callback, and 16 deliveries per flush. Payload storage uses the actual layouts of 8 Slider value changes and a combination of one theme change and 3 transform updates.
    fn default() -> Self {
        Self::new()
            .notifications::<crate::ui::widgets::slider::ValueChanged>(8)
            .commands(
                CommandCapacity::new()
                    .set_theme(1)
                    .set::<crate::ui::property::prop::Transform>(3),
            )
            .flush_limit(16)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EventLimits {
    pub(crate) notifications: Option<NotificationCapacity>,
    pub(crate) commands: Option<CommandCapacity>,
    pub(crate) flush_limit: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EventCapacityError {
    SizeOverflow,
    MissingFlushLimit,
    Command(CommandError),
    Storage(NotificationError),
    FrozenConfiguration,
    Busy,
    ForeignOwner,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::Component;
    use crate::input::event::contract::{EntityNotification, WidgetSource};
    use crate::test_support::tracked_allocations;

    struct Pad;
    impl Component for Pad {}

    #[derive(Debug)]
    struct Small {
        _value: u8,
    }

    #[derive(Debug)]
    #[repr(align(64))]
    struct Large {
        _bytes: [u8; 64],
    }

    struct Empty;

    macro_rules! notification {
        ($($payload:ty),+ $(,)?) => {
            $(
                impl Event for $payload {
                    type Route = EntityNotification;
                    type Source = WidgetSource<Pad>;
                    type Options = ();
                }
            )+
        };
    }

    notification!(Small, Large, Empty);

    #[test]
    fn default_preset_uses_typed_layouts_and_explicit_empty_remains_empty() {
        assert_eq!(
            tracked_allocations(|| {
                let preset = EventCapacity::default().resolve().unwrap();
                let payload = PayloadRequirements::new()
                    .include::<crate::ui::widgets::slider::ValueChanged>(8)
                    .unwrap()
                    .limits()
                    .unwrap();
                assert_eq!(
                    preset.notifications,
                    Some(NotificationCapacity {
                        records: 8,
                        payload_bytes: payload.0,
                        payload_alignment: payload.1,
                    })
                );
                assert_eq!(
                    preset.commands,
                    Some(
                        CommandCapacity::new()
                            .set_theme(1)
                            .set::<crate::ui::property::prop::Transform>(3)
                    )
                );
                assert_eq!(preset.flush_limit, Some(16));
                assert_ne!(preset, EventCapacity::new().resolve().unwrap());
            }),
            0
        );
    }

    #[test]
    fn requirements_are_const_and_resolve_without_allocating() {
        const CAPACITY: EventCapacity = EventCapacity::new()
            .notifications::<Small>(2)
            .commands(CommandCapacity::new().despawn(1))
            .flush_limit(2);
        const RESOLVED: EventLimits = match CAPACITY.resolve() {
            Ok(limits) => limits,
            Err(_) => panic!("valid constant requirements"),
        };
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    EventCapacity::new()
                        .notifications::<Small>(2)
                        .commands(CommandCapacity::new().despawn(1))
                        .flush_limit(2)
                        .resolve(),
                    Ok(RESOLVED)
                );
            }),
            0
        );
        assert_eq!(
            RESOLVED.notifications,
            Some(NotificationCapacity {
                records: 2,
                payload_bytes: 2,
                payload_alignment: 1,
            })
        );
    }

    #[test]
    fn combined_payload_requirements_do_not_depend_on_declaration_order() {
        let forward = EventCapacity::new()
            .notifications::<Small>(1000)
            .notifications::<Large>(1)
            .flush_limit(1001);
        let reverse = EventCapacity::new()
            .notifications::<Large>(1)
            .notifications::<Small>(1000)
            .flush_limit(1001);
        assert_eq!(forward, reverse);
        assert_eq!(
            forward.resolve().unwrap().notifications,
            Some(NotificationCapacity {
                records: 1001,
                payload_bytes: 1127,
                payload_alignment: 64,
            })
        );
    }

    #[test]
    fn undeclared_and_explicit_zero_requirements_remain_distinct() {
        let undeclared = EventCapacity::new().resolve().unwrap();
        assert_eq!(undeclared.notifications, None);
        assert_eq!(undeclared.commands, None);
        assert_eq!(undeclared.flush_limit, None);
        let zero = EventCapacity::new()
            .notifications::<Empty>(0)
            .commands(CommandCapacity::new())
            .flush_limit(0)
            .resolve()
            .unwrap();
        assert_eq!(zero.notifications, Some(NotificationCapacity::default()));
        assert_eq!(zero.commands, Some(CommandCapacity::new()));
        assert_eq!(zero.flush_limit, Some(0));
        assert_ne!(zero, undeclared);
    }

    #[test]
    fn nonzero_record_requirements_need_an_explicit_flush_limit_even_for_zero_sized_payloads() {
        let capacity = EventCapacity::new().notifications::<Empty>(1);
        assert_eq!(
            capacity.resolve(),
            Err(EventCapacityError::MissingFlushLimit)
        );
        let zero = capacity.flush_limit(0).resolve().unwrap();
        assert_eq!(zero.flush_limit, Some(0));
        assert_eq!(
            zero.notifications,
            Some(NotificationCapacity {
                records: 1,
                payload_bytes: 0,
                payload_alignment: 1,
            })
        );
    }

    #[test]
    fn record_payload_and_command_configuration_overflows_preserve_the_first_error() {
        let records = EventCapacity::new()
            .notifications::<Empty>(usize::MAX)
            .notifications::<Empty>(1);
        let commands = CommandCapacity::new().despawn(usize::MAX).despawn(1);
        assert_eq!(
            records.commands(commands).flush_limit(1).resolve(),
            Err(EventCapacityError::SizeOverflow)
        );
        let payload = EventCapacity::new().notifications::<Large>(usize::MAX);
        assert_eq!(
            payload.flush_limit(1).resolve(),
            Err(EventCapacityError::SizeOverflow)
        );
        let first_command = EventCapacity::new().commands(commands);
        assert_eq!(
            first_command.notifications::<Large>(usize::MAX).resolve(),
            Err(EventCapacityError::Command(CommandError::SizeOverflow))
        );
    }
}
