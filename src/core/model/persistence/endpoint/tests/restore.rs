use super::*;
use crate::core::persistence::EncodeOutcome;
use crate::core::{
    model::{ModelHandle, OwnedModel},
    reactive::{Computed, with_world_scope},
};
use core::cell::RefCell;

#[derive(Clone, Copy)]
struct Change;
impl Change {
    const ALL: Self = Self;
    fn contains(self, _: Self) -> bool {
        true
    }
}

#[crate::model(persistence, change = Change, watch(content = Change::ALL))]
struct Leaf {
    #[persist]
    #[observe]
    value: u8,
    scratch: u8,
    extractions: Rc<Cell<usize>>,
    sound: Option<u8>,
}
#[crate::model]
impl Leaf {
    #[effects]
    fn take_sound(&mut self) -> Option<u8> {
        self.extractions.set(self.extractions.get() + 1);
        self.value += 1;
        self.sound.take()
    }
}

#[crate::model(persistence)]
struct Parent {
    #[persist]
    left: crate::model!(owned Leaf),
    #[persist]
    right: crate::model!(owned Leaf),
}
#[crate::model]
impl Parent {
    #[observe]
    fn total(&self) -> u16 {
        u16::from(self.left.value()) + u16::from(self.right.value())
    }
}

fn parent(left: u8, right: u8, extractions: &Rc<Cell<usize>>) -> Parent {
    let leaf = |value| {
        Leaf {
            value,
            scratch: 9,
            extractions: extractions.clone(),
            sound: Some(1),
        }
        .into()
    };
    Parent {
        left: leaf(left),
        right: leaf(right),
    }
}

#[test]
fn restore_publishes_the_complete_tree_without_extracting_or_delivering_effects() {
    let extractions = Rc::new(Cell::new(0));
    let mut world = World::new();
    let (registration, handle) = model::register(&mut world, parent(1, 2, &extractions));
    let left = handle.left();
    let identity = left.access().clone();
    let total_source = handle.clone();
    let left_source = left.clone();
    let (total, observed) = with_world_scope(&mut world, || {
        (
            Computed::new(move || total_source.total()),
            Computed::new(move || left_source.value()),
        )
    });
    assert_eq!((total.get(), observed.get()), (3, 1));
    let deliveries = Rc::new(Cell::new(0));
    let count = deliveries.clone();
    left.on_effect(move |_: &u8| count.set(count.get() + 1))
        .unwrap();
    let endpoint = PersistenceEndpoint::new(&registration.root, FieldCodec::default());
    let mut store = Store::default();
    FieldCodec::default()
        .encode(&parent(20, 30, &extractions), &mut store.bytes)
        .unwrap();
    // This storage clones its byte buffer once; preparing and applying the static tree adds no allocation.
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(endpoint.restore(&world, "parent", &store), Ok(true));
        }),
        1
    );
    assert_eq!((total.get(), observed.get()), (50, 20));
    assert!(left.access().same_identity(&identity));
    assert_eq!(left.content_revision(), 1);
    assert_eq!(handle.right().content_revision(), 1);
    assert_eq!(extractions.get(), 0);
    assert_eq!(deliveries.get(), 0);
    assert_eq!(left.read(|raw| (raw.scratch, raw.sound)), (9, Some(1)));
    assert!(!registration.root.persistence.pending());
    let source = registration.root.value.borrow();
    assert_eq!(source.left.read(|raw| raw.value), 20);
}

struct ReadStore {
    reads: Cell<usize>,
    bytes: Option<Vec<u8>>,
}
impl Storage for ReadStore {
    fn read(&self, _: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.bytes.clone())
    }
    fn write(&mut self, _: &str, _: &[u8]) -> Result<WriteOutcome, StorageError> {
        unreachable!()
    }
    fn remove(&mut self, _: &str) -> Result<(), StorageError> {
        unreachable!()
    }
}

#[test]
fn missing_and_invalid_records_leave_data_and_candidate_unchanged() {
    let mut world = World::new();
    let (registration, handle) = model::register(&mut world, state());
    let endpoint = PersistenceEndpoint::new(&registration.root, FieldCodec::default());
    let candidate = &registration.root.persistence;
    candidate.confirm(candidate.stamp());
    let stamp = candidate.stamp();
    let mut store = ReadStore {
        reads: Cell::new(0),
        bytes: None,
    };
    assert_eq!(endpoint.restore(&world, "state", &store), Ok(false));
    store.bytes = Some(vec![0, 1, 2]);
    assert_eq!(
        endpoint.restore(&world, "state", &store),
        Err(PersistenceError::InvalidArchive)
    );
    assert_eq!(handle.value(), 7);
    assert_eq!(candidate.stamp(), stamp);
    assert!(!candidate.pending());
    handle.increment();
    assert_eq!(handle.value(), 8);
}

