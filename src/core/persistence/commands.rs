use super::Persistence;
use crate::{
    core::model::{ModelHandle, PersistenceTarget},
    ecs::{CommandCapacity, World, commands::DeferredCommands},
};

enum Request {
    Save(Option<PersistenceTarget>),
    Restore(Option<PersistenceTarget>),
    SaveAll,
    SetEnabled(bool),
}

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
    /// Queue a save of this root. Its data is read after callback borrows end; capturing its weak identity does not extend its lifetime.
    pub fn save<H: ModelHandle>(&mut self, model: &H) {
        self.send(Request::Save(model.access().persistence_target()));
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
        Request::Save(target) => persistence.save_target(target),
        Request::Restore(target) => persistence.restore_target(target),
        Request::SaveAll => persistence.save_all(),
        Request::SetEnabled(enabled) => persistence.set_enabled(enabled),
    };
    if let Err(error) = result {
        crate::warn!(target: "mirui::persistence", "persistence command failed: {error}");
    }
}

#[cfg(all(test, feature = "std"))]
mod tests;
