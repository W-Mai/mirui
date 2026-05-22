extern crate alloc;

pub mod algorithm;
pub mod budget;
pub mod core;
pub mod error;
pub mod handle;
pub mod lookup;
pub mod stats;

pub use self::core::{Cache, CacheBuilder, Entry, OccupiedEntry, VacantEntry};
pub use algorithm::{Algorithm, Lru};
pub use budget::{HasSize, MaxSize};
pub use error::CacheError;
pub use handle::Handle;
pub use lookup::{HashLookup, LinearLookup, Lookup, NodeId, OrdLookup};
pub use stats::CacheStats;