#[test]
fn foreign_busy_read_only_and_retired_roots_are_rejected_before_reading_storage() {
    let mut world = World::new();
    let (registration, _) = model::register(&mut world, state());
    let endpoint = PersistenceEndpoint::new(&registration.root, FieldCodec::default());
    let store = ReadStore {
        reads: Cell::new(0),
        bytes: None,
    };
    assert_eq!(
        endpoint.restore(&World::new(), "state", &store),
        Err(PersistenceError::WrongOwner)
    );
    let borrow = registration.root.value.borrow();
    assert_eq!(
        endpoint.restore(&world, "state", &store),
        Err(PersistenceError::ModelBusy)
    );
    drop(borrow);
    let read_only = crate::core::reactive::ModelReadOnlyGuard::enter();
    assert_eq!(
        endpoint.restore(&world, "state", &store),
        Err(PersistenceError::ModelBusy)
    );
    drop(read_only);
    drop(registration);
    assert_eq!(
        endpoint.restore(&world, "state", &store),
        Err(PersistenceError::Unregistered)
    );
    assert_eq!(store.reads.get(), 0);
}

struct FaultyCodec {
    prepare: Box<dyn Fn() -> Result<(), PersistenceError>>,
    panic_apply: bool,
}

#[crate::model(persistence)]
struct WithRows {
    #[persist]
    value: u8,
    rows: crate::model!(owned crate::core::model::ModelList<State>),
}
#[crate::model]
impl WithRows {}

#[test]
fn active_member_iteration_blocks_whole_root_restore_without_storage_io() {
    let mut world = World::new();
    let (registration, handle) = model::register(
        &mut world,
        WithRows {
            value: 4,
            rows: crate::core::model::ModelList::from([state()]).into(),
        },
    );
    let endpoint = PersistenceEndpoint::new(&registration.root, FieldCodec::default());
    let store = ReadStore {
        reads: Cell::new(0),
        bytes: None,
    };
    let rows = handle.rows();
    let iterator = rows.iter();
    assert_eq!(
        endpoint.restore(&world, "root", &store),
        Err(PersistenceError::ModelBusy)
    );
    assert_eq!(store.reads.get(), 0);
    drop(iterator);
    assert_eq!(endpoint.restore(&world, "root", &store), Ok(false));
    assert_eq!(store.reads.get(), 1);
}
impl ModelCodec for FaultyCodec {
    type Model = State;
    type Prepared = u8;
    fn encode(&self, _: &State, _: &mut Vec<u8>) -> Result<EncodeOutcome, PersistenceError> {
        unreachable!()
    }
    fn prepare(&self, _: &State, _: &[u8]) -> Result<u8, PersistenceError> {
        (self.prepare)()?;
        Ok(42)
    }
    fn apply(&self, model: &mut State, value: u8) {
        model.value = value;
        assert!(!self.panic_apply, "invalid codec application");
    }
}

#[test]
fn failed_preparation_does_not_poison_but_unwound_application_does() {
    let mut world = World::new();
    let (registration, handle) = model::register(&mut world, state());
    let store = ReadStore {
        reads: Cell::new(0),
        bytes: Some(Vec::new()),
    };
    let fail = PersistenceEndpoint::new(
        &registration.root,
        FaultyCodec {
            prepare: Box::new(|| Err(PersistenceError::Capacity)),
            panic_apply: false,
        },
    );
    assert_eq!(
        fail.restore(&world, "state", &store),
        Err(PersistenceError::Capacity)
    );
    assert_eq!(handle.value(), 7);
    assert!(!registration.root.poisoned.get());
    let panic = PersistenceEndpoint::new(
        &registration.root,
        FaultyCodec {
            prepare: Box::new(|| Ok(())),
            panic_apply: true,
        },
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || panic.restore(&world, "state", &store)
        ))
        .is_err()
    );
    assert!(registration.root.poisoned.get());
    assert!(!registration.root.updating.get());
    assert_eq!(
        panic.restore(&world, "state", &store),
        Err(PersistenceError::Poisoned)
    );
}

#[test]
fn retirement_during_prepare_never_applies_the_prepared_patch() {
    let mut world = World::new();
    let (registration, _) = model::register(&mut world, state());
    let root = registration.root.clone();
    let retained = RefCell::new(Some(registration));
    let codec = FaultyCodec {
        prepare: Box::new(move || {
            retained.borrow_mut().take();
            Ok(())
        }),
        panic_apply: false,
    };
    let endpoint = PersistenceEndpoint::new(&root, codec);
    let store = ReadStore {
        reads: Cell::new(0),
        bytes: Some(Vec::new()),
    };
    assert_eq!(
        endpoint.restore(&world, "state", &store),
        Err(PersistenceError::Unregistered)
    );
    assert_eq!(root.value.borrow().value, 7);
}
