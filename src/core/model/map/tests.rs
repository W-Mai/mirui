use super::*;
use crate::{app::App, test_support::tracked_allocations};
use alloc::string::{String, ToString};

#[crate::model]
struct Item {
    #[observe]
    value: u32,
}

#[crate::model]
impl Item {
    fn increment(&mut self) {
        self.value += 1;
    }
}

#[test]
fn ordered_borrowed_keys_and_replacement_preserve_identity_without_allocation() {
    let mut map = ModelMap::bounded(3);
    let keys = ["z".to_string(), "a".to_string(), "m".to_string()];
    let mut ids = [None; 3];
    assert_eq!(
        tracked_allocations(|| {
            for (index, key) in keys.into_iter().enumerate() {
                ids[index] = Some(
                    map.try_insert(
                        key,
                        Item {
                            value: index as u32,
                        },
                    )
                    .ok()
                    .unwrap()
                    .0,
                );
            }
            assert!(map.contains_key("a"));
            assert!(!map.contains_key("b"));
            assert_eq!(map.get("m").unwrap().value(), 2);
            assert!(map.iter().map(|(key, _)| key.as_str()).eq(["a", "m", "z"]));
        }),
        0
    );
    let key = "m".to_string();
    assert_eq!(
        tracked_allocations(|| {
            let (id, old) = map.try_insert(key, Item { value: 20 }).ok().unwrap();
            assert_eq!(Some(id), ids[2]);
            assert_eq!(old.unwrap().value, 2);
            for (_, item) in map.iter_mut() {
                item.increment();
            }
            map.retain(|key, _| key != "z");
        }),
        0
    );
    assert_eq!(map.get("a").unwrap().value(), 2);
    assert_eq!(map.get("m").unwrap().value(), 21);
    assert!(map.get_by_id(ids[0].unwrap()).is_none());
}

#[test]
fn registered_map_members_share_observation_and_iterator_guards() {
    let mut app = App::headless(32, 32);
    let data = ModelMap::from_entries([(2, Item { value: 2 }), (1, Item { value: 1 })])
        .ok()
        .unwrap();
    let map = app.add_model(data).finish();
    assert_eq!(map.values().next().unwrap().value(), 1);
    let (id, member) = map.entries().next().unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for item in map.values() {
                item.increment();
            }
            assert_eq!(map.get(&1).unwrap().value(), 2);
            let (same_id, old) = map.try_insert(1, Item { value: 9 }).ok().unwrap();
            assert_eq!(same_id, id);
            assert_eq!(old.unwrap().value, 2);
            assert_eq!(member.value(), 9);
            let mut sum = 0;
            map.for_each(|key, value| sum += key + value.value());
            assert_eq!(sum, 15);
        }),
        0
    );
    let lease = map.entries();
    assert_eq!(map.clear(), Err(CollectionError::Iterating));
    assert_eq!(
        map.try_insert(3, Item { value: 3 }).err().unwrap().0,
        CollectionError::Iterating
    );
    drop(lease);
    map.remove(&1).unwrap();
    let new_id = map.try_insert(0, Item { value: 7 }).ok().unwrap().0;
    assert_ne!(id, new_id);
    assert!(map.get_by_id(id).is_none());
    assert!(!super::super::ModelHandle::callback_ready(&member));
    assert_eq!(map.get(&0).unwrap().value(), 7);
}

#[test]
fn duplicate_import_returns_all_non_clone_data() {
    let (error, items) = ModelMap::from_entries([
        (String::from("b"), Item { value: 3 }),
        (String::from("a"), Item { value: 1 }),
        (String::from("b"), Item { value: 2 }),
    ])
    .err()
    .unwrap();
    assert_eq!(error, CollectionError::DuplicateKey);
    assert_eq!(items.len(), 3);
    assert_eq!(items.iter().map(|(_, item)| item.value).sum::<u32>(), 6);
}

