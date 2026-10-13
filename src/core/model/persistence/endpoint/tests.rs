use super::*;
use crate::core::persistence::EncodeOutcome;
use crate::{
    core::{model, persistence::FieldCodec, storage::StorageError},
    ecs::World,
    test_support::tracked_allocations,
};
use alloc::boxed::Box;
use core::cell::Cell;

mod restore;

#[crate::model(persistence)]
struct State {
    #[persist]
    #[observe]
    value: u8,
    temporary: bool,
}

#[crate::model]
impl State {
    fn increment(&mut self) {
        self.value += 1;
    }
    fn temporary(&mut self) {
        self.temporary = !self.temporary;
    }
    fn panic(&mut self) {
        self.value += 1;
        panic!("update interrupted");
    }
}

fn state() -> State {
    State {
        value: 7,
        temporary: false,
    }
}

#[derive(Default)]
struct Store {
    bytes: Vec<u8>,
    writes: usize,
    fail: bool,
    on_write: Option<Box<dyn FnMut()>>,
}

impl Storage for Store {
    fn read(&self, _: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(Some(self.bytes.clone()))
    }
    fn write(&mut self, _: &str, bytes: &[u8]) -> Result<WriteOutcome, StorageError> {
        self.writes += 1;
        if self.fail {
            return Err(StorageError::Io);
        }
        if let Some(callback) = &mut self.on_write {
            callback();
        }
        if self.bytes == bytes {
            return Ok(WriteOutcome::Unchanged);
        }
        self.bytes.clear();
        self.bytes.extend_from_slice(bytes);
        Ok(WriteOutcome::Written)
    }
    fn remove(&mut self, _: &str) -> Result<(), StorageError> {
        self.bytes.clear();
        Ok(())
    }
}

#[test]
fn write_receipts_control_candidates_and_reuse_encoding_capacity() {
    let mut world = World::new();
    let (registration, handle) = model::register(&mut world, state());
    let endpoint = PersistenceEndpoint::new(&registration.root, Rc::new(FieldCodec::default()));
    let candidate = &registration.root.persistence;
    let mut store = Store::default();
    let mut output = Vec::new();
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, false),
        Ok(Some(WriteOutcome::Written))
    );
    assert!(!candidate.pending());
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, false),
        Ok(None)
    );
    assert_eq!(store.writes, 1);
    handle.temporary();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                endpoint.save(&world, "state", &mut store, &mut output, false),
                Ok(Some(WriteOutcome::Unchanged))
            );
        }),
        0
    );
    assert!(!candidate.pending());
    handle.increment();
    store.fail = true;
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, false),
        Err(PersistenceError::Storage(StorageError::Io))
    );
    assert!(candidate.pending());
    store.fail = false;
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, false),
        Ok(Some(WriteOutcome::Written))
    );
    assert!(!candidate.pending());
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, true),
        Ok(Some(WriteOutcome::Unchanged))
    );
    assert_eq!(store.writes, 5);
}

#[test]
fn mutation_during_storage_write_cannot_acknowledge_newer_data() {
    let mut world = World::new();
    let (registration, handle) = model::register(&mut world, state());
    let endpoint = PersistenceEndpoint::new(&registration.root, Rc::new(FieldCodec::default()));
    let mut store = Store {
        on_write: Some(Box::new(move || handle.increment())),
        ..Store::default()
    };
    let mut output = Vec::new();
    endpoint
        .save(&world, "state", &mut store, &mut output, false)
        .unwrap();
    assert!(registration.root.persistence.pending());
    assert_eq!(registration.root.value.borrow().value, 8);
    let codec = FieldCodec::default();
    let mut stored = state();
    let prepared = codec.prepare(&stored, &store.bytes).unwrap();
    codec.apply(&mut stored, prepared);
    assert_eq!(stored.value, 7);
    store.on_write = None;
    endpoint
        .save(&world, "state", &mut store, &mut output, false)
        .unwrap();
    assert!(!registration.root.persistence.pending());
}

#[test]
fn failed_explicit_save_becomes_retryable_even_for_a_clean_root() {
    let mut world = World::new();
    let (registration, _) = model::register(&mut world, state());
    let endpoint = PersistenceEndpoint::new(&registration.root, Rc::new(FieldCodec::default()));
    let mut store = Store::default();
    let mut output = Vec::new();
    endpoint
        .save(&world, "state", &mut store, &mut output, false)
        .unwrap();
    assert!(!registration.root.persistence.pending());
    store.fail = true;
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, true),
        Err(PersistenceError::Storage(StorageError::Io))
    );
    assert!(registration.root.persistence.pending());
    store.fail = false;
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, false),
        Ok(Some(WriteOutcome::Unchanged))
    );
    assert!(!registration.root.persistence.pending());
}

#[test]
fn invalid_roots_never_reach_storage() {
    let mut world = World::new();
    let (registration, handle) = model::register(&mut world, state());
    let root = registration.root.clone();
    let weak = Rc::downgrade(&root);
    let endpoint = PersistenceEndpoint::new(&root, Rc::new(FieldCodec::default()));
    let mut store = Store::default();
    let mut output = Vec::new();
    let foreign = World::new();
    assert_eq!(
        endpoint.save(&foreign, "state", &mut store, &mut output, true),
        Err(PersistenceError::WrongOwner)
    );
    let borrow = root.value.borrow_mut();
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, true),
        Err(PersistenceError::ModelBusy)
    );
    drop(borrow);
    root.updating.set(true);
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, true),
        Err(PersistenceError::ModelBusy)
    );
    root.updating.set(false);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.panic())).is_err());
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, true),
        Err(PersistenceError::Poisoned)
    );
    drop(registration);
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, true),
        Err(PersistenceError::Unregistered)
    );
    drop(root);
    assert!(weak.upgrade().is_none());
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, true),
        Err(PersistenceError::Unregistered)
    );
    assert_eq!(store.writes, 0);
}

