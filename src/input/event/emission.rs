//! Deferred widget notifications submitted after parameter borrows end.

use alloc::rc::Rc;
use core::any::type_name;
use core::cell::Cell;

use crate::ecs::commands::{CommandCommitError, CommandError, DeferredCommands};
use crate::ecs::world::WorldId;
use crate::ecs::{Entity, World};

use super::contract::{EntityEventSource, EntityNotification, Event};
use super::lifecycle::AppNotice;
use super::listeners::{EventListeners, ListenerError};
use super::notification::{
    NotificationCapacity, NotificationError, NotificationFlushError, NotificationQueue,
};

struct PendingNotifications {
    owner: WorldId,
    queue: NotificationQueue,
    // Runtime limits can shrink without replacing retained storage.
    business: Cell<Option<NotificationCapacity>>,
    app_slot: Cell<bool>,
    app_payload: Cell<(usize, usize)>,
}

impl PendingNotifications {
    fn new(
        world: &World,
        business: Option<NotificationCapacity>,
        app_slot: bool,
    ) -> Result<Self, NotificationError> {
        let pending = Self {
            owner: world.id(),
            queue: NotificationQueue::default(),
            business: Cell::new(None),
            app_slot: Cell::new(false),
            app_payload: Cell::new((0, 1)),
        };
        pending.prepare(business, app_slot)?;
        Ok(pending)
    }

    fn prepare(
        &self,
        business: Option<NotificationCapacity>,
        app_slot: bool,
    ) -> Result<(), NotificationError> {
        self.prepare_with_payload(business, app_slot, self.app_payload.get())
    }

    fn prepare_with_payload(
        &self,
        business: Option<NotificationCapacity>,
        app_slot: bool,
        app_payload: (usize, usize),
    ) -> Result<(), NotificationError> {
        self.queue.check_idle()?;
        let app_slot = self.app_slot.get() || app_slot;
        let capacity = business.unwrap_or_default();
        if !capacity.payload_alignment.is_power_of_two() {
            return Err(NotificationError::InvalidLayout);
        }
        let records = capacity
            .records
            .checked_add(usize::from(app_slot))
            .ok_or(NotificationError::SizeOverflow)?;
        let alignment = capacity.payload_alignment.max(app_payload.1);
        // One App payload can split the business payload sequence at two alignment boundaries.
        let padding = if app_payload.0 > 0 && capacity.payload_bytes > 0 {
            alignment
                .checked_sub(1)
                .and_then(|n| n.checked_mul(2))
                .ok_or(NotificationError::SizeOverflow)?
        } else {
            0
        };
        let payload_bytes = capacity
            .payload_bytes
            .checked_add(app_payload.0)
            .and_then(|bytes| bytes.checked_add(padding))
            .ok_or(NotificationError::SizeOverflow)?;
        self.queue.reserve(NotificationCapacity {
            records,
            payload_bytes,
            payload_alignment: alignment,
        })?;
        self.business.set(business);
        self.app_slot.set(app_slot);
        self.app_payload.set(app_payload);
        Ok(())
    }

    fn flush(
        &self,
        listeners: &EventListeners,
        world: &mut World,
        limit: usize,
    ) -> Result<usize, NotificationFlushError<ListenerError>> {
        self.flush_with(listeners, world, limit, |_| Ok(()))
    }

    fn flush_with(
        &self,
        listeners: &EventListeners,
        world: &mut World,
        limit: usize,
        before_dispatch: impl FnMut(&mut World) -> Result<(), ListenerError>,
    ) -> Result<usize, NotificationFlushError<ListenerError>> {
        if self.owner != world.id() {
            return Err(NotificationFlushError::Listener(
                ListenerError::ForeignOwner,
            ));
        }
        listeners.flush_notifications_with(world, &self.queue, limit, before_dispatch)
    }
}