#[test]
fn map_load_preflight_returns_original_order_and_leaves_old_members_untouched() {
    #[derive(Eq, PartialEq, Ord, PartialOrd)]
    struct Key(u8);
    let mut map = ModelMap::bounded(3);
    let old = map.try_insert(Key(9), Item { value: 9 }).ok().unwrap().0;
    let error = map
        .load([
            (Key(2), Item { value: 2 }),
            (Key(1), Item { value: 1 }),
            (Key(2), Item { value: 3 }),
        ])
        .unwrap_err();
    assert_eq!(error.error(), CollectionError::DuplicateKey);
    assert!(
        error
            .into_input()
            .map(|(key, item)| (key.0, item.value))
            .eq([(2, 2), (1, 1), (2, 3)])
    );
    assert_eq!(map.get_by_id(old).unwrap().value(), 9);
    let consumed = core::cell::Cell::new(0);
    let error = map
        .load((0..6).map(|key| {
            consumed.set(consumed.get() + 1);
            (
                Key(key),
                Item {
                    value: u32::from(key),
                },
            )
        }))
        .unwrap_err();
    assert_eq!(error.error(), CollectionError::Limit);
    assert_eq!(consumed.get(), 4);
    assert!(error.into_input().map(|(key, _)| key.0).eq(0..6));
    assert_eq!(map.get_by_id(old).unwrap().value(), 9);
    map.load([(Key(2), Item { value: 2 }), (Key(1), Item { value: 1 })])
        .unwrap();
    assert!(map.iter().map(|(key, _)| key.0).eq([1, 2]));
    assert!(map.get_by_id(old).is_none());
    assert_eq!(map.capacity(), 3);
    assert_eq!(map.limit(), Some(3));
    assert_eq!(
        tracked_allocations(|| assert!(map.try_insert(Key(3), Item { value: 3 }).is_ok())),
        0
    );
}

#[test]
fn map_reconcile_preserves_matching_ids_and_returns_unsorted_duplicates_before_merging() {
    let mut map = ModelMap::from_entries([(2, Item { value: 2 }), (4, Item { value: 4 })])
        .ok()
        .unwrap();
    let ids = map.entries().map(|entry| entry.0).collect::<Vec<_>>();
    let error = map
        .reconcile(
            [
                (4, Item { value: 40 }),
                (1, Item { value: 1 }),
                (4, Item { value: 41 }),
            ],
            |_, _| panic!("duplicate batch must not merge"),
        )
        .unwrap_err();
    assert_eq!(error.error(), CollectionError::DuplicateKey);
    assert!(
        error
            .into_input()
            .map(|(key, item)| (key, item.value))
            .eq([(4, 40), (1, 1), (4, 41)])
    );
    assert_eq!(map.get_by_id(ids[0]).unwrap().value(), 2);
    assert_eq!(map.get_by_id(ids[1]).unwrap().value(), 4);
    map.reconcile(
        [
            (5, Item { value: 5 }),
            (2, Item { value: 20 }),
            (1, Item { value: 1 }),
        ],
        |old, new| old.value = new.value,
    )
    .unwrap();
    assert!(
        map.iter()
            .map(|(key, item)| (*key, item.value()))
            .eq([(1, 1), (2, 20), (5, 5)])
    );
    assert_eq!(map.entries().nth(1).unwrap().0, ids[0]);
    assert!(map.get_by_id(ids[1]).is_none());
    let mut calls = 0;
    map.reconcile(
        [
            (2, Item { value: 21 }),
            (1, Item { value: 10 }),
            (5, Item { value: 50 }),
        ],
        |old, new| {
            calls += 1;
            old.value = new.value;
        },
    )
    .unwrap();
    assert_eq!(calls, 3);
    assert!(
        map.iter()
            .map(|(key, item)| (*key, item.value()))
            .eq([(1, 10), (2, 21), (5, 50)])
    );
}

#[test]
fn panicking_map_merge_keeps_keys_aligned_with_completed_member_changes() {
    let mut map = ModelMap::from_entries([
        (2, Item { value: 2 }),
        (4, Item { value: 4 }),
        (6, Item { value: 6 }),
    ])
    .ok()
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        map.reconcile(
            [
                (7, Item { value: 7 }),
                (4, Item { value: 40 }),
                (3, Item { value: 3 }),
                (2, Item { value: 20 }),
                (1, Item { value: 1 }),
            ],
            |old, new| {
                assert_ne!(new.value, 40, "merge interrupted");
                old.value = new.value;
            },
        )
        .unwrap();
    }));
    assert!(result.is_err());
    assert!(map.iter().map(|(key, item)| (*key, item.value())).eq([
        (1, 1),
        (2, 20),
        (3, 3),
        (4, 4)
    ]));
    assert_eq!(map.get(&2).unwrap().value(), 20);
    assert!(map.get(&6).is_none());
    assert_eq!(
        tracked_allocations(|| {
            assert!(map.try_insert(5, Item { value: 5 }).is_ok());
            assert_eq!(map.get(&5).unwrap().value(), 5);
        }),
        0
    );
}

