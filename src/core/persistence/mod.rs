//! Typed save and restore on top of [`Storage`]. Models use named field archives; signal and resource values use postcard. Byte registrations preserve their complete custom format.

extern crate alloc;

mod access;
#[doc(hidden)]
pub mod archive;
mod autosave;
mod codec_registry;
mod error;
mod model_codec;
mod models;
#[doc(hidden)]
pub mod value;
pub use access::Persistence;
use autosave::Autosave;
pub use autosave::IntoDuration;
pub use error::PersistenceError;
#[doc(hidden)]
pub use model_codec::FieldCodec;
pub use model_codec::ModelCodec;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::marker::PhantomData;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::core::reactive::Signal;
use crate::core::storage::Storage;
use crate::ecs::World;
use crate::surface::Surface;

const VALUE_VERSION: u8 = 1;

type SaveFn = Box<dyn FnMut(&World) -> Result<Option<Vec<u8>>, PersistenceError>>;

struct TrackedItem {
    key: &'static str,
    save: SaveFn,
    // `bytes()` escape hatch round-trips verbatim; postcard path gets a
    // VALUE_VERSION byte for migrations.
    framed: bool,
}

impl TrackedItem {
    fn save(&mut self, world: &World, storage: &mut dyn Storage) -> Result<(), PersistenceError> {
        if let Some(bytes) = (self.save)(world)? {
            if self.framed {
                write_value(storage, self.key, &bytes)?;
            } else {
                storage.write(self.key, &bytes)?;
            }
        }
        Ok(())
    }
}

/// World resource the plugin installs. Live save/restore endpoints stay
/// reachable after `App::add_plugin` so user systems can register more
/// items as widgets spawn at runtime, or trigger `save_all` manually.
pub struct PersistenceRegistry {
    storage: Box<dyn Storage>,
    items: Vec<TrackedItem>,
    autosave: Autosave,
    models: Vec<models::ModelEntry>,
    codecs: codec_registry::CodecRegistry,
    output: Vec<u8>,
    enabled: bool,
}

impl PersistenceRegistry {
    /// Enable or suspend IO. Re-enabling keeps current Model values and marks them for saving; it does not restore storage.
    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled && !self.enabled {
            for item in &self.models {
                item.endpoint.mark_pending();
            }
        }
        self.enabled = enabled;
    }
    /// Configure the periodic autosave interval, starting at the next clock sample.
    ///
    /// # Arguments
    ///
    /// * `interval` - Integer values are milliseconds. A `core::time::Duration` specifies the interval directly. Zero disables periodic autosaving. Invalid integers leave the configuration unchanged and emit a warning.
    pub fn autosave(&mut self, interval: impl IntoDuration) -> &mut Self {
        self.autosave.configure(interval);
        self
    }

    /// Persist a `Signal<T>` under `key`. Pulls any existing stored
    /// value into the signal immediately, so late-registered widgets
    /// catch up to disk state in the same call.
    pub fn signal<T>(
        &mut self,
        key: &'static str,
        signal: Signal<T>,
    ) -> Result<(), PersistenceError>
    where
        T: Serialize + DeserializeOwned + Clone + 'static,
    {
        self.check_key(key)?;
        if self.enabled
            && let Some(bytes) = read_value(&*self.storage, key)?
        {
            let value =
                postcard::from_bytes::<T>(&bytes).map_err(|_| PersistenceError::InvalidArchive)?;
            signal.set(value);
        }
        let save_sig = signal;
        self.items.push(TrackedItem {
            key,
            save: Box::new(move |_world: &World| {
                postcard::to_allocvec(&save_sig.get_untracked())
                    .map(Some)
                    .map_err(|_| PersistenceError::Encoding)
            }),
            framed: true,
        });
        Ok(())
    }

    /// Persist a World resource `T`. The resource must already exist
    /// when `save_all` runs; restore only writes if a resource of type
    /// `T` is present (matching the resource's existing identity rules).
    pub fn resource<T>(
        &mut self,
        world: &mut World,
        key: &'static str,
    ) -> Result<(), PersistenceError>
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        self.check_key(key)?;
        if self.enabled
            && let Some(bytes) = read_value(&*self.storage, key)?
        {
            let value =
                postcard::from_bytes::<T>(&bytes).map_err(|_| PersistenceError::InvalidArchive)?;
            world.insert_resource(value);
        }
        self.items.push(TrackedItem {
            key,
            save: Box::new(|world: &World| {
                world
                    .resource::<T>()
                    .map(postcard::to_allocvec)
                    .transpose()
                    .map_err(|_| PersistenceError::Encoding)
            }),
            framed: true,
        });
        Ok(())
    }

    /// Save and restore custom bytes without framing. Restore runs immediately when a stored value exists; returning `None` from save leaves that value unchanged. Restore errors prevent registration and leave storage untouched. Restore callbacks must validate before mutating state; their mutations are not rolled back.
    pub fn bytes(
        &mut self,
        key: &'static str,
        mut save: impl FnMut() -> Option<Vec<u8>> + 'static,
        mut restore: impl FnMut(&[u8]) -> Result<(), PersistenceError> + 'static,
    ) -> Result<(), PersistenceError> {
        self.check_key(key)?;
        if self.enabled
            && let Some(bytes) = self.storage.read(key)?
        {
            restore(&bytes)?;
        }
        self.items.push(TrackedItem {
            key,
            save: Box::new(move |_| Ok(save())),
            framed: false,
        });
        Ok(())
    }

    /// Flush every tracked item. All items are attempted; the first error is returned.
    pub fn save_all(&mut self, world: &World) -> Result<(), PersistenceError> {
        if !self.enabled {
            return Err(PersistenceError::Disabled);
        }
        let mut result = Ok(());
        for item in &mut self.items {
            let saved = item.save(world, &mut *self.storage);
            result = result.and(saved);
        }
        self.models.retain(|item| item.endpoint.is_alive());
        for item in &mut self.models {
            result = result.and(item.save(world, &mut *self.storage, &mut self.output, false));
        }
        result
    }

    /// Flush a single tracked item by key. Useful when only one piece
    /// of state changed and the caller wants to avoid touching others.
    pub fn save(&mut self, world: &World, key: &str) -> Result<(), PersistenceError> {
        if !self.enabled {
            return Err(PersistenceError::Disabled);
        }
        if let Some(item) = self
            .models
            .iter_mut()
            .find(|item| item.key == key && item.endpoint.is_alive())
        {
            return item.save(world, &mut *self.storage, &mut self.output, true);
        }
        for item in &mut self.items {
            if item.key == key {
                return item.save(world, &mut *self.storage);
            }
        }
        Err(PersistenceError::Unregistered)
    }

    /// Drop persisted bytes for `key`. The tracked item stays
    /// registered — future saves will repopulate storage.
    pub fn remove(&mut self, key: &str) -> Result<(), PersistenceError> {
        if !self.enabled {
            return Err(PersistenceError::Disabled);
        }
        self.storage.remove(key).map_err(Into::into)
    }
}

