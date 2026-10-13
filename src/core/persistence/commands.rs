use super::{IntoDuration, Persistence, PersistenceError, SaveTarget, target::SaveRequest};
use crate::core::reactive::Signal;
use crate::{
    core::model::{ModelHandle, PersistenceTarget},
    ecs::{CommandCapacity, World, commands::DeferredCommands},
};
use alloc::{borrow::Cow, boxed::Box, vec::Vec};
use core::time::Duration;
use serde::{Serialize, de::DeserializeOwned};

enum Request {
    Save(SaveRequest),
    Restore(Option<PersistenceTarget>),
    SaveAll,
    SetEnabled(bool),
    RegisterResource {
        key: &'static str,
        register: fn(&mut Persistence<'_>, &'static str) -> Result<(), PersistenceError>,
    },
    Register(Registration),
    Remove(Cow<'static, str>),
    Autosave(Duration),
}

type Registration = Box<dyn FnOnce(&mut Persistence<'_>) -> Result<(), PersistenceError>>;

/// Persistence requests submitted after the current callback releases its borrows.
pub struct PersistenceCommands<'borrow, 'queue>(&'borrow mut DeferredCommands<'queue>);

impl<'queue> DeferredCommands<'queue> {
    /// Record persistence operations without accessing storage inside the callback.
    pub fn persistence(&mut self) -> PersistenceCommands<'_, 'queue> {
        PersistenceCommands(self)
    }
}

impl CommandCapacity {
    /// Reserve persistence requests. Model data and encoded bytes are not stored in the command queue.
    pub const fn persistence(self, count: usize) -> Self {
        self.resource_value::<Request>(count)
    }
}

impl PersistenceCommands<'_, '_> {
    /// Queue a save by Model, Signal, resource borrow, or non-Model key. Only identity is recorded; current data is read after callback borrows end, including changes made after this call. Source registration is resolved at submission, and ambiguous sources require a key.
    pub fn save(&mut self, target: impl SaveTarget) {
        self.send(Request::Save(target.into_request()));
    }

    /// Register a resource and restore it after callback borrows end.
    pub fn register_resource<T>(&mut self, key: &'static str)
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        self.send(Request::RegisterResource {
            key,
            register: |persistence, key| persistence.register_resource::<T>(key),
        });
    }

    /// Register and restore a Signal after callback borrows end. Recording this registration allocates one temporary capture box.
    pub fn register_signal<T>(&mut self, key: &'static str, signal: Signal<T>)
    where
        T: Serialize + DeserializeOwned + Clone + 'static,
    {
        self.send(Request::Register(Box::new(move |persistence| {
            persistence.register_signal(key, signal)
        })));
    }

    /// Register byte callbacks after callback borrows end. Recording this registration allocates one temporary capture box; restore errors prevent installation but do not roll back callback mutations.
    pub fn register_bytes(
        &mut self,
        key: &'static str,
        save: impl FnMut() -> Option<Vec<u8>> + 'static,
        restore: impl FnMut(&[u8]) -> Result<(), PersistenceError> + 'static,
    ) {
        self.send(Request::Register(Box::new(move |persistence| {
            persistence.register_bytes(key, save, restore)
        })));
    }

    /// Remove stored bytes without unregistering the source or resetting current values.
    pub fn remove(&mut self, key: impl Into<Cow<'static, str>>) {
        self.send(Request::Remove(key.into()));
    }

    /// Configure periodic saving after this callback. Integers specify milliseconds; `core::time::Duration` is also accepted. Zero disables periodic saving; invalid integers warn and leave the interval unchanged.
    pub fn autosave(&mut self, interval: impl IntoDuration) {
        if let Some(interval) = interval.try_into_duration() {
            self.send(Request::Autosave(interval));
        } else {
            crate::warn!(target: "mirui::persistence", "invalid autosave interval; configuration unchanged");
        }
    }

    /// Queue restoration after callback borrows end, publishing observations without delivering effects.
    pub fn restore<H: ModelHandle>(&mut self, model: &H) {
        self.send(Request::Restore(model.access().persistence_target()));
    }

    /// Save pending values without overriding failed-restore protection.
    pub fn save_all(&mut self) {
        self.send(Request::SaveAll);
    }

    /// Enable or suspend IO when this command is submitted. Enabling keeps current Model data.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.send(Request::SetEnabled(enabled));
    }

    fn send(&mut self, request: Request) {
        self.0.apply_resource(request, apply);
    }
}

fn apply(world: &mut World, request: Request) {
    let mut persistence = Persistence::new(world);
    let result = match request {
        Request::Save(target) => persistence.save_request(target),
        Request::Restore(target) => persistence.restore_target(target),
        Request::SaveAll => persistence.save_all(),
        Request::SetEnabled(enabled) => persistence.set_enabled(enabled),
        Request::RegisterResource { key, register } => register(&mut persistence, key),
        Request::Register(register) => register(&mut persistence),
        Request::Remove(key) => persistence.remove(&key),
        Request::Autosave(interval) => persistence.autosave(interval),
    };
    if let Err(error) = result {
        crate::warn!(target: "mirui::persistence", "persistence command failed: {error}");
    }
}

#[cfg(all(test, feature = "std"))]
mod tests;
