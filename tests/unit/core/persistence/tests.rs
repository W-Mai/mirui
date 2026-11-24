use super::*;
use crate::app::App;
use crate::core::storage::MemoryStorage;

fn fresh_world() -> (World, PersistenceRegistry) {
    let world = World::new();
    let registry = PersistenceRegistry {
        storage: Box::new(MemoryStorage::new()),
        items: Vec::new(),
        autosave: Autosave::default(),
        models: Vec::new(),
        codecs: codec_registry::CodecRegistry::default(),
        output: Vec::new(),
        enabled: true,
    };
    (world, registry)
}

#[test]
fn autosave_runs_at_frame_boundaries_and_suspend_without_a_clock() {
    use core::cell::Cell;
    std::thread_local! {
        static NOW: Cell<u64> = const { Cell::new(0) };
    }
    let saves = alloc::rc::Rc::new(Cell::new(0));
    let recorded = saves.clone();
    let mut app = App::headless(16, 16);
    app.add_resource(crate::ecs::MonoClock::new(|| NOW.get()));
    app.add_plugin(
        PersistencePlugin::new(MemoryStorage::new())
            .autosave(core::time::Duration::from_nanos(500))
            .bytes(
                "count",
                move || {
                    recorded.set(recorded.get() + 1);
                    None
                },
                |_| Ok(()),
            ),
    );
    app.spawn_root().id();
    for (now, expected) in [(499, 0), (500, 1), (999, 1), (1000, 2)] {
        NOW.set(now);
        app.tick();
        assert_eq!(saves.get(), expected);
    }
    app.world
        .resource_mut::<PersistenceRegistry>()
        .unwrap()
        .autosave(0);
    NOW.set(10_000);
    app.tick();
    assert_eq!(saves.get(), 2);
    app.world.remove_resource::<crate::ecs::MonoClock>();
    app.suspend();
    assert_eq!(saves.get(), 3);
}

#[test]
fn signal_round_trip_through_storage() {
    let (world, mut reg) = fresh_world();
    let count = Signal::new(7i32);
    reg.signal("count", count.clone()).unwrap();
    count.set(42);
    reg.save_all(&world).unwrap();

    let count_back = Signal::new(0i32);
    let (_, mut restored) = fresh_world();
    restored.storage = reg.storage;
    restored.signal("count", count_back.clone()).unwrap();
    assert_eq!(count_back.get_untracked(), 42);
}

#[test]
fn signal_first_register_seeds_from_disk() {
    let (_, mut reg) = fresh_world();
    write_value(
        &mut *reg.storage,
        "score",
        &postcard::to_allocvec(&99u32).unwrap(),
    )
    .unwrap();
    let score = Signal::new(0u32);
    reg.signal("score", score.clone()).unwrap();
    assert_eq!(score.get_untracked(), 99);
}

#[test]
fn save_single_key_skips_other_items() {
    let (world, mut reg) = fresh_world();
    let a = Signal::new(1i32);
    let b = Signal::new(2i32);
    reg.signal("a", a.clone()).unwrap();
    reg.signal("b", b.clone()).unwrap();
    a.set(10);
    b.set(20);
    reg.save(&world, "a").unwrap();

    let storage_a = read_value(&*reg.storage, "a").unwrap();
    let storage_b = read_value(&*reg.storage, "b").unwrap();
    let decoded_a: i32 = postcard::from_bytes(&storage_a.expect("a written")).unwrap();
    assert_eq!(decoded_a, 10);
    assert!(storage_b.is_none(), "save(\"a\") must not flush b");
}

#[test]
fn unknown_key_returns_unregistered() {
    let (world, mut reg) = fresh_world();
    assert_eq!(
        reg.save(&world, "missing-key"),
        Err(PersistenceError::Unregistered)
    );
}

#[test]
fn corrupt_payload_prevents_registration_and_overwrite() {
    let (world, mut reg) = fresh_world();
    reg.storage.write("count", b"garbage").unwrap();
    let count = Signal::new(3i32);
    assert_eq!(
        reg.signal("count", count.clone()),
        Err(PersistenceError::InvalidArchive)
    );
    assert_eq!(
        count.get_untracked(),
        3,
        "garbage payload leaves signal default"
    );
    reg.save_all(&world).unwrap();
    assert_eq!(reg.storage.read("count"), Ok(Some(b"garbage".to_vec())));
}

