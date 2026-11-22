use crate::core::threading::{Shared as Rc, transaction};
#[cfg(not(feature = "sync"))]
use core::cell::Cell;
#[cfg(feature = "sync")]
use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct Counter {
    #[cfg(not(feature = "sync"))]
    value: Cell<usize>,
    #[cfg(feature = "sync")]
    value: AtomicUsize,
}

impl Counter {
    fn new(value: usize) -> Self {
        Self {
            #[cfg(not(feature = "sync"))]
            value: Cell::new(value),
            #[cfg(feature = "sync")]
            value: AtomicUsize::new(value),
        }
    }

    #[inline]
    fn get(&self) -> usize {
        #[cfg(not(feature = "sync"))]
        {
            self.value.get()
        }
        #[cfg(feature = "sync")]
        {
            self.value.load(Ordering::Relaxed)
        }
    }

    #[inline]
    fn set(&self, value: usize) {
        #[cfg(not(feature = "sync"))]
        self.value.set(value);
        #[cfg(feature = "sync")]
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
    pub(crate) fn resize(&mut self, bytes: usize) -> bool {
        let ledger = self.ledger.as_ref().expect("active reservation");
        transaction(|| {
            let other = ledger.reserved.get() - self.bytes;
            if bytes > ledger.limit - ledger.allocated.get() - other {
                return false;
            }
            ledger.reserved.set(other + bytes);
            self.bytes = bytes;
            true
        })
    }

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

    #[cfg(all(feature = "std", feature = "sync"))]
    pub(crate) fn split(&mut self, bytes: usize) -> Option<Self> {
        let remaining = self.bytes.checked_sub(bytes)?;
        let allocation = Self {
            ledger: Rc::clone(&self.ledger),
            bytes,
        };
        self.bytes = remaining;
        Some(allocation)
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
#[path = "../../tests/unit/render/budget.rs"]
mod tests;
