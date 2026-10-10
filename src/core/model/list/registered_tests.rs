use super::*;
use crate::{
    app::App,
    core::{
        model::{ModelHandle, SharedValue},
        reactive::{Computed, with_world_scope},
    },
    test_support::tracked_allocations,
};
use alloc::rc::Rc;
use core::cell::Cell;

#[derive(Clone, Copy)]
struct Notice(u16);

#[crate::model]
struct Item {
    #[observe]
    value: u16,
    notice: Option<Notice>,
}

#[crate::model]
impl Item {
    fn new(value: u16) -> Self {
        Self {
            value,
            notice: None,
        }
    }
    fn increment(&mut self) {
        self.value += 1;
        self.notice = Some(Notice(self.value));
    }
    #[effects]
    fn take_notice(&mut self) -> [Option<Notice>; 1] {
        [self.notice.take()]
    }
}

#[crate::model]
struct Group {
    items: model!(owned ModelList<Item>),
}

#[crate::model]
impl Group {
    fn new() -> Self {
        let mut items = ModelList::bounded(4);
        assert!(items.try_push(Item::new(1)).is_ok());
        assert!(items.try_push(Item::new(2)).is_ok());
        Self {
            items: items.into(),
        }
    }
    #[observe]
    fn total(&self) -> u16 {
        self.items.iter().map(|item| item.value()).sum()
    }
    fn change_all(&mut self) {
        for item in self.items.iter_mut() {
            item.increment();
        }
    }
    fn replace_all(&mut self) {
        self.items = [Item::new(8), Item::new(9)].into();
    }
    fn remove_reject(&mut self, id: ModelItemId) -> Result<(), ()> {
        self.items.remove(id);
        Err(())
    }
    fn load_reject(&mut self) -> Result<(), ()> {
        self.items.load([Item::new(6)]).unwrap();
        Err(())
    }
}

#[test]
fn load_joins_parent_transactions_and_retires_members_without_reviving_routes() {
    let mut app = App::headless(32, 32);
    let parent = app.add_model(Group::new());
    let list = parent.items();
    let watched = parent.clone();
    let total = with_world_scope(&mut app.world, || Computed::new(move || watched.total()));
    assert_eq!(total.get(), 3);
    let (id, old) = list.entries().next().unwrap();
    let notices = Rc::new(Cell::new(0));
    let count = notices.clone();
    app.on_effect(&old, move |_: Notice| count.set(count.get() + 1))
        .unwrap();
    list.load([Item::new(7), Item::new(8)]).unwrap();
    assert_eq!(total.get(), 15);
    assert_eq!(list.limit(), Some(4));
    assert_eq!(list.capacity(), 4);
    assert!(list.get_by_id(id).is_none());
    assert!(!ModelHandle::callback_ready(&old));
    assert_eq!(tracked_allocations(|| list.get(0).unwrap().increment()), 0);
    assert_eq!(notices.get(), 0);
    assert_eq!(total.get(), 16);
    let replaced = list.get(0).unwrap();
    assert_eq!(parent.load_reject(), Err(()));
    assert!(!ModelHandle::callback_ready(&replaced));
    assert_eq!(parent.total(), 6);
    assert_eq!(total.get(), 16);
    list.get(0).unwrap().increment();
    assert_eq!(total.get(), 7);
}

#[test]
fn load_rejects_leases_before_consuming_input_and_prepares_new_nested_shapes() {
    let mut app = App::headless(32, 32);
    let list = app.add_model(ModelList::from([NestedItem::new(1, 1)]));
    let held = list.iter();
    let reads = Cell::new(0);
    let input = [NestedItem::new(8, 3)]
        .into_iter()
        .inspect(|_| reads.set(reads.get() + 1));
    let error = list.load(input).unwrap_err();
    assert_eq!(error.error(), CollectionError::Iterating);
    assert_eq!(reads.get(), 0);
    drop(held);
    let old = list.get(0).unwrap();
    let old_child = old.children().get(0).unwrap();
    list.load(error.into_input()).unwrap();
    assert!(!ModelHandle::callback_ready(&old));
    assert!(!ModelHandle::callback_ready(&old_child));
    assert_eq!(list.get(0).unwrap().children().capacity(), 3);
    let input = NestedItem::new(10, 3);
    assert_eq!(
        tracked_allocations(|| {
            list.clear().unwrap();
            let id = list
                .try_push(input)
                .unwrap_or_else(|_| panic!("prepared shape"));
            list.get_by_id(id)
                .unwrap()
                .children()
                .get(0)
                .unwrap()
                .increment();
        }),
        0
    );
    assert_eq!(list.get(0).unwrap().children().get(0).unwrap().value(), 11);
}

