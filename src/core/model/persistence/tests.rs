use crate::{
    core::model::{self, Model, ModelList},
    ecs::World,
    test_support::tracked_allocations,
};
use alloc::rc::Rc;
use core::cell::Cell;

#[crate::model(persistence)]
struct State {
    #[persist]
    #[observe]
    value: u32,
    temporary: bool,
    sound: Option<u8>,
}

#[crate::model]
impl State {
    fn new() -> Self {
        Self {
            value: 0,
            temporary: false,
            sound: None,
        }
    }
    fn increment(&mut self) {
        self.value += 1;
    }
    fn temporary(&mut self) {
        self.temporary = !self.temporary;
    }
    fn fail(&mut self) -> Result<(), ()> {
        self.value += 1;
        Err(())
    }
    fn emit(&mut self) {
        self.sound = Some(1);
    }
    fn unwind(&mut self, fail: bool) {
        self.value += 1;
        assert!(!fail, "model update failure");
    }
    #[effects]
    fn take_sound(&mut self) -> Option<u8> {
        self.sound.take()
    }
}

#[crate::model]
struct Plain {
    value: u32,
}
#[crate::model]
impl Plain {}

#[crate::model(persistence)]
struct Parent {
    #[persist]
    child: model!(owned State),
    rows: model!(owned ModelList<State>),
}
#[crate::model]
impl Parent {
    fn change_both(&mut self) {
        self.child.increment();
        for row in self.rows.iter_mut() {
            row.increment();
        }
    }
}

#[test]
fn unmarked_models_use_zero_sized_state() {
    assert_eq!(core::mem::size_of::<<Plain as Model>::Persistence>(), 0);
    assert!(core::mem::size_of::<<State as Model>::Persistence>() <= 16);
    let plain = Plain { value: 1 };
    assert_eq!(plain.value, 1);
}

#[test]
fn root_mutations_and_errors_mark_one_candidate_without_allocation() {
    let mut world = World::new();
    let (registration, handle) = model::register(&mut world, State::new());
    let candidate = &registration.root.persistence;
    candidate.confirm(candidate.stamp());
    let before = candidate.stamp();
    assert_eq!(handle.value(), 0);
    assert!(!candidate.pending());
    assert_eq!(tracked_allocations(|| handle.increment()), 0);
    assert!(candidate.pending());
    assert_eq!(candidate.stamp(), before + 1);
    candidate.confirm(candidate.stamp());
    assert_eq!(
        tracked_allocations(|| assert_eq!(handle.fail(), Err(()))),
        0
    );
    assert!(candidate.pending());
    assert_eq!(handle.value(), 2);
    candidate.confirm(candidate.stamp());
    handle.temporary();
    assert!(candidate.pending());
}

#[test]
fn child_and_collection_mutations_share_the_outer_root_candidate() {
    let mut world = World::new();
    let (registration, handle) = model::register(
        &mut world,
        Parent {
            child: State::new().into(),
            rows: ModelList::from([State::new()]).into(),
        },
    );
    let candidate = &registration.root.persistence;
    candidate.confirm(candidate.stamp());
    handle.child().increment();
    assert_eq!(candidate.stamp(), 1);
    candidate.confirm(candidate.stamp());
    handle.rows().get(0).unwrap().increment();
    assert!(candidate.pending());
    assert_eq!(candidate.stamp(), 2);
    candidate.confirm(candidate.stamp());
    assert_eq!(tracked_allocations(|| handle.change_both()), 0);
    assert_eq!(candidate.stamp(), 3);
    assert!(candidate.pending());
    candidate.confirm(candidate.stamp());
    let id = handle.rows().entries().next().unwrap().0;
    handle.rows().remove(id).unwrap();
    assert!(candidate.pending());
}

#[test]
fn candidate_is_visible_before_effect_delivery() {
    let mut world = World::new();
    let (registration, handle) = model::register(&mut world, State::new());
    let root = Rc::downgrade(&registration.root);
    let delivered = Rc::new(Cell::new(false));
    let output = delivered.clone();
    handle
        .on_effect(move |_: &u8| {
            let root = root.upgrade().unwrap();
            assert!(root.persistence.pending());
            output.set(true);
        })
        .unwrap();
    registration.root.persistence.confirm(0);
    handle.emit();
    assert!(delivered.get());
}

#[test]
fn mutation_panics_preserve_root_poisoning_and_retirement() {
    let mut world = World::new();
    let (registration, handle) = model::register(&mut world, State::new());
    let weak = Rc::downgrade(&registration.root);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.unwind(true))).is_err()
    );
    assert!(registration.root.poisoned.get());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.value())).is_err());
    drop(registration);
    assert!(weak.upgrade().is_none());
}
