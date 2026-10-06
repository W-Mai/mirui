//! Deferred widget notifications submitted after parameter borrows end.

use alloc::rc::Rc;
use core::any::type_name;

use crate::ecs::commands::{CommandCommitError, CommandError, DeferredCommands};
use crate::ecs::world::WorldId;
use crate::ecs::{Entity, World};

use super::contract::{EntityEventSource, EntityNotification, Event};
use super::listeners::{EventListeners, ListenerError};
use super::notification::{
    NotificationCapacity, NotificationError, NotificationFlushError, NotificationQueue,
};

struct PendingNotifications {
    owner: WorldId,
    queue: NotificationQueue,
}

impl PendingNotifications {
    fn new(world: &World, capacity: NotificationCapacity) -> Result<Self, NotificationError> {
        let queue = NotificationQueue::default();
        queue.reserve(capacity)?;
        Ok(Self {
            owner: world.id(),
            queue,
        })
    }

    fn flush(
        &self,
        listeners: &EventListeners,
        world: &mut World,
        limit: usize,
    ) -> Result<usize, NotificationFlushError<ListenerError>> {
        if self.owner != world.id() {
            return Err(NotificationFlushError::Listener(
                ListenerError::ForeignOwner,
            ));
        }
        listeners.flush_notifications(world, &self.queue, limit)
    }
}

/// Prepare notification storage without exposing the runtime service.
#[doc(hidden)]
pub fn prepare_notifications(
    world: &mut World,
    capacity: NotificationCapacity,
) -> Result<(), NotificationError> {
    if let Some(pending) = world.resource::<Rc<PendingNotifications>>() {
        return pending.queue.reserve(capacity);
    }
    let pending = Rc::new(PendingNotifications::new(world, capacity)?);
    world.insert_resource(pending);
    Ok(())
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

impl DeferredCommands<'_> {
    /// Record a widget notification for submission after callback borrows end.
    ///
    /// This records a request, not a successful delivery. Recording and
    /// submission errors are returned by the callback executor.
    #[doc(hidden)]
    pub fn emit<E>(&mut self, entity: Entity, payload: E)
    where
        E: Event<Route = EntityNotification>,
        E::Source: EntityEventSource,
    {
        self.apply(entity, payload, submit::<E>);
    }
}

fn submit<E>(world: &mut World, entity: Entity, payload: E) -> Result<(), CommandError>
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
    pending
        .queue
        .push_entity(entity, payload)
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
