use super::*;
use crate::test_support::tracked_allocations;

#[test]
fn resized_reservations_protect_other_work_and_release_on_unwind() {
    let budget = MemoryBudget::new(128);
    let mut floor = budget.reserve(96).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            let candidate = budget.reserve(32).unwrap();
            assert!(candidate.commit(33).is_err());
            assert_eq!(floor.bytes(), 96);
            assert_eq!(budget.available(), 32);
            assert!(floor.resize(64));
            let live = budget.reserve(64).unwrap().commit(64).unwrap();
            assert!(!floor.resize(65));
            assert_eq!(floor.bytes(), 64);
            assert!(floor.resize(0));
            assert_eq!(budget.available(), 64);
            drop(live);
            assert!(floor.resize(128));
            assert!(!floor.resize(usize::MAX));
        }),
        0
    );
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _floor = floor;
        panic!("abandoned preparation");
    }));
    assert!(failure.is_err());
    assert_eq!(budget.available(), 128);
    assert_eq!(budget.allocated(), 0);
}

#[cfg(all(feature = "std", feature = "sync"))]
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