#[derive(Clone, Copy)]
struct MergeNotice(u32);
#[crate::model]
struct NotifyingItem {
    #[observe]
    value: u32,
    notice: Option<MergeNotice>,
}
#[crate::model]
impl NotifyingItem {
    #[effects]
    fn take_notice(&mut self) -> [Option<MergeNotice>; 1] {
        [self.notice.take()]
    }
}

#[test]
fn map_reconcile_preserves_effect_routes_and_observations_and_rejects_leases() {
    use crate::core::{
        model::ModelHandle,
        reactive::{Computed, with_world_scope},
    };
    let mut app = App::headless(32, 32);
    let map = app
        .add_model(
            ModelMap::from_entries([(
                2,
                NotifyingItem {
                    value: 2,
                    notice: None,
                },
            )])
            .ok()
            .unwrap(),
        )
        .finish();
    let (id, item) = map.entries().next().unwrap();
    let observed = item.clone();
    let value = with_world_scope(&mut app.world, || Computed::new(move || observed.value()));
    assert_eq!(value.get(), 2);
    let calls = alloc::rc::Rc::new(core::cell::Cell::new(0));
    let count = calls.clone();
    let published = value.clone();
    item.on_effect(move |notice: &MergeNotice| {
        assert_eq!(published.get(), notice.0);
        count.set(notice.0);
    })
    .unwrap();
    let held = map.entries();
    let consumed = core::cell::Cell::new(0);
    let input = [(
        2,
        NotifyingItem {
            value: 20,
            notice: Some(MergeNotice(20)),
        },
    )]
    .into_iter()
    .inspect(|_| consumed.set(consumed.get() + 1));
    let error = map
        .reconcile(input, |_, _| panic!("leased collection"))
        .unwrap_err();
    assert_eq!(error.error(), CollectionError::Iterating);
    assert_eq!(consumed.get(), 0);
    drop(held);
    map.reconcile(error.into_input(), |old, new| *old = new)
        .unwrap();
    assert_eq!(map.entries().next().unwrap().0, id);
    assert_eq!(item.value(), 20);
    assert_eq!(value.get(), 20);
    assert_eq!(calls.get(), 20);
    assert!(ModelHandle::callback_ready(&item));
    map.reconcile([], |_, _| {}).unwrap();
    assert!(map.is_empty());
    assert!(!ModelHandle::callback_ready(&item));
}

#[crate::model]
struct Group {
    items: model!(owned ModelMap<u32, Item>),
}

#[crate::model]
impl Group {
    #[observe]
    fn total(&self) -> u32 {
        self.items.values().map(|item| item.value()).sum()
    }

    fn reset(&mut self) {
        self.items = ModelMap::from_entries([(8, Item { value: 8 }), (3, Item { value: 3 })])
            .ok()
            .unwrap()
            .into();
    }

    fn load(&mut self) {
        self.items
            .load([(8, Item { value: 80 }), (3, Item { value: 30 })])
            .unwrap();
    }
}

