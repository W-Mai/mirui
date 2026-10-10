//! Typed save / restore on top of [`Storage`]. Default-encodes through
//! postcard (compact, no_std + alloc); `bytes(...)` gives a byte-level
//! escape hatch for custom wire formats.

extern crate alloc;

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

type SaveFn = Box<dyn FnMut(&World) -> Option<Vec<u8>>>;

struct TrackedItem {
    key: &'static str,
    save: SaveFn,
    // `bytes()` escape hatch round-trips verbatim; postcard path gets a
    // VALUE_VERSION byte for migrations.
    framed: bool,
}

/// World resource the plugin installs. Live save/restore endpoints stay
/// reachable after `App::add_plugin` so user systems can register more
/// items as widgets spawn at runtime, or trigger `save_all` manually.
pub struct PersistenceRegistry {
    storage: Box<dyn Storage>,
    items: Vec<TrackedItem>,
    autosave_interval_ms: Option<u32>,
    last_save_ms: u32,
}

impl PersistenceRegistry {
    pub fn autosave_every_ms(&mut self, interval: u32) -> &mut Self {
        self.autosave_interval_ms = if interval == 0 { None } else { Some(interval) };
        self
    }

    /// Persist a `Signal<T>` under `key`. Pulls any existing stored
    /// value into the signal immediately, so late-registered widgets
    /// catch up to disk state in the same call.
    pub fn signal<T>(&mut self, key: &'static str, signal: Signal<T>)
    where
        T: Serialize + DeserializeOwned + Clone + 'static,
    {
        if let Some(bytes) = read_value(&*self.storage, key) {
            if let Ok(value) = postcard::from_bytes::<T>(&bytes) {
                signal.set(value);
            }
        }
        let save_sig = signal;
        self.items.push(TrackedItem {
            key,
            save: Box::new(move |_world: &World| {
                postcard::to_allocvec(&save_sig.get_untracked()).ok()
            }),
            framed: true,
        });
    }

    /// Persist a World resource `T`. The resource must already exist
    /// when `save_all` runs; restore only writes if a resource of type
    /// `T` is present (matching the resource's existing identity rules).
    pub fn resource<T>(&mut self, world: &mut World, key: &'static str)
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        if let Some(bytes) = read_value(&*self.storage, key) {
            if let Ok(value) = postcard::from_bytes::<T>(&bytes) {
                world.insert_resource(value);
            }
        }
        self.items.push(TrackedItem {
            key,
            save: Box::new(|world: &World| {
                let r = world.resource::<T>()?;
                postcard::to_allocvec(r).ok()
            }),
            framed: true,
        });
    }

    /// Save and restore custom bytes without framing. Restore runs immediately when a stored value exists; returning `None` from save leaves that value unchanged.
    pub fn bytes(
        &mut self,
        key: &'static str,
        mut save: impl FnMut() -> Option<Vec<u8>> + 'static,
        mut restore: impl FnMut(&[u8]) + 'static,
    ) {
        if let Some(bytes) = self.storage.read(key) {
            restore(&bytes);
        }
        self.items.push(TrackedItem {
            key,
            save: Box::new(move |_| save()),
            framed: false,
        });
    }

    /// Flush every tracked item to storage. Errors from individual
    /// items are swallowed so a corrupt entry doesn't block the rest.
    pub fn save_all(&mut self, world: &World) {
        for item in &mut self.items {
            if let Some(bytes) = (item.save)(world) {
                if item.framed {
                    write_value(&mut *self.storage, item.key, &bytes);
                } else {
                    self.storage.write(item.key, &bytes);
                }
            }
        }
    }

    /// Flush a single tracked item by key. Useful when only one piece
    /// of state changed and the caller wants to avoid touching others.
    pub fn save(&mut self, world: &World, key: &str) {
        for item in &mut self.items {
            if item.key == key {
                if let Some(bytes) = (item.save)(world) {
                    if item.framed {
                        write_value(&mut *self.storage, item.key, &bytes);
                    } else {
                        self.storage.write(item.key, &bytes);
                    }
                }
                return;
            }
        }
    }

    /// Drop persisted bytes for `key`. The tracked item stays
    /// registered — future saves will repopulate storage.
    pub fn remove(&mut self, key: &str) {
        self.storage.remove(key);
    }
}

fn read_value(storage: &dyn Storage, key: &str) -> Option<Vec<u8>> {
    let bytes = storage.read(key)?;
    if bytes.first().copied() == Some(VALUE_VERSION) {
        Some(bytes[1..].to_vec())
    } else {
        None
    }
}

fn write_value(storage: &mut dyn Storage, key: &str, payload: &[u8]) {
    let mut framed = Vec::with_capacity(payload.len() + 1);
    framed.push(VALUE_VERSION);
    framed.extend_from_slice(payload);
    storage.write(key, &framed);
}

/// Deferred-build registration captured before the plugin reaches
/// `App::add_plugin`. The plugin replays them inside `build` so the
/// registry it just inserted into the World sees a fully primed set
/// of tracked items by the first `on_start` hook.
type Pending = Box<dyn FnOnce(&mut PersistenceRegistry, &mut World)>;

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
    autosave_interval_ms: Option<u32>,
}

