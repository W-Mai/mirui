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
        }
    }
}

impl core::error::Error for PersistenceError {}
