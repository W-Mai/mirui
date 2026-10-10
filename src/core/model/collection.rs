use alloc::vec::Vec;
use core::cell::Cell;

/// Stable identity of a member, independent of its position or business key.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModelItemId {
    pub(super) collection: u64,
    pub(super) slot: usize,
    pub(super) generation: u64,
}

/// An effect emitted by a collection member with its stable identity.
pub struct ModelItemEffect<E> {
    pub id: ModelItemId,
    pub event: E,
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

/// A rejected batch and its unconsumed input.
///
/// `into_input` yields the staged prefix, the item that stopped loading, and the remaining iterator in input order. No input value needs `Clone`. Iterator panics follow the enclosing Model transaction's panic contract.
pub struct CollectionLoadError<I: Iterator> {
    error: CollectionError,
    staged: Vec<I::Item>,
    pending: Option<I::Item>,
    remaining: I,
}

impl<I: Iterator> CollectionLoadError<I> {
    pub fn error(&self) -> CollectionError {
        self.error
    }

    pub fn into_input(self) -> impl Iterator<Item = I::Item> {
        self.staged
            .into_iter()
            .chain(self.pending)
            .chain(self.remaining)
    }

    pub(super) fn untouched(error: CollectionError, remaining: I) -> Self {
        Self {
            error,
            staged: Vec::new(),
            pending: None,
            remaining,
        }
    }

    pub(super) fn staged(error: CollectionError, staged: Vec<I::Item>, remaining: I) -> Self {
        Self {
            error,
            staged,
            pending: None,
            remaining,
        }
    }
}

impl<I: Iterator> core::fmt::Debug for CollectionLoadError<I> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CollectionLoadError")
            .field("error", &self.error)
            .field("staged_len", &self.staged.len())
            .field("pending", &self.pending.is_some())
            .finish_non_exhaustive()
    }
}

pub(super) fn collect_input<I: Iterator>(
    remaining: I,
    limit: Option<usize>,
) -> Result<(Vec<I::Item>, I), CollectionLoadError<I>> {
    collect_input_checked(remaining, limit, |_, _| Ok(()))
}

pub(super) fn collect_input_checked<I: Iterator>(
    mut remaining: I,
    limit: Option<usize>,
    mut check: impl FnMut(&[I::Item], &I::Item) -> Result<(), CollectionError>,
) -> Result<(Vec<I::Item>, I), CollectionLoadError<I>> {
    let mut staged = Vec::new();
    while let Some(item) = remaining.next() {
        let error = if limit.is_some_and(|limit| staged.len() == limit) {
            Some(CollectionError::Limit)
        } else if let Err(error) = check(&staged, &item) {
            Some(error)
        } else if staged.len() == staged.capacity() {
            let capacity = staged
                .len()
                .saturating_mul(2)
                .max(1)
                .min(limit.unwrap_or(usize::MAX));
            staged
                .try_reserve_exact(capacity - staged.len())
                .err()
                .map(|_| CollectionError::Allocation)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(CollectionLoadError {
                error,
                staged,
                pending: Some(item),
                remaining,
            });
        }
        staged.push(item);
    }
    Ok((staged, remaining))
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