struct RawCodec {
    fail: Cell<bool>,
    outcome: Cell<EncodeOutcome>,
    on_encode: Option<Box<dyn Fn()>>,
}

impl ModelCodec for RawCodec {
    type Model = State;
    type Prepared = u8;
    fn encode(
        &self,
        model: &State,
        output: &mut Vec<u8>,
    ) -> Result<EncodeOutcome, PersistenceError> {
        if let Some(callback) = &self.on_encode {
            callback();
        }
        output.extend_from_slice(&[0xff, model.value]);
        if self.fail.get() {
            Err(PersistenceError::Encoding)
        } else {
            Ok(self.outcome.get())
        }
    }
    fn prepare(&self, _: &State, bytes: &[u8]) -> Result<u8, PersistenceError> {
        match bytes {
            [0xff, value] => Ok(*value),
            _ => Err(PersistenceError::InvalidArchive),
        }
    }
    fn apply(&self, model: &mut State, value: u8) {
        model.value = value;
    }
}

#[test]
fn custom_codec_owns_complete_bytes_and_failed_encoding_never_writes() {
    let mut world = World::new();
    let (registration, _) = model::register(&mut world, state());
    let codec = Rc::new(RawCodec {
        fail: Cell::new(true),
        outcome: Cell::new(EncodeOutcome::Encoded),
        on_encode: None,
    });
    let endpoint = PersistenceEndpoint::new(&registration.root, codec.clone());
    let mut store = Store::default();
    let mut output = alloc::vec![1, 2, 3];
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, false),
        Err(PersistenceError::Encoding)
    );
    assert!(output.is_empty());
    assert_eq!(store.writes, 0);
    assert!(registration.root.persistence.pending());
    codec.fail.set(false);
    endpoint
        .save(&world, "state", &mut store, &mut output, false)
        .unwrap();
    assert_eq!(store.bytes, [0xff, 7]);
    assert!(!registration.root.persistence.pending());
    let mut restored = State {
        value: 0,
        temporary: true,
    };
    assert_eq!(
        codec.prepare(&restored, &[0xff]),
        Err(PersistenceError::InvalidArchive)
    );
    assert_eq!(restored.value, 0);
    let prepared = codec.prepare(&restored, &store.bytes).unwrap();
    codec.apply(&mut restored, prepared);
    assert_eq!(restored.value, 7);
    assert!(restored.temporary);
}

#[test]
fn retirement_during_encoding_prevents_storage_access() {
    let mut world = World::new();
    let (registration, _) = model::register(&mut world, state());
    let root = registration.root.clone();
    let registration = core::cell::RefCell::new(Some(registration));
    let codec = Rc::new(RawCodec {
        fail: Cell::new(false),
        outcome: Cell::new(EncodeOutcome::Deferred),
        on_encode: Some(Box::new(move || {
            registration.borrow_mut().take();
        })),
    });
    let endpoint = PersistenceEndpoint::new(&root, codec);
    let mut store = Store::default();
    let mut output = Vec::new();
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, false),
        Err(PersistenceError::Unregistered)
    );
    assert!(output.is_empty());
    assert_eq!(store.writes, 0);
}

#[test]
fn field_codec_prepares_complete_records_without_mutating_the_source() {
    let codec = FieldCodec::<State>::default();
    let mut bytes = Vec::new();
    codec.encode(&state(), &mut bytes).unwrap();
    let mut restored = State {
        value: 0,
        temporary: true,
    };
    assert!(codec.prepare(&restored, &bytes[..bytes.len() - 1]).is_err());
    let prepared = codec.prepare(&restored, &bytes).unwrap();
    assert_eq!(restored.value, 0);
    assert_eq!(
        tracked_allocations(|| codec.apply(&mut restored, prepared)),
        0
    );
    assert_eq!(restored.value, 7);
    assert!(restored.temporary);
}

#[test]
fn deferred_encoding_discards_partial_output_and_reuses_capacity_until_saved() {
    let mut world = World::new();
    let (registration, _) = model::register(&mut world, state());
    let codec = Rc::new(RawCodec {
        fail: Cell::new(false),
        outcome: Cell::new(EncodeOutcome::Deferred),
        on_encode: None,
    });
    let endpoint = PersistenceEndpoint::new(&registration.root, codec.clone());
    let mut store = Store::default();
    let mut output = Vec::with_capacity(8);
    let capacity = output.capacity();
    assert_eq!(
        tracked_allocations(|| {
            for force in [false, true] {
                assert_eq!(
                    endpoint.save(&world, "state", &mut store, &mut output, force),
                    Ok(None)
                );
                assert!(output.is_empty());
                assert_eq!(output.capacity(), capacity);
                assert!(registration.root.persistence.pending());
            }
        }),
        0
    );
    assert_eq!(store.writes, 0);
    codec.outcome.set(EncodeOutcome::Encoded);
    assert_eq!(
        endpoint.save(&world, "state", &mut store, &mut output, false),
        Ok(Some(WriteOutcome::Written))
    );
    assert_eq!(store.bytes, [0xff, 7]);
    assert!(!registration.root.persistence.pending());
}