fn read_value(storage: &dyn Storage, key: &str) -> Result<Option<Vec<u8>>, PersistenceError> {
    let Some(mut bytes) = storage.read(key)? else {
        return Ok(None);
    };
    if bytes.first().copied() == Some(VALUE_VERSION) {
        bytes.remove(0);
        Ok(Some(bytes))
    } else {
        Err(PersistenceError::InvalidArchive)
    }
}

fn write_value(
    storage: &mut dyn Storage,
    key: &str,
    payload: &[u8],
) -> Result<(), PersistenceError> {
    let mut framed = Vec::with_capacity(payload.len() + 1);
    framed.push(VALUE_VERSION);
    framed.extend_from_slice(payload);
    storage.write(key, &framed)?;
    Ok(())
}

/// Deferred-build registration captured before the plugin reaches
/// `App::add_plugin`. The plugin replays them inside `build` so the
/// registry it just inserted into the World sees a fully primed set
/// of tracked items by the first `on_start` hook.
type Pending =
    Box<dyn FnOnce(&mut PersistenceRegistry, &mut World) -> Result<(), PersistenceError>>;

/// Typed save / restore on top of a [`Storage`] backend.
///
/// **Inserts**
/// - resource: [`PersistenceRegistry`]
/// - system:   none
/// - view:     none
/// - entity:   none
/// - hooks:    `build` (installs the registry, restores tracked items)
///   / `post_render` (autosave when due) / `on_suspend` (flush) /
///   `on_quit` (flush)
pub struct PersistencePlugin<S: Storage + 'static> {
    storage: Option<S>,
    pending: Vec<Pending>,
    autosave: Autosave,
    enabled: bool,
    codecs: codec_registry::CodecRegistry,
}

