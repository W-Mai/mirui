use super::*;
use crate::{
    app::App,
    core::reactive::{Computed, with_world_scope},
    test_support::tracked_allocations,
};
use alloc::{
    rc::Rc,
    string::{String, ToString},
};
use core::cell::Cell;

#[test]
fn sorted_membership_moves_non_clone_values_and_preserves_input_on_failure() {
    #[derive(Eq, Ord, PartialEq, PartialOrd)]
    struct Key(u8);
    let mut set = ModelSet::bounded(2);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(set.try_insert(Key(2)).ok(), Some(true));
            assert_eq!(set.try_insert(Key(1)).ok(), Some(true));
            assert_eq!(set.try_insert(Key(1)).ok(), Some(false));
            let (error, key) = set.try_insert(Key(3)).err().unwrap();
            assert_eq!(error, CollectionError::Capacity);
            assert_eq!(key.0, 3);
            assert_eq!(set.reserve(1), Err(CollectionError::Limit));
            let mut order = 0;
            set.for_each(|key| order = order * 10 + key.0);
            assert_eq!(order, 12);
            assert!(set.remove(&Key(1)));
            assert!(!set.remove(&Key(1)));
            assert_eq!(set.try_insert(Key(3)).ok(), Some(true));
        }),
        0
    );
}

#[test]
fn duplicate_and_absent_edits_do_not_publish_membership() {
    let mut app = App::headless(32, 32);
    let set = app.add_model(ModelSet::with_capacity(4)).finish();
    let watched = set.clone();
    let runs = Rc::new(Cell::new(0));
    let count = runs.clone();
    let contains = with_world_scope(&mut app.world, || {
        Computed::new(move || {
            count.set(count.get() + 1);
            watched.contains("a")
        })
    });
    assert!(!contains.get());
    let first = "a".to_string();
    let duplicate = first.clone();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(set.try_insert(first), Ok(true));
            assert!(contains.get());
            assert_eq!(runs.get(), 2);
            assert_eq!(set.try_insert(duplicate), Ok(false));
            assert!(!set.remove("b"));
            set.retain(|_| true);
            assert!(contains.get());
            assert_eq!(runs.get(), 2);
            assert!(set.remove("a"));
            assert!(!contains.get());
            assert_eq!(runs.get(), 3);
        }),
        0
    );
}

#[crate::model]
struct Selection {
    keys: model!(owned ModelSet<u32>),
}

#[crate::model]
impl Selection {
    #[observe]
    fn sum(&self) -> u32 {
        let mut sum = 0;
        self.keys.for_each(|key| sum += key);
        sum
    }
    fn reset(&mut self) {
        self.keys = ModelSet::from_iter([3, 3, 4]).into();
    }
    fn insert_and_reject(&mut self) -> Result<(), ()> {
        assert!(self.keys.try_insert(5).is_ok());
        Err(())
    }
}

#[test]
fn owned_set_replacement_keeps_field_observers_and_joins_parent_transactions() {
    let mut app = App::headless(32, 32);
    let parent = app
        .add_model(Selection {
            keys: ModelSet::from_iter([1, 1, 2]).into(),
        })
        .finish();
    let set = parent.keys();
    let watched = set.clone();
    let contains = with_world_scope(&mut app.world, || {
        Computed::new(move || watched.contains(&1))
    });
    let watched = parent.clone();
    let sum = with_world_scope(&mut app.world, || Computed::new(move || watched.sum()));
    assert!(contains.get());
    assert_eq!(sum.get(), 3);
    parent.reset();
    assert!(!contains.get());
    assert_eq!(sum.get(), 7);
    assert_eq!(parent.insert_and_reject(), Err(()));
    assert!(set.contains(&5));
    assert_eq!(sum.get(), 7);
    assert!(set.remove(&3));
    assert_eq!(sum.get(), 9);
}

#[test]
fn explicit_reserve_prepares_first_edit_and_borrowed_visits_block_reentry() {
    let mut app = App::headless(32, 32);
    let set = app.add_model(ModelSet::<String>::new()).finish();
    set.reserve(2).unwrap();
    let key = String::from("key");
    assert_eq!(
        tracked_allocations(|| assert_eq!(set.try_insert(key), Ok(true))),
        0
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        set.for_each(|_| set.clear());
    }));
    assert!(result.is_err());
    assert!(set.contains("key"));
    assert!(set.remove("key"));
    assert!(set.is_empty());
}