#[test]
fn list_items_share_the_parent_transaction_and_distinct_observation_sources() {
    let mut app = App::headless(32, 32);
    let parent = app.add_model(Group::new());
    let list = parent.items();
    let counts = Rc::new(Cell::new(0));
    let count = counts.clone();
    let watched = list.clone();
    let len = with_world_scope(&mut app.world, || {
        Computed::new(move || {
            count.set(count.get() + 1);
            watched.len()
        })
    });
    let watched = parent.clone();
    let total = with_world_scope(&mut app.world, || Computed::new(move || watched.total()));
    assert_eq!(len.get(), 2);
    assert_eq!(total.get(), 3);
    let item = list.get(0).unwrap();
    let delivered = Rc::new(Cell::new(0));
    let count = delivered.clone();
    let watched = total.clone();
    app.on_effect(&item, move |notice: Notice| {
        count.set(notice.0);
        assert_eq!(watched.get(), 4);
    })
    .unwrap();
    assert_eq!(tracked_allocations(|| item.increment()), 0);
    assert_eq!(total.get(), 4);
    assert_eq!(delivered.get(), 2);
    assert_eq!(len.get(), 2);
    assert_eq!(counts.get(), 1);
    assert_eq!(
        tracked_allocations(|| {
            assert!(list.try_push(Item::new(3)).is_ok());
        }),
        0
    );
    assert_eq!(total.get(), 7);
    assert_eq!(len.get(), 3);
    assert_eq!(counts.get(), 2);
    let before = list.entries().next().unwrap().0;
    assert_eq!(tracked_allocations(|| list.rotate_left(1).unwrap()), 0);
    assert_eq!(list.get_by_id(before).unwrap().value(), 2);
    assert_eq!(list.get(2).unwrap().value(), 2);
    assert_eq!(total.get(), 7);
}

#[test]
fn retirement_invalidates_old_handles_and_reuses_only_prepared_capacity() {
    let mut app = App::headless(32, 32);
    let parent = app.add_model(Group::new());
    let list = parent.items();
    let (id, old) = list.entries().next().unwrap();
    let delivered = Rc::new(Cell::new(0));
    let count = delivered.clone();
    app.on_effect(&old, move |_: Notice| count.set(count.get() + 1))
        .unwrap();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(list.remove(id).unwrap().unwrap().value, 1);
            assert!(list.get_by_id(id).is_none());
            assert!(old.access().upgrade().is_none());
            let new_id = list
                .try_push(Item::new(7))
                .unwrap_or_else(|_| panic!("prepared slot"));
            assert_ne!(id, new_id);
            list.get_by_id(new_id).unwrap().increment();
        }),
        0
    );
    assert_eq!(delivered.get(), 0);
    assert_eq!(parent.total(), 10);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| old.increment())).is_err());
    parent.change_all();
    assert_eq!(parent.total(), 12);
}

#[test]
fn structural_lease_allows_item_updates_but_rejects_ancestor_replacement_before_execution() {
    let mut app = App::headless(32, 32);
    let parent = app.add_model(Group::new());
    let list = parent.items();
    assert_eq!(
        tracked_allocations(|| {
            for item in list.iter() {
                item.increment();
                assert_eq!(list.clear(), Err(CollectionError::Iterating));
                assert_eq!(
                    list.try_push(Item::new(3)).err().unwrap().0,
                    CollectionError::Iterating
                );
            }
        }),
        0
    );
    assert_eq!(parent.total(), 5);
    let lease = list.entries();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parent.replace_all())).is_err()
    );
    assert_eq!(parent.total(), 5);
    drop(lease);
    parent.change_all();
    assert_eq!(parent.total(), 7);
    parent.replace_all();
    assert_eq!(parent.total(), 17);
}

#[test]
fn rejected_outer_update_still_retires_removed_identity_without_publishing() {
    let mut app = App::headless(32, 32);
    let parent = app.add_model(Group::new());
    let watched = parent.clone();
    let total = with_world_scope(&mut app.world, || Computed::new(move || watched.total()));
    assert_eq!(total.get(), 3);
    let (id, old) = parent.items().entries().next().unwrap();
    assert_eq!(parent.remove_reject(id), Err(()));
    assert!(old.access().upgrade().is_none());
    assert!(parent.items().get_by_id(id).is_none());
    assert_eq!(parent.total(), 2);
    assert_eq!(total.get(), 3);
    parent.change_all();
    assert_eq!(total.get(), 3);
}

