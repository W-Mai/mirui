use alloc::rc::Rc;
use core::cell::Cell;

#[derive(Debug)]
struct Ledger {
    limit: usize,
    allocated: Cell<usize>,
    reserved: Cell<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct MemoryBudget(Option<Rc<Ledger>>);

impl MemoryBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self((limit != 0).then(|| {
            Rc::new(Ledger {
                limit,
                allocated: Cell::new(0),
                reserved: Cell::new(0),
            })
        }))
    }

    pub(crate) fn available(&self) -> usize {
        self.0.as_ref().map_or(0, |ledger| {
            ledger.limit - ledger.allocated.get() - ledger.reserved.get()
        })
    }

    pub(crate) fn reserve(&self, bytes: usize) -> Option<Reservation> {
        let ledger = self.0.as_ref()?;
        if bytes > self.available() {
            return None;
        }
        ledger.reserved.set(ledger.reserved.get() + bytes);
        Some(Reservation {
            ledger: Some(Rc::clone(ledger)),
            bytes,
        })
    }

    #[cfg(test)]
    pub(crate) fn allocated(&self) -> usize {
        self.0.as_ref().map_or(0, |ledger| ledger.allocated.get())
    }
}

#[derive(Debug)]
pub(crate) struct Reservation {
    ledger: Option<Rc<Ledger>>,
    bytes: usize,
}

impl Reservation {
    pub(crate) fn commit(mut self, actual: usize) -> Result<Allocation, Self> {
        let ledger = self.ledger.as_ref().expect("active reservation");
        let remaining = ledger.reserved.get() - self.bytes;
        if actual > ledger.limit - ledger.allocated.get() - remaining {
            return Err(self);
        }
        ledger.reserved.set(remaining);
        ledger.allocated.set(ledger.allocated.get() + actual);
        Ok(Allocation {
            ledger: self.ledger.take().expect("active reservation"),
            bytes: actual,
        })
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(ledger) = &self.ledger {
            ledger.reserved.set(ledger.reserved.get() - self.bytes);
        }
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
        self.ledger
            .allocated
            .set(self.ledger.allocated.get() - self.bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tracked_allocations;

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