pub(crate) fn prepare_app_payload<E: Event>(world: &World) -> Result<(), NotificationError> {
    let pending = world
        .resource::<Rc<PendingNotifications>>()
        .ok_or(NotificationError::InvalidLayout)?;
    let (bytes, alignment) = pending.app_payload.get();
    let (required, required_alignment) =
        crate::ecs::payload::PayloadLayout::of::<E>().required(0)?;
    pending.prepare_with_payload(
        pending.business.get(),
        true,
        (bytes.max(required), alignment.max(required_alignment)),
    )
}

/// Prepare notification storage without exposing the runtime service.
#[doc(hidden)]
pub fn prepare_notifications(
    world: &mut World,
    capacity: NotificationCapacity,
) -> Result<(), NotificationError> {
    if let Some(pending) = world.resource::<Rc<PendingNotifications>>() {
        pending.queue.validate_capacity(capacity)?;
        let capacity = match pending.business.get() {
            Some(current) => NotificationCapacity {
                records: current.records.max(capacity.records),
                payload_bytes: current.payload_bytes.max(capacity.payload_bytes),
                payload_alignment: current.payload_alignment.max(capacity.payload_alignment),
            },
            None => capacity,
        };
        return pending.prepare(Some(capacity), false);
    }
    prepare_runtime_notifications(world, Some(capacity), false)
}

pub(crate) fn prepare_runtime_notifications(
    world: &mut World,
    business: Option<NotificationCapacity>,
    app_slot: bool,
) -> Result<(), NotificationError> {
    if let Some(pending) = world.resource::<Rc<PendingNotifications>>() {
        return pending.prepare(business, app_slot);
    }
    if business.is_none() && !app_slot {
        return Ok(());
    }
    let pending = Rc::new(PendingNotifications::new(world, business, app_slot)?);
    world.insert_resource(pending);
    Ok(())
}

pub(crate) fn has_business_storage(world: &World) -> bool {
    world
        .resource::<Rc<PendingNotifications>>()
        .is_some_and(|pending| pending.business.get().is_some())
}

pub(crate) fn enqueue_app_notice<E: AppNotice>(
    world: &mut World,
    payload: E,
) -> Result<(), CommandError> {
    let pending = world
        .resource::<Rc<PendingNotifications>>()
        .ok_or(CommandError::Commit(CommandCommitError::MissingService {
            service: type_name::<PendingNotifications>(),
        }))?;
    if pending.owner != world.id() {
        return Err(CommandError::Commit(CommandCommitError::ForeignOwner {
            service: type_name::<PendingNotifications>(),
        }));
    }
    if !pending.app_slot.get() {
        return Err(CommandError::Commit(CommandCommitError::RecordCapacity {
            available: 0,
        }));
    }
    pending
        .queue
        .push_app_notice(payload)
        .map_err(|(error, _payload)| storage_error(error))
}

/// Flush the installed service, or return no batch when it is absent.
#[doc(hidden)]
pub fn flush_notifications(
    world: &mut World,
    listeners: &EventListeners,
    limit: usize,
) -> Option<Result<usize, NotificationFlushError<ListenerError>>> {
    let pending = world.resource::<Rc<PendingNotifications>>()?.clone();
    Some(pending.flush(listeners, world, limit))
}

pub(crate) fn flush_notifications_with(
    world: &mut World,
    listeners: &EventListeners,
    limit: usize,
    before_dispatch: impl FnMut(&mut World) -> Result<(), ListenerError>,
) -> Option<Result<usize, NotificationFlushError<ListenerError>>> {
    let pending = world.resource::<Rc<PendingNotifications>>()?.clone();
    Some(pending.flush_with(listeners, world, limit, before_dispatch))
}

