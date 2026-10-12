use mirui::{
    app::App,
    core::{
        model::{EffectRegistrationError, ModelItemEffect, ModelItemId, ModelList, ModelMap},
        reactive::Computed,
    },
    model,
};
use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

struct Sound {
    text: String,
    drops: Rc<Cell<usize>>,
}

impl Drop for Sound {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

fn sound(drops: &Rc<Cell<usize>>, text: &str) -> Sound {
    Sound {
        text: text.into(),
        drops: drops.clone(),
    }
}

#[model]
struct Tone {
    pending: Option<Sound>,
}

#[model]
impl Tone {
    fn emit(&mut self, sound: Sound) {
        self.pending = Some(sound);
    }

    #[effects]
    fn take_sound(&mut self) -> Option<Sound> {
        self.pending.take()
    }
}

#[model]
struct Item {
    key: u8,
    #[observe]
    count: usize,
    pending: [Option<Sound>; 2],
    child: model!(owned Tone),
}

#[model]
impl Item {
    fn new(key: u8) -> Self {
        Self {
            key,
            count: 0,
            pending: [None, None],
            child: Tone { pending: None }.into(),
        }
    }

    fn emit(&mut self, sounds: [Option<Sound>; 2]) {
        self.count += sounds.iter().flatten().count();
        self.pending = sounds;
    }

    fn emit_child(&mut self, sound: Sound) {
        self.child.emit(sound);
    }

    #[effects]
    fn take_sounds(&mut self) -> [Option<Sound>; 2] {
        core::mem::take(&mut self.pending)
    }
}

#[model]
struct Group {
    items: model!(owned ModelList<Item>),
    other: model!(owned ModelList<Item>),
    keyed: model!(owned ModelMap<u8, Item>),
}

#[model]
impl Group {
    fn new() -> Self {
        Self {
            items: ModelList::from([Item::new(1), Item::new(2)]).into(),
            other: ModelList::from([Item::new(9)]).into(),
            keyed: ModelMap::from_entries([(2, Item::new(2)), (1, Item::new(1))])
                .ok()
                .unwrap()
                .into(),
        }
    }

    #[observe]
    fn total(&self) -> usize {
        self.items.iter().map(|item| item.count()).sum()
    }

    fn emit_and_reorder(&mut self, sounds: [Sound; 3]) {
        let [first, second, child] = sounds;
        self.items.get_mut(0).unwrap().emit([Some(first), None]);
        let item = self.items.get_mut(1).unwrap();
        item.emit([None, Some(second)]);
        item.emit_child(child);
        self.items.rotate_left(1).unwrap();
    }

    fn emit_and_reject(&mut self, sound: Sound) -> Result<(), ()> {
        self.items.get_mut(0).unwrap().emit([Some(sound), None]);
        Err(())
    }

    fn emit_and_remove(&mut self, sound: Sound) {
        let id = self.items.entries().next().unwrap().0;
        self.items.get_mut(0).unwrap().emit([Some(sound), None]);
        self.items.remove(id);
    }

    fn swap_lists(&mut self) {
        core::mem::swap(&mut self.items, &mut self.other);
    }

    fn reset_list(&mut self) {
        self.items = ModelList::from([Item::new(7)]).into();
    }

