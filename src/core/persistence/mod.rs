//! Typed save and restore on top of [`Storage`]. Models use named field archives; signal and resource values use postcard. Byte registrations preserve their complete custom format.

extern crate alloc;

mod access;
#[doc(hidden)]
pub mod archive;
mod autosave;
mod codec_registry;
mod commands;
mod error;
mod model_codec;
mod models;
mod status;
mod target;
pub use target::SaveTarget;
#[doc(hidden)]
pub mod value;
pub use access::Persistence;
use autosave::Autosave;
pub use autosave::IntoDuration;
pub use commands::PersistenceCommands;
pub use error::PersistenceError;
#[doc(hidden)]
pub use model_codec::FieldCodec;
pub use model_codec::{EncodeOutcome, ModelCodec};
pub use status::PersistenceStatus;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::TypeId;
use core::marker::PhantomData;
use target::Source;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::core::reactive::Signal;
use crate::core::storage::Storage;
use crate::ecs::World;

const VALUE_VERSION: u8 = 1;

type SaveFn = Box<dyn FnMut(&World) -> Result<Option<Vec<u8>>, PersistenceError>>;

struct TrackedItem {
    key: &'static str,
    source: Source,
    save: SaveFn,
}

impl TrackedItem {
    fn save(&mut self, world: &World, storage: &mut dyn Storage) -> Result<(), PersistenceError> {
        if let Some(bytes) = (self.save)(world)? {
            if self.source != Source::Bytes {
                write_value(storage, self.key, &bytes)?;
            } else {
                storage.write(self.key, &bytes)?;
            }
        }
        Ok(())
    }
}

pub(crate) struct PersistenceRegistry {
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
            source: Source::Signal(save_sig.source_id()),
            save: Box::new(move |_world: &World| {
                postcard::to_allocvec(&save_sig.get_untracked())
                    .map(Some)
                    .map_err(|_| PersistenceError::Encoding)
            }),
        });
        Ok(())
    }

    /// Restore and track the current resource of type `T`; missing resources are inserted on restore and skipped on save.
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
            source: Source::Resource(TypeId::of::<T>()),
            save: Box::new(|world: &World| {
                world
                    .resource::<T>()
                    .map(postcard::to_allocvec)
                    .transpose()
                    .map_err(|_| PersistenceError::Encoding)
            }),
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
            source: Source::Bytes,
            save: Box::new(move |_| Ok(save())),
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

    /// Save one non-Model registration by key.
    pub fn save(&mut self, world: &World, key: &str) -> Result<(), PersistenceError> {
        if !self.enabled {
            return Err(PersistenceError::Disabled);
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
/// - resource: internal persistence registrations
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
    pub(crate) fn install(&mut self, world: &mut World) {
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
        registry.autosave.reset(clock_ns(world));
        // No MonoClock → autosave never ticks; on_suspend/on_quit still flush.
        if registry.autosave.enabled() && world.resource::<crate::ecs::MonoClock>().is_none() {
            crate::warn!(
                target: "mirui::persistence",
                "PersistencePlugin: autosave set but no MonoClock resource installed; \
                 periodic flushes will not fire. Add a clock plugin (e.g. StdInstantClockPlugin) \
                 before this one. on_suspend / on_quit flushes still work."
            );
        }
        for register in self.pending.drain(..) {
            if let Err(error) = register(&mut registry, world) {
                crate::warn!(target: "mirui::persistence", "persistence registration failed: {error}");
            }
        }
        world.insert_resource(registry);
    }
}

pub(crate) fn flush_due(world: &mut World) {
    let now_ns = clock_ns(world);
    let due = world
        .resource_mut::<PersistenceRegistry>()
        .is_some_and(|reg| reg.enabled && reg.autosave.due(now_ns));
    if due {
        flush_and_stamp(world, now_ns);
    }
}

pub(crate) fn flush(world: &mut World) {
    let now_ns = clock_ns(world);
    flush_and_stamp(world, now_ns);
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
#[path = "../../../tests/unit/core/persistence/tests.rs"]
mod tests;