impl DeferredCommands<'_> {
    /// Record a notification submitted only when its entity has a listener for this event type at command commit time.
    ///
    /// This records a request, not a successful delivery. Recording and
    /// submission errors are returned by the callback executor.
    #[doc(hidden)]
    pub fn emit<E>(&mut self, entity: Entity, payload: E)
    where
        E: Event<Route = EntityNotification>,
        E::Source: EntityEventSource,
    {
        self.apply(entity, payload, submit_if_listened::<E>);
    }

    /// Record a notification without requiring an installed listener at command commit time. Delivery uses the listeners present when the queue is flushed; undelivered events are not retained for future subscribers. Entity, source, and capacity checks still apply.
    #[doc(hidden)]
    pub fn emit_always<E>(&mut self, entity: Entity, payload: E)
    where
        E: Event<Route = EntityNotification>,
        E::Source: EntityEventSource,
    {
        self.apply(entity, payload, submit::<E>);
    }
}

impl crate::ecs::CommandCapacity {
    /// Reserve widget notifications, including their deferred submission function.
    pub const fn emit<E>(self, count: usize) -> Self
    where
        E: Event<Route = EntityNotification>,
        E::Source: EntityEventSource,
    {
        self.entity_value::<E>(count)
    }
}

pub(crate) fn submit_if_listened<E>(
    world: &mut World,
    entity: Entity,
    payload: E,
) -> Result<(), CommandError>
where
    E: Event<Route = EntityNotification>,
    E::Source: EntityEventSource,
{
    if !super::runtime::has_entity_listener::<E>(world, entity).map_err(|error| match error {
        ListenerError::ForeignOwner => CommandError::Commit(CommandCommitError::ForeignOwner {
            service: "event listeners",
        }),
        _ => CommandError::Commit(CommandCommitError::Busy),
    })? {
        return Ok(());
    }
    submit(world, entity, payload)
}

pub(crate) fn submit<E>(world: &mut World, entity: Entity, payload: E) -> Result<(), CommandError>
where
    E: Event<Route = EntityNotification>,
    E::Source: EntityEventSource,
{
    if !world.is_alive(entity) {
        return Err(CommandError::Commit(CommandCommitError::MissingTarget {
            entity,
        }));
    }
    if !E::Source::present(world, entity) {
        return Err(CommandError::Commit(CommandCommitError::MissingSource {
            entity,
            source: type_name::<E::Source>(),
        }));
    }
    let pending = world
        .resource::<Rc<PendingNotifications>>()
        .ok_or(CommandError::Commit(CommandCommitError::MissingService {
            service: type_name::<PendingNotifications>(),
        }))?;
    if pending.owner != world.id() {
        return Err(CommandError::Commit(CommandCommitError::ForeignOwner {
            service: type_name::<PendingNotifications>(),
        }));
    }
    let capacity =
        pending
            .business
            .get()
            .ok_or(CommandError::Commit(CommandCommitError::MissingService {
                service: type_name::<PendingNotifications>(),
            }))?;
    pending
        .queue
        .push_business(
            entity,
            crate::core::reactive::retained::InputLease::capture(world, entity),
            payload,
            capacity,
        )
        .map_err(|(error, _payload)| storage_error(error))
}

fn storage_error(error: NotificationError) -> CommandError {
    CommandError::Commit(match error {
        NotificationError::RecordCapacity { available } => {
            CommandCommitError::RecordCapacity { available }
        }
        NotificationError::PayloadCapacity {
            required,
            available,
        } => CommandCommitError::PayloadCapacity {
            required,
            available,
        },
        NotificationError::PayloadAlignment {
            required,
            available,
        } => CommandCommitError::PayloadAlignment {
            required,
            available,
        },
        NotificationError::Busy => CommandCommitError::Busy,
        NotificationError::SizeOverflow => return CommandError::SizeOverflow,
        NotificationError::InvalidLayout
        | NotificationError::AllocationFailed
        | NotificationError::FlushLimit { .. } => {
            unreachable!("submission only records into reserved notification storage")
        }
    })
}

#[cfg(test)]
mod tests;