impl<S: Storage + 'static> PersistencePlugin<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage: Some(storage),
            pending: Vec::new(),
            autosave: Autosave::default(),
            enabled: true,
            codecs: codec_registry::CodecRegistry::default(),
        }
    }

    /// Configure whether the plugin performs IO. Disabled Model registrations keep their supplied values.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Select an archive codec for its associated Model type. All independent instances use this codec; nested Models remain part of their root's archive. Duplicate configurations emit a warning and retain the first codec.
    pub fn codec<C: ModelCodec>(mut self, codec: C) -> Self
    where
        C::Model: crate::core::model::PersistModel
            + crate::core::model::Model<Persistence = crate::core::model::Candidate>,
    {
        self.codecs.insert(codec);
        self
    }

    pub fn signal<T>(mut self, key: &'static str, signal: Signal<T>) -> Self
    where
        T: Serialize + DeserializeOwned + Clone + 'static,
    {
        self.pending
            .push(Box::new(move |reg, _world| reg.signal(key, signal)));
        self
    }

    pub fn resource<T>(mut self, key: &'static str) -> Self
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        self.pending
            .push(Box::new(move |reg, world| reg.resource::<T>(world, key)));
        let _ = PhantomData::<T>;
        self
    }

    /// Register custom encoding callbacks. Restore runs during plugin installation, before `add_plugin` returns. Save can return `None` to retain the stored value.
    ///
    /// ```
    /// use mirui::core::{persistence::PersistencePlugin, reactive::Signal, storage::MemoryStorage};
    ///
    /// let level = Signal::new(7_u8);
    /// let saved = level.clone();
    /// let restored = level.clone();
    /// let plugin = PersistencePlugin::new(MemoryStorage::new()).bytes(
    ///     "level",
    ///     move || Some(vec![saved.get_untracked()]),
    ///     move |bytes| {
    ///         if let [value] = bytes {
    ///             restored.set(*value);
    ///         }
    ///         Ok(())
    ///     },
    /// );
    /// ```
    pub fn bytes(
        mut self,
        key: &'static str,
        save: impl FnMut() -> Option<Vec<u8>> + 'static,
        restore: impl FnMut(&[u8]) -> Result<(), PersistenceError> + 'static,
    ) -> Self {
        self.pending
            .push(Box::new(move |reg, _world| reg.bytes(key, save, restore)));
        self
    }

    /// Configure the periodic autosave interval.
    ///
    /// # Arguments
    ///
    /// * `interval` - Integer values are milliseconds. A `core::time::Duration` specifies the interval directly. Zero disables periodic autosaving. Invalid integers leave the configuration unchanged and emit a warning.
    ///
    /// Due saves run at the next persistence scheduling boundary. Periodic autosaving is disabled by default; suspend and quit still save.
    ///
    /// ```
    /// use core::time::Duration;
    /// use mirui::core::{persistence::PersistencePlugin, storage::MemoryStorage};
    ///
    /// let plugin = PersistencePlugin::new(MemoryStorage::new()).autosave(1000);
    /// let plugin = plugin.autosave(Duration::from_secs(2));
    /// ```
    pub fn autosave(mut self, interval: impl IntoDuration) -> Self {
        self.autosave.configure(interval);
        self
    }
}

impl<B, F, S> Plugin<B, F> for PersistencePlugin<S>
where
    B: Surface,
    F: RendererFactory<B>,
    S: Storage + 'static,
{
    fn build(&mut self, app: &mut App<B, F>) {
        let storage = self
            .storage
            .take()
            .expect("PersistencePlugin::build called twice");
        let mut registry = PersistenceRegistry {
            storage: Box::new(storage),
            items: Vec::new(),
            autosave: core::mem::take(&mut self.autosave),
            models: Vec::new(),
            codecs: core::mem::take(&mut self.codecs),
            output: Vec::new(),
            enabled: self.enabled,
        };
        registry.autosave.reset(clock_ns(&app.world));
        // No MonoClock → autosave never ticks; on_suspend/on_quit still flush.
        if registry.autosave.enabled() && app.world.resource::<crate::ecs::MonoClock>().is_none() {
            crate::warn!(
                target: "mirui::persistence",
                "PersistencePlugin: autosave set but no MonoClock resource installed; \
                 periodic flushes will not fire. Add a clock plugin (e.g. StdInstantClockPlugin) \
                 before this one. on_suspend / on_quit flushes still work."
            );
        }
        for register in self.pending.drain(..) {
            if let Err(error) = register(&mut registry, &mut app.world) {
                crate::warn!(target: "mirui::persistence", "persistence registration failed: {error}");
            }
        }
        app.world.insert_resource(registry);
    }

    fn post_render(&mut self, world: &mut World, _render_nanos: u64) {
        let now_ns = clock_ns(world);
        let due = world
            .resource_mut::<PersistenceRegistry>()
            .is_some_and(|reg| reg.enabled && reg.autosave.due(now_ns));
        if due {
            flush_and_stamp(world, now_ns);
        }
    }

    fn on_suspend(&mut self, world: &mut World) {
        let now_ns = clock_ns(world);
        flush_and_stamp(world, now_ns);
    }

    fn on_quit(&mut self, world: &mut World) {
        let now_ns = clock_ns(world);
        flush_and_stamp(world, now_ns);
    }
}

fn clock_ns(world: &World) -> Option<u64> {
    world
        .resource::<crate::ecs::MonoClock>()
        .map(|c| c.now_ns())
}

fn flush_and_stamp(world: &mut World, now_ns: Option<u64>) {
    world.with_resource_box::<PersistenceRegistry, _>(|registry, world| {
        if !registry.enabled {
            return;
        }
        if let Err(error) = registry.save_all(world) {
            crate::warn!(target: "mirui::persistence", "persistence save failed: {error}");
        }
        registry.autosave.reset(now_ns);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