#[test]
fn set_import_limits_distinct_members_and_recovers_duplicate_input() {
    #[derive(Eq, Ord, PartialEq, PartialOrd)]
    struct Key(u8);
    let mut set = ModelSet::bounded(2);
    set.load([Key(1), Key(1), Key(2), Key(2)]).unwrap();
    assert_eq!(set.len(), 2);
    let consumed = Cell::new(0);
    let input = [Key(1), Key(1), Key(2), Key(3), Key(4)]
        .into_iter()
        .inspect(|_| consumed.set(consumed.get() + 1));
    let error = set.reconcile(input).unwrap_err();
    assert_eq!(error.error(), CollectionError::Limit);
    assert_eq!(consumed.get(), 4);
    assert!(error.into_input().map(|key| key.0).eq([1, 1, 2, 3, 4]));
    assert!(set.contains(&Key(1)));
    assert!(set.contains(&Key(2)));
    assert_eq!(set.limit(), Some(2));
    set.load([]).unwrap();
    assert_eq!(set.capacity(), 2);
    assert_eq!(
        tracked_allocations(|| assert!(set.try_insert(Key(8)).is_ok())),
        0
    );
}

#[test]
fn membership_reconciliation_publishes_only_the_difference_and_keeps_parent_observers() {
    let mut app = App::headless(32, 32);
    let selection = app
        .add_model(Selection {
            keys: ModelSet::from_iter([1, 2]).into(),
        })
        .finish();
    let watched = selection.clone();
    let runs = Rc::new(Cell::new(0));
    let count = runs.clone();
    let sum = with_world_scope(&mut app.world, || {
        Computed::new(move || {
            count.set(count.get() + 1);
            watched.sum()
        })
    });
    assert_eq!(sum.get(), 3);
    let keys = selection.keys();
    let watched = keys.clone();
    let membership_runs = Rc::new(Cell::new(0));
    let count = membership_runs.clone();
    let len = with_world_scope(&mut app.world, || {
        Computed::new(move || {
            count.set(count.get() + 1);
            watched.len()
        })
    });
    assert_eq!(len.get(), 2);
    keys.reconcile([2, 1, 2, 1]).unwrap();
    assert_eq!(sum.get(), 3);
    assert_eq!(len.get(), 2);
    assert_eq!(runs.get(), 1);
    assert_eq!(membership_runs.get(), 1);
    keys.reconcile([4, 2, 4]).unwrap();
    assert_eq!(sum.get(), 6);
    assert_eq!(len.get(), 2);
    assert_eq!(membership_runs.get(), 2);
    keys.load([9, 9]).unwrap();
    assert_eq!(sum.get(), 9);
    assert_eq!(len.get(), 1);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(keys.try_insert(7), Ok(true));
            assert_eq!(sum.get(), 16);
            assert_eq!(len.get(), 2);
        }),
        0
    );
}

#[test]
fn reconciliation_retains_the_existing_value_for_equal_keys() {
    struct Key {
        id: u8,
        old: bool,
    }
    impl PartialEq for Key {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
        }
    }
    impl Eq for Key {}
    impl Ord for Key {
        fn cmp(&self, other: &Self) -> core::cmp::Ordering {
            self.id.cmp(&other.id)
        }
    }
    impl PartialOrd for Key {
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    let mut set = ModelSet::from_iter([Key { id: 1, old: true }, Key { id: 2, old: true }]);
    set.reconcile([Key { id: 2, old: false }, Key { id: 3, old: false }])
        .unwrap();
    set.for_each(|key| assert_eq!(key.old, key.id == 2));
    set.reconcile([Key { id: 3, old: true }, Key { id: 2, old: false }])
        .unwrap();
    set.for_each(|key| assert_eq!(key.old, key.id == 2));
    set.load([Key { id: 2, old: false }]).unwrap();
    set.for_each(|key| assert!(!key.old));
}
