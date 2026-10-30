use crate::core::threading::{Shared as Rc, transaction};
#[cfg(not(feature = "sync-cache"))]
use core::cell::Cell;
#[cfg(feature = "sync-cache")]
use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct Counter {
    #[cfg(not(feature = "sync-cache"))]
    value: Cell<usize>,
    #[cfg(feature = "sync-cache")]
    value: AtomicUsize,
}

impl Counter {
    fn new(value: usize) -> Self {
        Self {
            #[cfg(not(feature = "sync-cache"))]
            value: Cell::new(value),
            #[cfg(feature = "sync-cache")]
            value: AtomicUsize::new(value),
        }
    }

    #[inline]
    fn get(&self) -> usize {
        #[cfg(not(feature = "sync-cache"))]
        {
            self.value.get()
        }
        #[cfg(feature = "sync-cache")]
        {
            self.value.load(Ordering::Relaxed)
        }
    }

    #[inline]
    fn set(&self, value: usize) {
        #[cfg(not(feature = "sync-cache"))]
        self.value.set(value);
        #[cfg(feature = "sync-cache")]
        self.value.store(value, Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct Ledger {
    limit: usize,
    allocated: Counter,
    reserved: Counter,
}

#[derive(Clone, Debug)]
pub(crate) struct MemoryBudget(Option<Rc<Ledger>>);

impl MemoryBudget {
    pub(crate) const OWNER_BYTES: usize =
        core::mem::size_of::<Ledger>() + 2 * core::mem::size_of::<usize>();
    pub(crate) fn new(limit: usize) -> Self {
        Self((limit != 0).then(|| {
            Rc::new(Ledger {
                limit,
                allocated: Counter::new(0),
                reserved: Counter::new(0),
            })
        }))
    }

    pub(crate) fn available(&self) -> usize {
        self.0.as_ref().map_or(0, |ledger| {
            transaction(|| ledger.limit - ledger.allocated.get() - ledger.reserved.get())
        })
    }

    pub(crate) fn reserve(&self, bytes: usize) -> Option<Reservation> {
        let ledger = self.0.as_ref()?;
        let accepted = transaction(|| {
            if bytes > ledger.limit - ledger.allocated.get() - ledger.reserved.get() {
                return false;
            }
            ledger.reserved.set(ledger.reserved.get() + bytes);
            true
        });
        if !accepted {
            return None;
        }
        Some(Reservation {
            ledger: Some(Rc::clone(ledger)),
            bytes,
        })
    }

    #[cfg(test)]
    pub(crate) fn allocated(&self) -> usize {
        self.0
            .as_ref()
            .map_or(0, |ledger| transaction(|| ledger.allocated.get()))
    }
}

#[derive(Debug)]
pub(crate) struct Reservation {
    ledger: Option<Rc<Ledger>>,
    bytes: usize,
}

impl Reservation {
    pub(crate) fn retain(mut self) -> RetainedReservation {
        RetainedReservation(Rc::new(Retained {
            ledger: self.ledger.take().expect("active reservation"),
            available: Counter::new(self.bytes),
            capacity: self.bytes,
        }))
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) fn commit(mut self, actual: usize) -> Result<Allocation, Self> {
        let ledger = self.ledger.as_ref().expect("active reservation");
        let accepted = transaction(|| {
            let remaining = ledger.reserved.get() - self.bytes;
            if actual > ledger.limit - ledger.allocated.get() - remaining {
                return false;
            }
            ledger.reserved.set(remaining);
            ledger.allocated.set(ledger.allocated.get() + actual);
            true
        });
        if !accepted {
            return Err(self);
        }
        Ok(Allocation {
            ledger: self.ledger.take().expect("active reservation"),
            bytes: actual,
        })
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(ledger) = &self.ledger {
            transaction(|| ledger.reserved.set(ledger.reserved.get() - self.bytes));
        }
    }
}

// Suballocations return to the original reservation, not to another consumer.
// The shared ledger remains the only authority for the global byte limit.
#[derive(Clone, Debug)]
pub(crate) struct RetainedReservation(Rc<Retained>);

#[derive(Debug)]
struct Retained {
    ledger: Rc<Ledger>,
    available: Counter,
    capacity: usize,
}

impl Drop for Retained {
    fn drop(&mut self) {
        transaction(|| {
            self.ledger
                .reserved
                .set(self.ledger.reserved.get() - self.available.get());
        });
    }
}

impl RetainedReservation {
    pub(crate) fn capacity(&self) -> usize {
        self.0.capacity
    }

    pub(crate) fn allocate(&self, bytes: usize) -> Option<RetainedAllocation> {
        let accepted = transaction(|| {
            if bytes > self.0.available.get() {
                return false;
            }
            self.0.available.set(self.0.available.get() - bytes);
            self.0
                .ledger
                .reserved
                .set(self.0.ledger.reserved.get() - bytes);
            self.0
                .ledger
                .allocated
                .set(self.0.ledger.allocated.get() + bytes);
            true
        });
        if !accepted {
            return None;
        }
        Some(RetainedAllocation {
            reservation: self.clone(),
            bytes,
        })
    }
}

#[derive(Debug)]
pub(crate) struct RetainedAllocation {
    reservation: RetainedReservation,
    bytes: usize,
}

impl RetainedAllocation {
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn resize(&mut self, bytes: usize) -> bool {
        let state = &self.reservation.0;
        let accepted = transaction(|| {
            if bytes > self.bytes && bytes - self.bytes > state.available.get() {
                return false;
            }
            state
                .available
                .set(state.available.get() + self.bytes - bytes);
            state
                .ledger
                .allocated
                .set(state.ledger.allocated.get() - self.bytes + bytes);
            state
                .ledger
                .reserved
                .set(state.ledger.reserved.get() + self.bytes - bytes);
            true
        });
        if !accepted {
            return false;
        }
        self.bytes = bytes;
        true
    }
}

impl Drop for RetainedAllocation {
    fn drop(&mut self) {
        self.resize(0);
    }
}

#[derive(Debug)]
pub(crate) struct Allocation {
    ledger: Rc<Ledger>,
    bytes: usize,
}

const _: () = {
    assert!(core::mem::size_of::<MemoryBudget>() == core::mem::size_of::<usize>());
    assert!(core::mem::size_of::<Reservation>() == 2 * core::mem::size_of::<usize>());
    assert!(core::mem::size_of::<Allocation>() == 2 * core::mem::size_of::<usize>());
    assert!(core::mem::size_of::<Ledger>() == 3 * core::mem::size_of::<usize>());
};

impl Allocation {
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        transaction(|| {
            self.ledger
                .allocated
                .set(self.ledger.allocated.get() - self.bytes);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tracked_allocations;

    #[cfg(all(feature = "std", feature = "sync-cache"))]
    #[test]
    fn concurrent_admission_commits_one_shared_limit() {
        use std::sync::Barrier;

        fn send_sync<T: Send + Sync>() {}
        fn send<T: Send>() {}
        send_sync::<MemoryBudget>();
        send_sync::<RetainedReservation>();
        send_sync::<RetainedAllocation>();
        send::<crate::core::cache::LruLinearCache<u32, u32>>();

        let budget = MemoryBudget::new(1024);
        let barrier = Barrier::new(9);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let budget = &budget;
                let barrier = &barrier;
                scope.spawn(move || {
                    let reservation = budget.reserve(256);
                    barrier.wait();
                    barrier.wait();
                    let allocation = reservation.map(|value| value.commit(256).unwrap());
                    barrier.wait();
                    barrier.wait();
                    drop(allocation);
                });
            }
            barrier.wait();
            assert_eq!(budget.available(), 0);
            assert_eq!(budget.allocated(), 0);
            barrier.wait();
            barrier.wait();
            assert_eq!(budget.available(), 0);
            assert_eq!(budget.allocated(), 1024);
            barrier.wait();
        });
        assert_eq!(budget.available(), 1024);
        let retained = budget.reserve(1024).unwrap().retain();
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let retained = &retained;
                scope.spawn(move || {
                    for _ in 0..1000 {
                        if let Some(mut allocation) = retained.allocate(128) {
                            allocation.resize(256);
                            allocation.resize(32);
                        }
                    }
                });
            }
        });
        assert_eq!(budget.allocated(), 0);
        assert_eq!(budget.available(), 0);
        drop(retained);
        assert_eq!(budget.available(), 1024);
    }

    #[test]
    fn retained_allocations_preserve_the_original_admission_until_last_release() {
        let budget = MemoryBudget::new(256);
        let retained = budget.reserve(192).unwrap().retain();
        assert_eq!(retained.capacity(), 192);
        let allocations = tracked_allocations(|| {
            let first = retained.allocate(64).unwrap();
            let mut second = retained.allocate(96).unwrap();
            assert_eq!(budget.allocated(), 160);
            assert_eq!(budget.available(), 64);
            assert!(!second.resize(129));
            assert_eq!(budget.allocated(), 160);
            drop(first);
            assert_eq!(budget.available(), 64);
            assert!(second.resize(192));
            assert!(retained.allocate(1).is_none());
            assert!(second.resize(32));
            let third = retained.allocate(160).unwrap();
            drop(second);
            drop(third);
        });
        assert_eq!(allocations, 0);
        let pinned = retained.allocate(48).unwrap();
        drop(retained);
        assert_eq!(budget.allocated(), 48);
        assert_eq!(budget.available(), 64);
        drop(pinned);
        assert_eq!(budget.allocated(), 0);
        assert_eq!(budget.available(), 256);
    }

    #[test]
    fn reservations_and_allocations_share_one_limit_and_release_on_drop() {
        let budget = MemoryBudget::new(64);
        let pending = budget.reserve(40).unwrap();
        let other = budget.reserve(24).unwrap();
        assert_eq!(budget.available(), 0);
        assert_eq!(budget.allocated(), 0);
        assert!(budget.reserve(1).is_none());
        let pending = pending.commit(41).unwrap_err();
        assert_eq!(budget.available(), 0);
        drop(other);
        let allocation = pending.commit(48).unwrap();
        assert_eq!(allocation.bytes(), 48);
        assert_eq!(budget.available(), 16);
        assert_eq!(budget.allocated(), 48);
        drop(allocation);
        assert_eq!(budget.available(), 64);
        assert_eq!(budget.allocated(), 0);
    }

    #[test]
    fn leases_outlive_the_budget_owner_without_cycles() {
        let budget = MemoryBudget::new(64);
        let weak = Rc::downgrade(budget.0.as_ref().unwrap());
        let pending = budget.reserve(32).unwrap();
        let allocation = budget.reserve(32).unwrap().commit(32).unwrap();
        drop(budget);
        assert!(weak.upgrade().is_some());
        drop(pending);
        assert_eq!(weak.upgrade().unwrap().allocated.get(), 32);
        drop(allocation);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn disabled_and_warm_budgets_do_not_allocate_or_overflow() {
        let allocations = tracked_allocations(|| {
            let disabled = MemoryBudget::new(0);
            assert!(disabled.reserve(1).is_none());
        });
        assert_eq!(allocations, 0);
        let allocations = tracked_allocations(|| {
            let _budget = MemoryBudget::new(64);
        });
        assert_eq!(allocations, 1);
        let budget = MemoryBudget::new(usize::MAX);
        let allocations = tracked_allocations(|| {
            for _ in 0..16 {
                let first = budget.reserve(usize::MAX - 1).unwrap();
                assert!(budget.reserve(2).is_none());
                let second = budget.reserve(1).unwrap().commit(1).unwrap();
                let first = first.commit(usize::MAX).unwrap_err();
                drop(first);
                drop(second);
                assert_eq!(budget.available(), usize::MAX);
            }
        });
        assert_eq!(allocations, 0);
    }
}