#[test]
fn map_load_keeps_field_observers_and_retires_children_in_parent_transactions() {
    use crate::core::{
        model::ModelHandle,
        reactive::{Computed, with_world_scope},
    };
    let mut app = App::headless(32, 32);
    let group = app
        .add_model(Group {
            items: ModelMap::from_entries([(3, Item { value: 3 })])
                .ok()
                .unwrap()
                .into(),
        })
        .finish();
    let map = group.items();
    let watched = group.clone();
    let total = with_world_scope(&mut app.world, || Computed::new(move || watched.total()));
    assert_eq!(total.get(), 3);
    let (old_id, old) = map.entries().next().unwrap();
    let held = map.values();
    let consumed = core::cell::Cell::new(0);
    let input = [(7, Item { value: 7 })]
        .into_iter()
        .inspect(|_| consumed.set(consumed.get() + 1));
    let error = map.load(input).unwrap_err();
    assert_eq!(error.error(), CollectionError::Iterating);
    assert_eq!(consumed.get(), 0);
    drop(held);
    group.load();
    assert_eq!(total.get(), 110);
    assert_eq!(map.get(&3).unwrap().value(), 30);
    assert_eq!(map.get(&8).unwrap().value(), 80);
    assert!(!ModelHandle::callback_ready(&old));
    assert!(map.get_by_id(old_id).is_none());
    map.load(error.into_input()).unwrap();
    assert_eq!(total.get(), 7);
    assert_eq!(map.get(&7).unwrap().value(), 7);
    assert_eq!(
        tracked_allocations(|| {
            map.get(&7).unwrap().increment();
            assert_eq!(total.get(), 8);
            assert!(map.try_insert(9, Item { value: 9 }).is_ok());
            map.retain(|key, _| *key == 9).unwrap();
            assert_eq!(map.get(&9).unwrap().value(), 9);
        }),
        0
    );
}

#[test]
fn explicit_reservation_preserves_observers_and_prepares_first_insert() {
    use crate::core::reactive::{Computed, with_world_scope};
    let mut app = App::headless(32, 32);
    let group = app
        .add_model(Group {
            items: ModelMap::new().into(),
        })
        .finish();
    let map = group.items();
    let observed = map.clone();
    let missing = with_world_scope(&mut app.world, || {
        Computed::new(move || observed.get(&3).map(|item| item.value()))
    });
    let observed = group.clone();
    let total = with_world_scope(&mut app.world, || Computed::new(move || observed.total()));
    assert_eq!(missing.get(), None);
    assert_eq!(total.get(), 0);
    map.reserve(4).unwrap();
    assert_eq!(map.capacity(), 4);
    assert_eq!(
        tracked_allocations(|| {
            assert!(map.try_insert(3, Item { value: 3 }).is_ok());
            assert_eq!(missing.get(), Some(3));
            assert_eq!(total.get(), 3);
            map.get(&3).unwrap().increment();
            assert_eq!(missing.get(), Some(4));
            assert_eq!(total.get(), 4);
        }),
        0
    );
    let old_id = map.entries().next().unwrap().0;
    let old = map.get(&3).unwrap();
    group.reset();
    assert_eq!(map.get(&3).unwrap().value(), 3);
    assert_eq!(map.get(&8).unwrap().value(), 8);
    assert_eq!(missing.get(), Some(3));
    assert_eq!(total.get(), 11);
    assert!(map.get_by_id(old_id).is_none());
    assert!(!super::super::ModelHandle::callback_ready(&old));
    map.retain(|key, _| *key == 8).unwrap();
    assert_eq!(map.len(), 1);
    assert!(map.get(&3).is_none());
    assert_eq!(total.get(), 8);
}

#[test]
fn key_index_survives_partial_retention_and_capacity_errors_preserve_inputs() {
    let mut map = ModelMap::from_entries((0..4).map(|key| (key, Item { value: key })))
        .ok()
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        map.retain(|key, _| {
            assert!(*key != 2, "predicate");
            *key != 0
        });
    }));
    assert!(result.is_err());
    assert!(map.iter().map(|(key, _)| *key).eq([1, 2, 3]));
    for (key, item) in map.iter() {
        assert_eq!(*key, item.value());
    }
    let mut map = ModelMap::bounded(1);
    assert!(map.try_insert(1, Item { value: 1 }).is_ok());
    assert_eq!(map.reserve(1), Err(CollectionError::Limit));
    let (error, key, item) = map.try_insert(2, Item { value: 2 }).err().unwrap();
    assert_eq!((error, key, item.value), (CollectionError::Capacity, 2, 2));
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(&1).unwrap().value(), 1);
}

#[test]
fn borrowed_key_visits_reject_same_root_mutation_without_poisoning() {
    let mut app = App::headless(32, 32);
    let map = app
        .add_model(
            ModelMap::from_entries([(1, Item { value: 1 })])
                .ok()
                .unwrap(),
        )
        .finish();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        map.for_each(|_, member| member.increment());
    }));
    assert!(result.is_err());
    assert_eq!(map.get(&1).unwrap().value(), 1);
    map.get(&1).unwrap().increment();
    assert_eq!(map.get(&1).unwrap().value(), 2);
}