#[test]
fn whole_list_replacement_retires_members_but_preserves_field_facade() {
    let mut app = App::headless(32, 32);
    let parent = app.add_model(Group::new());
    let list = parent.items();
    let (id, old) = list.entries().next().unwrap();
    parent.replace_all();
    assert!(old.access().upgrade().is_none());
    assert!(list.get_by_id(id).is_none());
    assert_eq!(list.get(0).unwrap().value(), 8);
    list.get(0).unwrap().increment();
    assert_eq!(parent.total(), 18);
    drop(app);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| list.len())).is_err());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || list.assert_registered_owner()
        ))
        .is_err()
    );
}

#[test]
fn shrinking_then_reserving_cannot_reissue_retired_member_ids() {
    let mut app = App::headless(32, 32);
    let parent = app.add_model(Group::new());
    let list = parent.items();
    let id = list
        .try_push(Item::new(3))
        .unwrap_or_else(|_| panic!("capacity"));
    let old = list.get_by_id(id).unwrap();
    parent.replace_all();
    assert_eq!(list.capacity(), 2);
    list.reserve(1).unwrap();
    let replacement = list
        .try_push(Item::new(4))
        .unwrap_or_else(|_| panic!("capacity"));
    assert_eq!(replacement.slot, id.slot);
    assert_ne!(replacement, id);
    assert!(list.get_by_id(id).is_none());
    assert!(old.access().upgrade().is_none());
    assert_eq!(parent.total(), 21);
}

#[crate::model]
struct NestedItem {
    children: model!(owned ModelList<Item>),
}
#[crate::model]
impl NestedItem {
    fn new(value: u16, capacity: usize) -> Self {
        let mut children = ModelList::with_capacity(capacity);
        assert!(children.try_push(Item::new(value)).is_ok());
        Self {
            children: children.into(),
        }
    }
}

#[test]
fn nested_shapes_reuse_prepared_nodes_and_reject_unprepared_growth() {
    let mut app = App::headless(32, 32);
    let list = app.add_model(ModelList::from([
        NestedItem::new(1, 1),
        NestedItem::new(2, 2),
    ]));
    let (id, old) = list.entries().next_back().unwrap();
    let old_child = old.children().get(0).unwrap();
    list.clear().unwrap();
    let incoming = NestedItem::new(8, 2);
    assert_eq!(
        tracked_allocations(|| {
            let new_id = list
                .try_push(incoming)
                .unwrap_or_else(|_| panic!("prepared shape"));
            assert_eq!(new_id.slot, id.slot);
            assert_ne!(new_id, id);
            list.get_by_id(new_id)
                .unwrap()
                .children()
                .get(0)
                .unwrap()
                .increment();
        }),
        0
    );
    assert!(old.access().upgrade().is_none());
    assert!(old_child.access().upgrade().is_none());
    assert_eq!(list.get(0).unwrap().children().get(0).unwrap().value(), 9);
    let input = NestedItem::new(10, 3);
    let (error, input) = list.try_push(input).err().unwrap();
    assert_eq!(error, CollectionError::UnpreparedShape);
    assert_eq!(input.children.capacity(), 3);
    let id = list.entries().next().unwrap().0;
    let replacement = NestedItem::new(11, 2);
    assert_eq!(
        tracked_allocations(|| {
            assert!(list.replace(id, replacement).is_ok());
        }),
        0
    );
    assert_eq!(
        list.get_by_id(id)
            .unwrap()
            .children()
            .get(0)
            .unwrap()
            .value(),
        11
    );
}

#[crate::model]
struct Pair {
    left: model!(owned ModelList<Item>),
    right: model!(owned ModelList<Item>),
}
#[crate::model]
impl Pair {
    fn swap_lists(&mut self) {
        core::mem::swap(&mut self.left, &mut self.right);
    }
}

#[test]
fn swapping_fields_keeps_collection_ownership_and_retires_old_member_routes() {
    let mut app = App::headless(32, 32);
    let parent = app.add_model(Pair {
        left: [Item::new(1)].into(),
        right: [Item::new(2), Item::new(3)].into(),
    });
    let left = parent.left();
    let right = parent.right();
    let (old_id, old) = left.entries().next().unwrap();
    parent.swap_lists();
    assert_eq!(left.len(), 2);
    assert_eq!(left.get(0).unwrap().value(), 2);
    assert_eq!(right.len(), 1);
    assert_eq!(right.get(0).unwrap().value(), 1);
    assert!(old.access().upgrade().is_none());
    assert!(left.get_by_id(old_id).is_none());
    assert!(right.get_by_id(old_id).is_none());
    left.get(1).unwrap().increment();
    assert_eq!(left.get(1).unwrap().value(), 4);
    assert_eq!(right.get(0).unwrap().value(), 1);
}
