use core::cell::Cell;

/// Stable identity of a member, independent of its position or business key.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModelItemId {
    pub(super) collection: u64,
    pub(super) slot: usize,
    pub(super) generation: u64,
}

/// A collection operation that could not be completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollectionError {
    Capacity,
    Limit,
    Allocation,
    Index,
    Missing,
    IdentityExhausted,
    UnpreparedShape,
    Iterating,
    DuplicateKey,
}

pub(super) fn next_collection_id() -> u64 {
    static NEXT: critical_section::Mutex<Cell<u64>> = critical_section::Mutex::new(Cell::new(1));
    critical_section::with(|section| {
        let counter = NEXT.borrow(section);
        let id = counter.get();
        counter.set(
            id.checked_add(1)
                .expect("model collection identity exhausted"),
        );
        id
    })
}