#[test]
fn bytes_escape_hatch_round_trip() {
    let (world, mut reg) = fresh_world();
    use alloc::rc::Rc;
    use core::cell::Cell;
    let captured: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let captured_save = captured.clone();
    let captured_restore = captured.clone();
    // Raw bytes — no VALUE_VERSION frame.
    reg.storage.write("custom", b"\x05").unwrap();
    reg.bytes(
        "custom",
        move || Some(vec![captured_save.get() as u8]),
        move |bytes| {
            if let Some(b) = bytes.last() {
                captured_restore.set(*b as u32);
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(captured.get(), 5, "restore ran on register");
    captured.set(9);
    reg.save_all(&world).unwrap();
    let stored = reg.storage.read("custom").unwrap().expect("written");
    assert_eq!(stored, vec![9], "bytes() writes raw user bytes, no frame");
}

#[test]
fn plugin_restores_before_return_and_reuses_mutable_save_callbacks() {
    let state = Signal::new(0_u8);
    let restored = state.clone();
    let saved = state.clone();
    let mut calls = 0_u8;
    let mut storage = MemoryStorage::new();
    storage.write("custom", &[5]).unwrap();
    let plugin = PersistencePlugin::new(storage).bytes(
        "custom",
        move || {
            calls += 1;
            (calls != 2).then(|| vec![saved.get_untracked() + calls])
        },
        move |bytes| {
            restored.set(bytes[0]);
            Ok(())
        },
    );
    let mut app = App::headless(16, 16);
    assert_eq!(state.get_untracked(), 0);
    app.add_plugin(plugin);
    assert_eq!(state.get_untracked(), 5);
    for expected in [6_u8, 6, 8] {
        app.suspend();
        app.suspend();
        let registry = app.world.resource::<PersistenceRegistry>().unwrap();
        assert_eq!(registry.storage.read("custom"), Ok(Some(vec![expected])));
        app.resume();
    }
}

#[test]
fn unrecognized_framing_does_not_register_a_save_endpoint() {
    let (_, mut reg) = fresh_world();
    reg.storage
        .write("k", &[99, 0x10, 0x00, 0x00, 0x00])
        .unwrap();
    let v = Signal::new(7i32);
    assert_eq!(
        reg.signal("k", v.clone()),
        Err(PersistenceError::InvalidArchive)
    );
    assert_eq!(v.get_untracked(), 7);
    assert!(reg.items.is_empty());
}

#[test]
fn failed_item_does_not_block_others_and_is_retried() {
    use crate::core::storage::{StorageError, WriteOutcome};
    use alloc::rc::Rc;
    use core::cell::Cell;

    struct FailingStorage {
        inner: MemoryStorage,
        fail: Rc<Cell<bool>>,
    }
    impl Storage for FailingStorage {
        fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
            if key == "unreadable" {
                Err(StorageError::Unavailable)
            } else {
                self.inner.read(key)
            }
        }
        fn write(&mut self, key: &str, bytes: &[u8]) -> Result<WriteOutcome, StorageError> {
            if key == "retry" && self.fail.get() {
                Err(StorageError::Io)
            } else {
                self.inner.write(key, bytes)
            }
        }
        fn remove(&mut self, key: &str) -> Result<(), StorageError> {
            self.inner.remove(key)
        }
    }

    let (world, mut registry) = fresh_world();
    let fail = Rc::new(Cell::new(true));
    registry.storage = Box::new(FailingStorage {
        inner: MemoryStorage::new(),
        fail: fail.clone(),
    });
    assert_eq!(
        registry.bytes("unreadable", || Some(vec![0]), |_| panic!("read failed")),
        Err(PersistenceError::Storage(StorageError::Unavailable))
    );
    registry.storage.write("rejected", &[9]).unwrap();
    assert_eq!(
        registry.bytes(
            "rejected",
            || Some(vec![0]),
            |_| Err(PersistenceError::InvalidArchive)
        ),
        Err(PersistenceError::InvalidArchive)
    );
    registry
        .bytes("retry", || Some(vec![1]), |_| Ok(()))
        .unwrap();
    registry
        .bytes("other", || Some(vec![2]), |_| Ok(()))
        .unwrap();
    assert_eq!(registry.items.len(), 2);
    assert_eq!(
        registry.save_all(&world),
        Err(PersistenceError::Storage(StorageError::Io))
    );
    assert_eq!(registry.storage.read("retry"), Ok(None));
    assert_eq!(registry.storage.read("other"), Ok(Some(vec![2])));
    assert_eq!(registry.storage.read("rejected"), Ok(Some(vec![9])));
    fail.set(false);
    registry.save_all(&world).unwrap();
    assert_eq!(registry.storage.read("retry"), Ok(Some(vec![1])));
}

#[test]
fn flush_retains_the_registry_allocation() {
    let (mut world, mut registry) = fresh_world();
    registry.bytes("unchanged", || None, |_| Ok(())).unwrap();
    world.insert_resource(registry);
    let address = world.resource::<PersistenceRegistry>().unwrap() as *const _;

    let allocations = crate::test_support::tracked_allocations(|| {
        for now_ns in 1..=16 {
            flush_and_stamp(&mut world, Some(now_ns));
        }
    });

    assert_eq!(allocations, 0);
    let registry = world.resource::<PersistenceRegistry>().unwrap();
    assert_eq!(registry as *const _, address);
    assert_eq!(registry.items.len(), 1);
}

#[test]
fn flush_restores_the_registry_when_a_save_callback_unwinds() {
    let (mut world, mut registry) = fresh_world();
    let mut first = true;
    registry
        .bytes(
            "custom",
            move || {
                assert!(!core::mem::replace(&mut first, false), "save failed");
                Some(vec![7])
            },
            |_| Ok(()),
        )
        .unwrap();
    world.insert_resource(registry);
    let address = world.resource::<PersistenceRegistry>().unwrap() as *const _;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        flush_and_stamp(&mut world, Some(12));
    }));
    assert!(result.is_err());
    let registry = world.resource::<PersistenceRegistry>().unwrap();
    assert_eq!(registry as *const _, address);

    flush_and_stamp(&mut world, Some(20));
    let registry = world.resource::<PersistenceRegistry>().unwrap();
    assert_eq!(registry.storage.read("custom"), Ok(Some(vec![7])));
}
