use super::*;
use crate::test_support::tracked_allocations;

#[crate::model]
struct Item {
    #[observe]
    value: u16,
}

#[crate::model]
impl Item {
    fn increment(&mut self) {
        self.value += 1;
    }
}

fn item(value: u16) -> Item {
    Item { value }
}

fn push(list: &mut ModelList<Item>, value: u16) -> ModelItemId {
    list.try_push(item(value))
        .unwrap_or_else(|_| panic!("prepared insertion"))
}

fn assert_values(list: &ModelList<Item>, expected: &[u16]) {
    assert_eq!(list.len(), expected.len());
    for ((id, value), expected) in list.entries().zip(expected) {
        assert_eq!(value.value(), *expected);
        assert!(core::ptr::eq(value, list.get_by_id(id).unwrap()));
    }
}

#[test]
fn prepared_mutations_keep_identity_and_never_grow() {
    let mut list = ModelList::bounded(4);
    assert_eq!(
        tracked_allocations(|| {
            let a = push(&mut list, 1);
            let b = push(&mut list, 2);
            let c = push(&mut list, 3);
            list.swap(0, 2).unwrap();
            assert_values(&list, &[3, 2, 1]);
            list.move_to(a, 0).unwrap();
            assert_values(&list, &[1, 3, 2]);
            list.rotate_left(1).unwrap();
            assert_values(&list, &[3, 2, 1]);
            assert_eq!(list.replace(b, item(20)).ok().unwrap().value, 2);
            for value in list.iter_mut() {
                value.increment();
            }
            assert_values(&list, &[4, 21, 2]);
            assert_eq!(list.get_by_id(c).unwrap().value(), 4);
            assert_eq!(list.get_by_id(b).unwrap().value(), 21);
            assert_eq!(list.remove(a).unwrap().value, 2);
            let replacement = push(&mut list, 8);
            assert_eq!(replacement.slot, a.slot);
            assert_ne!(replacement, a);
            assert!(list.get_by_id(a).is_none());
            list.retain(|value| value.value() < 10);
            assert_values(&list, &[4, 8]);
            list.clear();
            assert!(list.is_empty());
            assert!(list.get_by_id(c).is_none());
            assert!(list.get_by_id(replacement).is_none());
            let reused = push(&mut list, 9);
            assert_ne!(reused, replacement);
            assert_eq!(list.capacity(), 4);
        }),
        0
    );
}

#[test]
fn capacity_failures_preserve_input_and_existing_data() {
    let mut list = ModelList::<Item>::new();
    assert_eq!(list.capacity(), 0);
    let (error, value) = list.try_push(item(7)).err().unwrap();
    assert_eq!(error, CollectionError::Capacity);
    assert_eq!(value.value, 7);
    list.reserve(2).unwrap();
    let a = push(&mut list, 1);
    list.reserve(1).unwrap();
    assert_eq!(list.capacity(), 2);
    push(&mut list, 2);
    let (error, value) = list.try_push(item(9)).err().unwrap();
    assert_eq!(error, CollectionError::Capacity);
    assert_eq!(value.value, 9);
    assert_values(&list, &[1, 2]);
    list.reserve(1).unwrap();
    assert_eq!(list.capacity(), 3);
    assert_eq!(list.get_by_id(a).unwrap().value(), 1);
    let mut bounded = ModelList::<Item>::bounded(2);
    push(&mut bounded, 4);
    assert_eq!(bounded.reserve(2), Err(CollectionError::Limit));
    assert_eq!(bounded.reserve(usize::MAX), Err(CollectionError::Limit));
    assert_values(&bounded, &[4]);
    assert_eq!(bounded.limit(), Some(2));
    assert_eq!(list.limit(), None);
}