impl<S: Storage + 'static> PersistencePlugin<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage: Some(storage),
            pending: Vec::new(),
            autosave_interval_ms: None,
        }
    }

    pub fn signal<T>(mut self, key: &'static str, signal: Signal<T>) -> Self
    where
        T: Serialize + DeserializeOwned + Clone + 'static,
    {
        self.pending.push(Box::new(move |reg, _world| {
            reg.signal(key, signal);
        }));
        self
    }

    pub fn resource<T>(mut self, key: &'static str) -> Self
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        self.pending.push(Box::new(move |reg, world| {
            reg.resource::<T>(world, key);
        }));
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
    ///     },
    /// );
    /// ```
    pub fn bytes(
        mut self,
        key: &'static str,
        save: impl FnMut() -> Option<Vec<u8>> + 'static,
        restore: impl FnMut(&[u8]) + 'static,
    ) -> Self {
        self.pending.push(Box::new(move |reg, _world| {
            reg.bytes(key, save, restore);
        }));
        self
    }

    pub fn autosave_every_ms(mut self, interval: u32) -> Self {
        self.autosave_interval_ms = if interval == 0 { None } else { Some(interval) };
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
            autosave_interval_ms: self.autosave_interval_ms,
            last_save_ms: clock_ms(&app.world),
        };
        // No MonoClock → autosave never ticks; on_suspend/on_quit still flush.
        #[cfg(feature = "std")]
        if self.autosave_interval_ms.is_some()
            && app.world.resource::<crate::ecs::MonoClock>().is_none()
        {
            crate::warn!(
                target: "mirui::persistence",
                "PersistencePlugin: autosave_every_ms set but no MonoClock resource installed; \
                 periodic flushes will not fire. Add a clock plugin (e.g. StdInstantClockPlugin) \
                 before this one. on_suspend / on_quit flushes still work."
            );
        }
        for register in self.pending.drain(..) {
            register(&mut registry, &mut app.world);
        }
        app.world.insert_resource(registry);
    }

    fn post_render(&mut self, world: &mut World, _render_nanos: u64) {
        let now_ms = clock_ms(world);
        let due = world
            .resource::<PersistenceRegistry>()
            .map(|reg| match reg.autosave_interval_ms {
                Some(interval) => now_ms.wrapping_sub(reg.last_save_ms) >= interval,
                None => false,
            })
            .unwrap_or(false);
        if due {
            flush_and_stamp(world, now_ms);
        }
    }

    fn on_suspend(&mut self, world: &mut World) {
        let now_ms = clock_ms(world);
        flush_and_stamp(world, now_ms);
    }

    fn on_quit(&mut self, world: &mut World) {
        let now_ms = clock_ms(world);
        flush_and_stamp(world, now_ms);
    }
}

fn clock_ms(world: &World) -> u32 {
    world
        .resource::<crate::ecs::MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0)
}

