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
    pub fn signal<T>(&mut self, world: &mut World, key: &'static str, signal: Signal<T>)
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
        });
        let _ = world;
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
        });
    }

    /// Byte-level escape hatch. Use for types that don't serialize
    /// through serde, or when the wire format must match an existing
    /// disk layout.
    pub fn bytes(
        &mut self,
        world: &mut World,
        key: &'static str,
        mut save: impl FnMut(&World) -> Option<Vec<u8>> + 'static,
        mut restore: impl FnMut(&mut World, &[u8]) + 'static,
    ) {
        if let Some(bytes) = read_value(&*self.storage, key) {
            restore(world, &bytes);
        }
        self.items.push(TrackedItem {
            key,
            save: Box::new(move |world| save(world)),
        });
    }

    /// Flush every tracked item to storage. Errors from individual
    /// items are swallowed so a corrupt entry doesn't block the rest.
    pub fn save_all(&mut self, world: &World) {
        for item in &mut self.items {
            if let Some(bytes) = (item.save)(world) {
                write_value(&mut *self.storage, item.key, &bytes);
            }
        }
    }

    /// Flush a single tracked item by key. Useful when only one piece
    /// of state changed and the caller wants to avoid touching others.
    pub fn save(&mut self, world: &World, key: &str) {
        for item in &mut self.items {
            if item.key == key {
                if let Some(bytes) = (item.save)(world) {
                    write_value(&mut *self.storage, item.key, &bytes);
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
        self.pending.push(Box::new(move |reg, world| {
            reg.signal(world, key, signal);
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
        // `T` lives inside the closure only, not on the plugin itself.
        let _ = PhantomData::<T>;
        self
    }

    pub fn bytes(
        mut self,
        key: &'static str,
        save: impl FnMut(&World) -> Option<Vec<u8>> + 'static,
        restore: impl FnMut(&mut World, &[u8]) + 'static,
    ) -> Self {
        self.pending.push(Box::new(move |reg, world| {
            reg.bytes(world, key, save, restore);
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

    fn on_pause(&mut self, world: &mut World) {
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
    let mut taken = match world.remove_resource::<PersistenceRegistry>() {
        Some(reg) => reg,
        None => return,
    };
    taken.save_all(world);
    taken.last_save_ms = now_ms;
    world.insert_resource(taken);
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
        let (mut world, mut reg) = fresh_world();
        let count = Signal::new(7i32);
        reg.signal(&mut world, "count", count.clone());
        count.set(42);
        reg.save_all(&world);

        let count_back = Signal::new(0i32);
        reg.signal(&mut world, "count", count_back.clone());
        assert_eq!(count_back.get_untracked(), 42);
    }

    #[test]
    fn signal_first_register_seeds_from_disk() {
        let (mut world, mut reg) = fresh_world();
        write_value(
            &mut *reg.storage,
            "score",
            &postcard::to_allocvec(&99u32).unwrap(),
        );
        let score = Signal::new(0u32);
        reg.signal(&mut world, "score", score.clone());
        assert_eq!(score.get_untracked(), 99);
    }

    #[test]
    fn save_single_key_skips_other_items() {
        let (mut world, mut reg) = fresh_world();
        let a = Signal::new(1i32);
        let b = Signal::new(2i32);
        reg.signal(&mut world, "a", a.clone());
        reg.signal(&mut world, "b", b.clone());
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
        let (mut world, mut reg) = fresh_world();
        reg.storage.write("count", b"garbage");
        let count = Signal::new(3i32);
        reg.signal(&mut world, "count", count.clone());
        assert_eq!(
            count.get_untracked(),
            3,
            "garbage payload leaves signal default"
        );
    }

    #[test]
    fn bytes_escape_hatch_round_trip() {
        let (mut world, mut reg) = fresh_world();
        use alloc::rc::Rc;
        use core::cell::Cell;
        let captured: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let captured_save = captured.clone();
        let captured_restore = captured.clone();
        reg.storage.write("custom", b"\x01\x00\x00\x00\x05");
        reg.bytes(
            &mut world,
            "custom",
            move |_world| Some(vec![captured_save.get() as u8]),
            move |_world, bytes| {
                if let Some(b) = bytes.last() {
                    captured_restore.set(*b as u32);
                }
            },
        );
        assert_eq!(captured.get(), 5, "restore ran on register");
        captured.set(9);
        reg.save_all(&world);
        let stored = read_value(&*reg.storage, "custom").expect("written");
        assert_eq!(stored, vec![9]);
    }

    #[test]
    fn version_mismatch_treated_as_missing() {
        let (mut world, mut reg) = fresh_world();
        reg.storage.write("k", &[99, 0x10, 0x00, 0x00, 0x00]);
        let v = Signal::new(7i32);
        reg.signal(&mut world, "k", v.clone());
        assert_eq!(v.get_untracked(), 7);
    }
}