#[test]
fn loading_recovers_non_clone_input_in_order_without_clearing_old_data() {
    let mut list = ModelList::bounded(2);
    let old = push(&mut list, 9);
    let read = core::cell::Cell::new(0);
    let input = (0..5).map(|value| {
        read.set(read.get() + 1);
        item(value)
    });
    let error = list.load(input).unwrap_err();
    assert_eq!(error.error(), CollectionError::Limit);
    assert_eq!(read.get(), 3);
    assert_eq!(list.get_by_id(old).unwrap().value(), 9);
    assert_values(&list, &[9]);
    assert_eq!(
        error
            .into_input()
            .map(|item| item.value)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
    list.load([item(1), item(2)]).unwrap();
    assert!(list.get_by_id(old).is_none());
    assert_eq!(list.limit(), Some(2));
    assert_eq!(list.capacity(), 2);
    assert_values(&list, &[1, 2]);
    list.load([]).unwrap();
    assert!(list.is_empty());
    assert_eq!(list.capacity(), 2);
}

#[test]
fn loading_preflights_exhausted_slots_without_losing_input() {
    let mut list = ModelList::bounded(1);
    list.slots[0].generation = u64::MAX - 1;
    let old = push(&mut list, 9);
    let error = list.load([item(1)]).unwrap_err();
    assert_eq!(error.error(), CollectionError::IdentityExhausted);
    assert_eq!(error.into_input().next().unwrap().value, 1);
    assert_eq!(list.get_by_id(old).unwrap().value(), 9);
    assert_values(&list, &[9]);
}

#[test]
fn foreign_and_exhausted_identities_cannot_alias_live_members() {
    let mut a = ModelList::with_capacity(1);
    let id = push(&mut a, 1);
    let mut b = ModelList::with_capacity(1);
    let other = push(&mut b, 2);
    assert!(a.get_by_id(other).is_none());
    assert!(b.remove(id).is_none());
    assert_eq!(
        b.replace(id, item(3)).err().unwrap().0,
        CollectionError::Missing
    );
    assert_values(&b, &[2]);
    a.remove(id).unwrap();
    a.slots[id.slot].generation = u64::MAX - 1;
    let last = push(&mut a, 3);
    assert_eq!(last.generation, u64::MAX);
    a.remove(last).unwrap();
    let (error, input) = a.try_push(item(4)).err().unwrap();
    assert_eq!(error, CollectionError::IdentityExhausted);
    assert_eq!(input.value, 4);
    assert!(a.get_by_id(last).is_none());
    assert!(a.is_empty());
    a.reserve(2).unwrap();
    let next = push(&mut a, 5);
    assert_ne!(next.slot, id.slot);
    assert!(a.get_by_id(id).is_none());
}

#[test]
fn mutable_iteration_follows_logical_order_after_every_move() {
    let mut list: ModelList<Item> = [item(1), item(2), item(3), item(4)].into();
    let ids: Vec<_> = list.entries().map(|(id, _)| id).collect();
    for from in 0..list.len() {
        for to in 0..list.len() {
            let id = list.entries().nth(from).unwrap().0;
            list.move_to(id, to).unwrap();
            let expected: Vec<_> = list.iter().map(|value| value.value()).collect();
            let mut actual = list.iter_mut();
            assert_eq!(actual.len(), 4);
            assert_eq!(actual.next_back().unwrap().value(), expected[3]);
            for (value, expected) in actual.zip(&expected[..3]) {
                assert_eq!(value.value(), *expected);
            }
            for (index, id) in ids.iter().enumerate() {
                assert_eq!(list.get_by_id(*id).unwrap().value(), index as u16 + 1);
            }
        }
    }
}

#[test]
fn imports_move_non_clone_data_and_ignore_input_spare_capacity() {
    let mut items = Vec::with_capacity(32);
    items.push(item(1));
    items.push(item(2));
    let mut list: ModelList<_> = items.into();
    assert_eq!(list.capacity(), 2);
    assert_values(&list, &[1, 2]);
    list.get_mut(0).unwrap().increment();
    let id = list.entries().next().unwrap().0;
    list.get_by_id_mut(id).unwrap().increment();
    assert_values(&list, &[3, 2]);
    let unknown = (0..7).filter(|_| true).map(item).collect::<ModelList<_>>();
    assert_values(&unknown, &[0, 1, 2, 3, 4, 5, 6]);
}

#[test]
fn invalid_positions_and_panicking_retention_preserve_index_invariants() {
    let mut list: ModelList<Item> = [item(1), item(2), item(3)].into();
    let id = list.entries().next().unwrap().0;
    assert_eq!(
        list.try_insert(4, item(4)).err().unwrap().0,
        CollectionError::Index
    );
    assert_eq!(list.swap(0, 3), Err(CollectionError::Index));
    assert_eq!(list.move_to(id, 3), Err(CollectionError::Index));
    assert_eq!(list.rotate_left(4), Err(CollectionError::Index));
    assert_values(&list, &[1, 2, 3]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        list.retain(|value| match value.value() {
            1 => false,
            2 => panic!("predicate"),
            _ => true,
        });
    }));
    assert!(result.is_err());
    assert_values(&list, &[2, 3]);
    assert!(list.get_by_id(id).is_none());
    list.clear();
    assert_eq!(list.rotate_left(usize::MAX), Ok(()));
    assert_eq!(list.swap(0, 0), Err(CollectionError::Index));
}
