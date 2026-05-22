extern crate alloc;

pub mod algorithm;
pub mod budget;
pub mod core;
pub mod error;
pub mod factory;
pub mod handle;
pub mod lookup;
pub mod stats;

pub use self::core::{Cache, CacheBuilder, Entry, OccupiedEntry, VacantEntry};
pub use algorithm::{Algorithm, Lru};
pub use budget::{HasSize, MaxSize};
pub use error::CacheError;
pub use factory::WithFactory;
pub use handle::Handle;
pub use lookup::{HashLookup, LinearLookup, Lookup, NodeId, OrdLookup};
pub use stats::CacheStats;

pub type LruCache<K, V> = Cache<K, V, Lru, HashLookup<K>>;
pub type LruBTreeCache<K, V> = Cache<K, V, Lru, OrdLookup<K>>;
pub type LruLinearCache<K, V> = Cache<K, V, Lru, LinearLookup<K>>;