fn flush_and_stamp(world: &mut World, now_ms: u32) {
    world.with_resource_box::<PersistenceRegistry, _>(|registry, world| {
        registry.save_all(world);
        registry.last_save_ms = now_ms;
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
            autosave_interval_ms: None,
            last_save_ms: 0,
        };
        (world, registry)
    }

    #[test]
    fn signal_round_trip_through_storage() {
        let (world, mut reg) = fresh_world();
        let count = Signal::new(7i32);
        reg.signal("count", count.clone());
        count.set(42);
        reg.save_all(&world);

        let count_back = Signal::new(0i32);
        reg.signal("count", count_back.clone());
        assert_eq!(count_back.get_untracked(), 42);
    }

    #[test]
    fn signal_first_register_seeds_from_disk() {
        let (_, mut reg) = fresh_world();
        write_value(
            &mut *reg.storage,
            "score",
            &postcard::to_allocvec(&99u32).unwrap(),
        );
        let score = Signal::new(0u32);
        reg.signal("score", score.clone());
        assert_eq!(score.get_untracked(), 99);
    }

    #[test]
    fn save_single_key_skips_other_items() {
        let (world, mut reg) = fresh_world();
        let a = Signal::new(1i32);
        let b = Signal::new(2i32);
        reg.signal("a", a.clone());
        reg.signal("b", b.clone());
        a.set(10);
        b.set(20);
        reg.save(&world, "a");

        let storage_a = read_value(&*reg.storage, "a");
        let storage_b = read_value(&*reg.storage, "b");
        let decoded_a: i32 = postcard::from_bytes(&storage_a.expect("a written")).unwrap();
        assert_eq!(decoded_a, 10);
        assert!(storage_b.is_none(), "save(\"a\") must not flush b");
    }

    #[test]
    fn unknown_key_is_skipped_not_panicking() {
        let (world, mut reg) = fresh_world();
        reg.save(&world, "missing-key");
    }

    #[test]
    fn corrupt_payload_falls_through_silently() {
        let (_, mut reg) = fresh_world();
        reg.storage.write("count", b"garbage");
        let count = Signal::new(3i32);
        reg.signal("count", count.clone());
        assert_eq!(
            count.get_untracked(),
            3,
            "garbage payload leaves signal default"
        );
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
        reg.storage.write("custom", b"\x05");
        reg.bytes(
            "custom",
            move || Some(vec![captured_save.get() as u8]),
            move |bytes| {
                if let Some(b) = bytes.last() {
                    captured_restore.set(*b as u32);
                }
            },
        );
        assert_eq!(captured.get(), 5, "restore ran on register");
        captured.set(9);
        reg.save_all(&world);
        let stored = reg.storage.read("custom").expect("written");
        assert_eq!(stored, vec![9], "bytes() writes raw user bytes, no frame");
    }

    #[test]
    fn plugin_restores_before_return_and_reuses_mutable_save_callbacks() {
        let state = Signal::new(0_u8);
        let restored = state.clone();
        let saved = state.clone();
        let mut calls = 0_u8;
        let mut storage = MemoryStorage::new();
        storage.write("custom", &[5]);
        let plugin = PersistencePlugin::new(storage).bytes(
            "custom",
            move || {
                calls += 1;
                (calls != 2).then(|| vec![saved.get_untracked() + calls])
            },
            move |bytes| restored.set(bytes[0]),
        );
        let mut app = App::headless(16, 16);
        assert_eq!(state.get_untracked(), 0);
        app.add_plugin(plugin);
        assert_eq!(state.get_untracked(), 5);
        for expected in [6_u8, 6, 8] {
            app.suspend();
            app.suspend();
            let registry = app.world.resource::<PersistenceRegistry>().unwrap();
            assert_eq!(registry.storage.read("custom"), Some(vec![expected]));
            app.resume();
        }
    }

    #[test]
    fn version_mismatch_treated_as_missing() {
        let (_, mut reg) = fresh_world();
        reg.storage.write("k", &[99, 0x10, 0x00, 0x00, 0x00]);
        let v = Signal::new(7i32);
        reg.signal("k", v.clone());
        assert_eq!(v.get_untracked(), 7);
    }

    #[test]
    fn flush_retains_the_registry_allocation() {
        let (mut world, mut registry) = fresh_world();
        registry.bytes("unchanged", || None, |_| {});
        world.insert_resource(registry);
        let address = world.resource::<PersistenceRegistry>().unwrap() as *const _;

        let allocations = crate::test_support::tracked_allocations(|| {
            for now_ms in 1..=16 {
                flush_and_stamp(&mut world, now_ms);
            }
        });

        assert_eq!(allocations, 0);
        let registry = world.resource::<PersistenceRegistry>().unwrap();
        assert_eq!(registry as *const _, address);
        assert_eq!(registry.last_save_ms, 16);
        assert_eq!(registry.items.len(), 1);
    }

    #[test]
    fn flush_restores_the_registry_when_a_save_callback_unwinds() {
        let (mut world, mut registry) = fresh_world();
        let mut first = true;
        registry.bytes(
            "custom",
            move || {
                assert!(!core::mem::replace(&mut first, false), "save failed");
                Some(vec![7])
            },
            |_| {},
        );
        world.insert_resource(registry);
        let address = world.resource::<PersistenceRegistry>().unwrap() as *const _;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            flush_and_stamp(&mut world, 12);
        }));
        assert!(result.is_err());
        let registry = world.resource::<PersistenceRegistry>().unwrap();
        assert_eq!(registry as *const _, address);
        assert_eq!(registry.last_save_ms, 0);

        flush_and_stamp(&mut world, 20);
        let registry = world.resource::<PersistenceRegistry>().unwrap();
        assert_eq!(registry.storage.read("custom"), Some(vec![7]));
        assert_eq!(registry.last_save_ms, 20);
    }
}
