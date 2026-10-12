use crate::core::storage::StorageError;

/// Failure to register, restore, or save persisted state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceError {
    /// The storage backend rejected an operation.
    Storage(StorageError),
    /// Stored bytes failed format or domain validation.
    InvalidArchive,
    /// The current value could not be encoded.
    Encoding,
    /// Encoding or restoration could not prepare the required capacity.
    Capacity,
    /// No endpoint is registered for the requested key.
    Unregistered,
    /// The Model belongs to a different application.
    WrongOwner,
    /// A Model transaction or conflicting borrow is still active.
    ModelBusy,
    /// An unwound Model update left its root poisoned.
    Poisoned,
    /// Persistence is disabled for this application.
    Disabled,
    /// A storage key cannot be empty.
    InvalidKey,
    /// Another live registration owns the storage key.
    DuplicateKey,
    /// No fields or custom codec describe the Model archive.
    MissingCodec,
}

impl From<StorageError> for PersistenceError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl core::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(f),
            Self::InvalidArchive => f.write_str("invalid persistence archive"),
            Self::Encoding => f.write_str("persistence encoding failed"),
            Self::Capacity => f.write_str("persistence capacity unavailable"),
            Self::Unregistered => f.write_str("persistence key is not registered"),
            Self::WrongOwner => f.write_str("persistent model belongs to a different App"),
            Self::ModelBusy => f.write_str("persistent model is busy"),
            Self::Poisoned => f.write_str("persistent model is poisoned"),
            Self::Disabled => f.write_str("persistence is disabled"),
            Self::InvalidKey => f.write_str("persistence key is empty"),
            Self::DuplicateKey => f.write_str("persistence key is already registered"),
            Self::MissingCodec => f.write_str("persistent model has no archive codec"),
        }
    }
}

impl core::error::Error for PersistenceError {}