    fn emit_map(&mut self, first: Sound, second: Sound) {
        self.keyed.get_mut(&2).unwrap().emit([Some(second), None]);
        self.keyed.get_mut(&1).unwrap().emit([Some(first), None]);
    }
}

#[test]
fn members_and_collection_borrow_the_same_event_after_all_observations_publish() {
    let mut app = App::headless(1, 1);
    let group = app.add_model(Group::new()).finish();
    let items = group.items();
    let identities: Vec<_> = items.entries().map(|(id, _)| id).collect();
    let root = app.spawn_root().id();
    let observed = group.clone();
    let total = app.compose(root, |_| Computed::new(move || observed.total()));
    assert_eq!(total.get(), 0);
    let trace = Rc::new(RefCell::new(Vec::with_capacity(8)));
    for (id, item) in items.entries() {
        let trace = trace.clone();
        let total = total.clone();
        item.on_effect(move |sound: &Sound| {
            assert_eq!(total.get(), 2);
            trace
                .borrow_mut()
                .push((id, false, sound as *const _ as usize));
        })
        .unwrap();
    }
    let delivered = trace.clone();
    items
        .on_effect(move |effect: &ModelItemEffect<Sound>| {
            assert_eq!(total.get(), 2);
            assert_eq!(
                effect.event.text,
                if effect.id == identities[1] { "b" } else { "a" }
            );
            delivered
                .borrow_mut()
                .push((effect.id, true, &effect.event as *const _ as usize));
        })
        .unwrap();
    let children = Rc::new(Cell::new(0));
    let received = children.clone();
    items
        .get(1)
        .unwrap()
        .child()
        .on_effect(move |sound: &Sound| {
            assert_eq!(sound.text, "child");
            received.set(received.get() + 1);
        })
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    let payloads = [
        sound(&drops, "a"),
        sound(&drops, "b"),
        sound(&drops, "child"),
    ];
    assert_eq!(
        tracking_allocator::tracked_allocations(|| group.emit_and_reorder(payloads)),
        0
    );
    let trace = trace.borrow();
    assert_eq!(trace.len(), 4);
    for pair in trace.chunks_exact(2) {
        assert_eq!(pair[0].0, pair[1].0);
        assert!(!pair[0].1 && pair[1].1);
        assert_eq!(pair[0].2, pair[1].2);
    }
    assert_eq!(trace[0].0, items.entries().next().unwrap().0);
    assert_eq!(children.get(), 1);
    assert_eq!(drops.get(), 3);
}

#[test]
fn collection_routes_survive_member_retirement_and_remain_on_their_field() {
    let mut app = App::headless(1, 1);
    let group = app.add_model(Group::new()).finish();
    let items = group.items();
    let other = group.other();
    let delivered = Rc::new(Cell::new(0));
    let count = delivered.clone();
    items
        .on_effect(move |_: &ModelItemEffect<Sound>| count.set(count.get() + 1))
        .unwrap();
    other
        .on_effect(|_: &ModelItemEffect<Sound>| panic!("handler followed swapped data"))
        .unwrap();
    assert_eq!(
        items.on_effect(|_: &ModelItemEffect<Sound>| {}),
        Err(EffectRegistrationError::AlreadyRegistered)
    );
    let old = items.get(0).unwrap();
    old.on_effect(|_: &Sound| panic!("retired member handler was reused"))
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    assert_eq!(group.emit_and_reject(sound(&drops, "rejected")), Err(()));
    assert_eq!(delivered.get(), 0);
    group.emit_and_remove(sound(&drops, "removed"));
    assert_eq!(drops.get(), 2);
    assert_eq!(delivered.get(), 0);
    items.try_push(Item::new(3)).ok().unwrap();
    items
        .get(1)
        .unwrap()
        .emit([Some(sound(&drops, "reused")), None]);
    assert_eq!(delivered.get(), 1);
    assert!(catch_unwind(AssertUnwindSafe(|| old.on_effect(|_: &Sound| {}))).is_err());
    group.swap_lists();
    items
        .get(0)
        .unwrap()
        .emit([Some(sound(&drops, "swapped")), None]);
    group.reset_list();
    items
        .get(0)
        .unwrap()
        .emit([Some(sound(&drops, "reset")), None]);
    items.clear().unwrap();
    items.try_push(Item::new(4)).ok().unwrap();
    items
        .get(0)
        .unwrap()
        .emit([Some(sound(&drops, "cleared")), None]);
    assert_eq!(delivered.get(), 4);
    drop(app);
    assert!(
        catch_unwind(AssertUnwindSafe(
            || items.on_effect(|_: &ModelItemEffect<Sound>| {})
        ))
        .is_err()
    );
    assert_eq!(Rc::strong_count(&delivered), 1);
}

#[test]
fn load_retires_member_consumers_and_reconcile_keeps_matching_consumers() {
    let mut app = App::headless(1, 1);
    let items = app.add_model(ModelList::from([Item::new(1)])).finish();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    items
        .on_effect(move |_: &ModelItemEffect<Sound>| count.set(count.get() + 10))
        .unwrap();
    let (id, item) = items.entries().next().unwrap();
    let count = calls.clone();
    item.on_effect(move |_: &Sound| count.set(count.get() + 1))
        .unwrap();
    items
        .reconcile(
            [Item::new(1)],
            |item| item.key,
            |old, new| old.count = new.count,
        )
        .unwrap();
    assert_eq!(items.entries().next().unwrap().0, id);
    let drops = Rc::new(Cell::new(0));
    item.emit([None, Some(sound(&drops, "retained"))]);
    assert_eq!(calls.get(), 11);
    items.load([Item::new(1)]).unwrap();
    assert_ne!(items.entries().next().unwrap().0, id);
    items
        .get(0)
        .unwrap()
        .emit([Some(sound(&drops, "loaded")), None]);
    assert_eq!(calls.get(), 21);
}

#[test]
fn map_aggregates_by_key_order_with_the_same_member_identities() {
    let mut app = App::headless(1, 1);
    let group = app.add_model(Group::new()).finish();
    let map = group.keyed();
    let ids: Vec<_> = map.entries().map(|(id, _)| id).collect();
    let actual = Rc::new(RefCell::new(Vec::<ModelItemId>::with_capacity(4)));
    let received = actual.clone();
    map.on_effect(move |effect: &ModelItemEffect<Sound>| received.borrow_mut().push(effect.id))
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    let first = sound(&drops, "first");
    let second = sound(&drops, "second");
    assert_eq!(
        tracking_allocator::tracked_allocations(|| group.emit_map(first, second)),
        0
    );
    assert_eq!(*actual.borrow(), ids);
    map.reconcile([(1, Item::new(1)), (2, Item::new(2))], |old, new| {
        old.count = new.count
    })
    .unwrap();
    map.get(&1)
        .unwrap()
        .emit([Some(sound(&drops, "retained")), None]);
    assert_eq!(actual.borrow()[2], ids[0]);
    map.load([(1, Item::new(1))]).unwrap();
    map.get(&1)
        .unwrap()
        .emit([Some(sound(&drops, "loaded")), None]);
    assert_ne!(actual.borrow()[3], ids[0]);
    assert_eq!(drops.get(), 4);
}

#[test]
fn panic_discards_remaining_events_and_restores_both_routes_without_replay() {
    for panic_in_member in [false, true] {
        let mut app = App::headless(1, 1);
        let items = app.add_model(ModelList::from([Item::new(1)])).finish();
        let item = items.get(0).unwrap();
        let first = Rc::new(Cell::new(true));
        let fail = first.clone();
        item.on_effect(move |_: &Sound| {
            if panic_in_member && fail.replace(false) {
                panic!("member failed");
            }
        })
        .unwrap();
        let aggregated = Rc::new(Cell::new(0));
        let count = aggregated.clone();
        items
            .on_effect(move |_: &ModelItemEffect<Sound>| {
                if !panic_in_member && first.replace(false) {
                    panic!("collection failed");
                }
                count.set(count.get() + 1);
            })
            .unwrap();
        let drops = Rc::new(Cell::new(0));
        let batch = [
            Some(sound(&drops, "first")),
            Some(sound(&drops, "remaining")),
        ];
        assert!(catch_unwind(AssertUnwindSafe(|| item.emit(batch))).is_err());
        assert_eq!(drops.get(), 2);
        item.emit([None, None]);
        assert_eq!(aggregated.get(), 0);
        item.emit([Some(sound(&drops, "new")), None]);
        assert_eq!(aggregated.get(), 1);
        assert_eq!(drops.get(), 3);
    }
}

#[test]
fn aggregate_callbacks_cannot_start_a_model_write() {
    let mut app = App::headless(1, 1);
    let items = app.add_model(ModelList::from([Item::new(1)])).finish();
    let item = items.get(0).unwrap();
    let feedback = item.clone();
    items
        .on_effect(move |_: &ModelItemEffect<Sound>| feedback.emit([None, None]))
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    assert!(
        catch_unwind(AssertUnwindSafe(
            || item.emit([Some(sound(&drops, "readonly")), None])
        ))
        .is_err()
    );
    assert_eq!(item.count(), 1);
    assert_eq!(drops.get(), 1);
}

#[test]
fn optional_effects_drop_without_a_consumer_and_install_one_shared_handler() {
    let mut app = App::headless(1, 1);
    let tones = app
        .add_model(ModelList::from([Tone { pending: None }]))
        .finish();
    let other = app
        .add_model(ModelList::from([Tone { pending: None }]))
        .finish();
    let drops = Rc::new(Cell::new(0));
    tones.get(0).unwrap().emit(sound(&drops, "unhandled"));
    assert_eq!(drops.get(), 1);
    let calls = Rc::new(Cell::new(0));
    let received = calls.clone();
    assert_eq!(
        tracking_allocator::tracked_allocations(|| {
            tones
                .on_effect(move |effect: &ModelItemEffect<Sound>| {
                    assert_eq!(effect.event.text, "handled");
                    received.set(received.get() + 1);
                })
                .unwrap();
        }),
        1
    );
    other.get(0).unwrap().emit(sound(&drops, "other"));
    let next = sound(&drops, "handled");
    assert_eq!(
        tracking_allocator::tracked_allocations(|| tones.get(0).unwrap().emit(next)),
        0
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(drops.get(), 3);
}
