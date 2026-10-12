use super::PersistenceError;

/// Current persistence state of a Model root. Querying status performs no storage IO.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceStatus {
    /// No persistence plugin is installed.
    Unavailable,
    /// This handle has no live persistence registration in the application.
    Unregistered,
    /// The root is registered, but persistence IO is disabled. Previous errors remain recorded.
    Disabled,
    /// The current save candidate has been acknowledged. This does not verify storage contents or durability.
    Clean,
    /// The root has a save candidate awaiting successful confirmation.
    Pending,
    /// Registration, saving, restoration, or access failed.
    Failed {
        /// The latest failure, or the retained restore-protection reason.
        error: PersistenceError,
        /// Automatic saves cannot overwrite this entry until an explicit save or restoration succeeds.
        protected: bool,
    },
}
