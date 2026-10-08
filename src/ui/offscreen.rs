//! Offscreen render — mark an entity with [`OffscreenRender`] to send
//! its subtree through a private buffer instead of writing straight
//! into the parent renderer's target. The buffer is sized at
//! `ComputedRect × scale` (so `scale = 1.0` is a 1:1 cache and
//! `scale < 1.0` reduces resolution), rendered into, then blit'd back
//! onto the parent at the entity's full rect.
//!
//! Buffers live in [`OffscreenBufferPool`], a `World` resource keyed
//! by `(entity, w, h, format, generation)`. The dirty walker bumps
//! `generation` whenever the entity itself or any descendant carries
//! `Dirty`, which forces the next render to miss the cache and
//! rebuild the buffer; otherwise the prior buffer is blit'd as-is and
//! the subtree's raster work is skipped entirely.
//!
//! # Usage
//!
//! Two pieces opt the app in:
//!
//! ```ignore
//! use mirui::prelude::*;
//! use mirui::ui::OffscreenRender;
//!
//! #[compose]
//! fn cached_panel() {
//!     ui! { View (width: 128, height: 64) [OffscreenRender::new()] {} };
//! }
//!
//! let mut app = App::headless(128, 64);
//! app.with_default_widgets();
//! let root = app.spawn_root().id();
//! // 1. Size the pool. Default is `Bytes(0)` (disabled), so the cache
//! //    never grows and OffscreenRender silently falls through to
//! //    inline. Pick a budget that fits a couple of buffers:
//! //    `width × height × bytes_per_pixel`. RGB565 is 2, RGBA8888 is 4.
//! app.with_offscreen_pool_budget(64 * 1024);
//!
//! // 2. Compose the node carrying the offscreen marker.
//! app.compose(root, cached_panel());
//! ```
//!
//! # When it pays off
//!
//! Best fit: a subtree that is **static or near-static between frames**
//! while something else on screen redraws (forces the dirty rect to
//! cover the cached entity). Inline path re-rasters the subtree;
//! offscreen path blit's the cached buffer once.
//!
//! Worst fit: the subtree mutates every frame, or a `WidgetTransform`
//! on the entity animates while the subtree is static. Both cases
//! bump `generation` (or fail to skip raster) so the offscreen path
//! runs the full raster + blit each frame and ends up slower than
//! inline.
//!
//! # Constraints (debug_assert panics)
//!
//! - Renderer must implement the SW pipeline. GPU backends log once
//!   and fall through to inline rendering.
//! - Entity cannot also carry `WidgetTransform3D`.
//! - `OffscreenRender` cannot nest.

use crate::core::cache::{Handle, LruCache, MaxSize, WithFactory};
use crate::ecs::{Entity, World};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::Fixed;
use core::cell::{Ref, RefCell};

/// Mark an entity for offscreen rendering. Insert / remove to toggle.
///
/// Default ([`Self::new`]) renders at full resolution and only buys
/// caching. `with_scale(Fixed::HALF)` renders at half resolution and
/// upscales on present. `with_opacity(128)` blends the cached subtree
/// onto the outer surface at the chosen alpha.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct OffscreenRender {
    /// Render scale relative to the entity's `ComputedRect`. 1.0 keeps
    /// the buffer at the entity's drawn size; 0.5 halves both axes (a
    /// quarter of the pixel count). Values below `Fixed::ONE / 8` are
    /// clamped at render time so `buf_w` / `buf_h` never round to 0.
    pub scale: Fixed,
    /// Group opacity applied when the cached buffer is blitted onto the
    /// outer surface (not a per-pixel filter inside the subtree).
    pub opacity: u8,
}

impl crate::ecs::Component for OffscreenRender {}

impl Default for OffscreenRender {
    fn default() -> Self {
        Self::new()
    }
}

impl OffscreenRender {
    pub const fn new() -> Self {
        Self {
            scale: Fixed::ONE,
            opacity: 255,
        }
    }

    pub const fn with_scale(scale: Fixed) -> Self {
        Self {
            scale,
            opacity: 255,
        }
    }

    pub const fn with_opacity(mut self, opacity: u8) -> Self {
        self.opacity = opacity;
        self
    }
}

/// Cache-invalidation counter. Bumped by the dirty walker when any
/// descendant of an `OffscreenRender` entity carries `Dirty`. Lives on
/// the same entity as `OffscreenRender`; default 0 on first render.
#[derive(Clone, Copy, Debug, Default)]
pub struct OffscreenGeneration(pub u32);

impl crate::ecs::Component for OffscreenGeneration {}

/// Mark on a consumer entity to keep `source`'s OffscreenRender
/// alive. mirui maintains a refcount per source — first ref adds
/// OffscreenRender to source; the last ref removed restores source
/// to its prior state (no OffscreenRender if the user hadn't opted
/// in).
///
/// Effect widgets typically attach this via the view registry's
/// `auto_attach` mechanism, so user code only inserts the effect's
/// main component.
///
/// Maintenance invalidates a consumer on its first reference, a change of source entity or pixel generation, or source dirtiness. Source identity includes the entity generation. A reference absent during maintenance loses its tracking state; reattaching it afterward starts a new observation. Removal and reinsertion before maintenance are coalesced into the final reference.
#[derive(Clone, Copy, Debug)]
pub struct WidgetTextureRef(pub Entity);

impl crate::ecs::Component for WidgetTextureRef {}

/// Internal marker on a source entity that received `OffscreenRender`
/// from `maintain_widget_texture_refs` (not from user code). Only
/// auto-added entries get removed when the last `WidgetTextureRef`
/// goes away; user-explicit `OffscreenRender` is left alone.
#[derive(Clone, Copy, Debug)]
pub struct OffscreenAutoAdded;

impl crate::ecs::Component for OffscreenAutoAdded {}

/// Mark on an `OffscreenRender` source so its buffer initialises to
/// fully-transparent black instead of a copy of the framebuffer
/// underneath. Effect widgets that need the buffer's alpha channel
/// to encode the source's actual silhouette (zero outside the
/// widget's drawn pixels) attach this — without it, alpha
/// extraction sees the framebuffer's alpha bleeding through and
/// produces a rectangular silhouette that ignores `border_radius`.
#[derive(Clone, Copy, Debug)]
pub struct OffscreenAlphaMode {
    /// `true` ⇒ buffer is cleared to RGBA `(0, 0, 0, 0)` before the
    /// subtree renders. `false` ⇒ buffer is pre-seeded from the
    /// framebuffer (default — keeps anti-aliased edges blending against
    /// the existing background).
    pub clear_transparent: bool,
}

impl OffscreenAlphaMode {
    pub const fn clear_transparent() -> Self {
        Self {
            clear_transparent: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WidgetTextureRefState {
    source: Entity,
    generation: u32,
}

impl crate::ecs::Component for WidgetTextureRefState {}

/// Cache key for [`OffscreenBufferPool`]. `entity` is part of the key so
/// each offscreen entity owns its own slot — sharing buffers across
/// entities would race when both render in the same frame.
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) struct BufferKey {
    pub entity: Entity,
    pub w: u16,
    pub h: u16,
    pub format: ColorFormat,
    pub generation: u32,
}

impl BufferKey {
    pub(crate) fn for_widget(
        entity: Entity,
        rect: crate::types::Rect,
        offscreen: OffscreenRender,
        generation: u32,
        backend_format: ColorFormat,
        clear_transparent: bool,
    ) -> Self {
        let scale = offscreen.scale.max(Fixed::ONE / 8);
        let width = Fixed::from_int(rect.w.to_int().max(1)) * scale;
        let height = Fixed::from_int(rect.h.to_int().max(1)) * scale;
        Self {
            entity,
            w: width.to_int().max(1).min(u16::MAX as i32) as u16,
            h: height.to_int().max(1).min(u16::MAX as i32) as u16,
            format: if clear_transparent {
                ColorFormat::RGBA8888
            } else {
                backend_format
            },
            generation,
        }
    }
}

/// LRU pool limited by the bytes held by its cached texture entries rather than their count. Evicted payloads retained by external snapshots are no longer counted against this cache budget. `App::with_factory` installs a zero-budget pool, disabling caching; configure it through [`crate::app::App::with_offscreen_pool_budget`].
pub struct OffscreenBufferPool {
    // RefCell so render_system can borrow the pool mutably while
    // holding `&World`. Each cached value is itself a RefCell<Texture>
    // because the inner SwRenderer borrows the buffer's bytes mutably
    // for the duration of the subtree render.
    pub(crate) cache:
        RefCell<WithFactory<LruCache<BufferKey, RefCell<Texture<'static>>>, BufferCtor>>,
    next_cache_revision: core::cell::Cell<u64>,
    // Parent target format; transparent sources select RGBA independently.
    pub(crate) backend_format: core::cell::Cell<Option<ColorFormat>>,
}

pub(crate) type BufferCtor = fn(&BufferKey) -> Result<RefCell<Texture<'static>>, BufferAllocError>;

#[derive(Debug)]
pub struct BufferAllocError;

fn make_buffer(k: &BufferKey) -> Result<RefCell<Texture<'static>>, BufferAllocError> {
    Ok(RefCell::new(Texture::owned(k.w, k.h, k.format)))
}

impl OffscreenBufferPool {
    /// Build a pool with an explicit byte budget. The budget caps the
    /// total Texture heap held by the cache; LRU eviction kicks in
    /// once an insert would push the running total past the budget.
    pub fn with_budget(budget_bytes: usize) -> Self {
        let cache = LruCache::builder()
            .max_size(MaxSize::Bytes(budget_bytes))
            .name("widget/offscreen")
            .build();
        Self {
            cache: RefCell::new(WithFactory::new(cache, make_buffer as BufferCtor)),
            next_cache_revision: core::cell::Cell::new(1),
            backend_format: core::cell::Cell::new(None),
        }
    }

    pub(crate) fn allocate_cache_revision(&self) -> Option<u64> {
        let revision = self.next_cache_revision.get();
        self.next_cache_revision.set(revision.checked_add(1)?);
        Some(revision)
    }

    pub(crate) fn trim_memory(&self) {
        self.cache.borrow_mut().cache_mut().clear();
        self.backend_format.set(None);
    }
}

impl Default for OffscreenBufferPool {
    /// Disabled cache. Buffer working sets depend on widget sizes and
    /// available RAM, neither of which the library can guess; the
    /// caller opts in via
    /// [`crate::app::App::with_offscreen_pool_budget`].
    fn default() -> Self {
        Self::with_budget(0)
    }
}

/// Retains a cached texture payload even if its entry is evicted from [`OffscreenBufferPool`]. A snapshot does not pin cache membership or guarantee that a later lookup finds the same generation.
#[derive(Clone)]
pub struct TextureSnapshot {
    handle: Handle<RefCell<Texture<'static>>>,
}

impl TextureSnapshot {
    /// Borrow the underlying texture immutably.
    ///
    /// Panics if this payload is currently borrowed mutably.
    pub fn borrow(&self) -> Ref<'_, Texture<'static>> {
        self.handle.get().borrow()
    }
}

fn texture_for(world: &World, entity: Entity, generation_offset: i64) -> Option<TextureSnapshot> {
    let pool = world.resource::<OffscreenBufferPool>()?;
    let backend_format = pool.backend_format.get()?;
    let off = world.get::<OffscreenRender>(entity)?;
    let rect = world.get::<super::ComputedRect>(entity)?.0;
    let clear_transparent = world
        .get::<OffscreenAlphaMode>(entity)
        .is_some_and(|mode| mode.clear_transparent);

    let g_now = world
        .get::<OffscreenGeneration>(entity)
        .map(|g| g.0)
        .unwrap_or(0);
    let g = if generation_offset >= 0 {
        g_now.checked_add(generation_offset as u32)?
    } else {
        g_now.checked_sub((-generation_offset) as u32)?
    };

    let key = BufferKey::for_widget(entity, rect, *off, g, backend_format, clear_transparent);
    let handle = pool.cache.borrow_mut().acquire(&key)?;
    Some(TextureSnapshot { handle })
}

/// Extension trait that lives on `World` for ergonomic access to
/// rendered widget textures from inside an effect widget's view fn.
pub trait WidgetTextureAccess {
    /// The entity's cached current generation at its current dimensions and format. Returns `None` when it has not been rendered, the entry has been evicted, or its configuration no longer matches the cached key.
    fn texture_of(&self, entity: Entity) -> Option<TextureSnapshot>;

    /// The preceding cached generation at the entity's current dimensions and format. It is not necessarily the previous display frame. Returns `None` before a preceding generation exists or when that key is absent from the cache.
    fn prev_texture_of(&self, entity: Entity) -> Option<TextureSnapshot>;
}

impl WidgetTextureAccess for World {
    fn texture_of(&self, entity: Entity) -> Option<TextureSnapshot> {
        texture_for(self, entity, 0)
    }

    fn prev_texture_of(&self, entity: Entity) -> Option<TextureSnapshot> {
        texture_for(self, entity, -1)
    }
}

/// Walk every `WidgetTextureRef` and reconcile each referenced
/// source's `OffscreenRender` state. Auto-add when a source gains
/// its first ref; auto-remove when the last ref drops.
pub fn maintain_widget_texture_refs(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    use alloc::vec::Vec;
    use hashbrown::HashMap;

    let mut state_index = 0;
    while let Some(consumer) = world
        .storage::<WidgetTextureRefState>()
        .and_then(|storage| storage.entities().get(state_index))
        .copied()
    {
        if world.has::<WidgetTextureRef>(consumer) {
            state_index += 1;
        } else {
            // Sparse removal moves the last entry into the current index.
            world.remove::<WidgetTextureRefState>(consumer);
        }
    }

    // Cheap path: if neither component is in use, the system has
    // nothing to do — common until any effect widget gets attached.
    let any_ref = world.query::<WidgetTextureRef>().iter().next().is_some();
    let any_auto = world.query::<OffscreenAutoAdded>().iter().next().is_some();
    if !any_ref && !any_auto {
        return Ok(());
    }

    let mut counts: HashMap<Entity, u32> = HashMap::new();
    for (_e, r) in world.query::<WidgetTextureRef>().iter() {
        *counts.entry(r.0).or_insert(0) += 1;
    }

    let mut to_add: Vec<Entity> = Vec::new();
    for (&source, &n) in &counts {
        if n > 0 && world.get::<OffscreenRender>(source).is_none() {
            to_add.push(source);
        }
    }
    for source in to_add {
        world.insert(source, OffscreenRender::default());
        world.insert(source, OffscreenAutoAdded);
    }

    let auto_entries: Vec<Entity> = world
        .query::<OffscreenAutoAdded>()
        .iter()
        .map(|(e, _)| e)
        .collect();
    for source in auto_entries {
        // Keep self-attached entries: an effect widget that holds a
        // `WidgetTextureRef` may also need its own `OffscreenRender`
        // (e.g. TemporalMix uses its own buffer for IIR feedback).
        // Such an entity isn't anyone's source, so the refcount check
        // alone would tear its buffer down.
        if counts.get(&source).copied().unwrap_or(0) == 0
            && world.get::<WidgetTextureRef>(source).is_none()
        {
            world.remove::<OffscreenRender>(source);
            world.remove::<OffscreenAutoAdded>(source);
        }
    }

    let pairs: Vec<(Entity, Entity)> = world
        .query::<WidgetTextureRef>()
        .iter()
        .map(|(e, r)| (e, r.0))
        .collect();
    for (consumer, source) in pairs {
        let g_now = world
            .get::<OffscreenGeneration>(source)
            .map(|g| g.0)
            .unwrap_or(0);
        let state = WidgetTextureRefState {
            source,
            generation: g_now,
        };
        let changed = world.get::<WidgetTextureRefState>(consumer) != Some(&state);
        let source_dirty = world.get::<super::dirty::Dirty>(source).is_some();
        let source_transform = world
            .get::<crate::ui::widgets::WidgetTransform>(source)
            .copied();
        // Source movement can change composed geometry without changing pixel generation.
        if changed || source_dirty {
            world.insert(consumer, super::dirty::Dirty);
            world.insert(consumer, state);
            // Mirror the source's WidgetTransform onto the consumer
            // so the dirty walker computes the consumer's screen
            // bbox at the source's actual painted position, not at
            // the consumer's static layout slot. Without this the
            // dirty rect doesn't track the source's animation and
            // old shadow / mirror pixels stick around as the source
            // moves.
            if let Some(tf) = source_transform {
                world.insert(consumer, tf);
            } else {
                world.remove::<crate::ui::widgets::WidgetTransform>(consumer);
            }
        }
    }
    Ok(())
}

crate::ecs::system::framework_system!(
    pub maintain_widget_texture_refs,
    PreRender,
    expect = [WidgetTextureRef, OffscreenAutoAdded, WidgetTextureRefState],
);

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_entity(id: u32) -> Entity {
        Entity { id, generation: 0 }
    }

    #[test]
    fn content_revisions_advance_and_overflow_disables_caching() {
        let pool = OffscreenBufferPool::with_budget(1024);
        assert_eq!(pool.allocate_cache_revision(), Some(1));
        assert_eq!(pool.allocate_cache_revision(), Some(2));
        pool.next_cache_revision.set(u64::MAX);
        assert_eq!(pool.allocate_cache_revision(), None);
    }

    #[test]
    fn offscreen_render_default_is_fully_opaque() {
        assert_eq!(OffscreenRender::default().opacity, 255);
        assert_eq!(OffscreenRender::new().opacity, 255);
        assert_eq!(OffscreenRender::with_scale(Fixed::HALF).opacity, 255);
    }

    #[test]
    fn offscreen_render_with_opacity_chains() {
        let off = OffscreenRender::new().with_opacity(128);
        assert_eq!(off.scale, Fixed::ONE);
        assert_eq!(off.opacity, 128);

        let off2 = OffscreenRender::with_scale(Fixed::HALF).with_opacity(64);
        assert_eq!(off2.scale, Fixed::HALF);
        assert_eq!(off2.opacity, 64);
    }

    #[test]
    fn pool_creates_with_byte_budget() {
        let pool = OffscreenBufferPool::with_budget(64 * 1024);
        assert_eq!(pool.cache.borrow().cache().len(), 0);
    }

    #[test]
    fn pool_default_disables_cache() {
        // No platform sniffing in the default — caller must opt in via
        // `App::with_offscreen_pool_budget`. Until they do, every
        // insert lands as a detached invalid handle and the cache
        // stays empty.
        let pool = OffscreenBufferPool::default();
        let key = BufferKey {
            entity: dummy_entity(1),
            w: 32,
            h: 32,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let handle = pool
            .cache
            .borrow_mut()
            .entry(key)
            .or_insert()
            .expect("ctor still runs even when the budget is zero");
        assert!(
            handle.is_invalid(),
            "Bytes(0) must hand back a detached handle"
        );
        assert_eq!(pool.cache.borrow().cache().len(), 0);
    }

    #[test]
    fn buffer_key_distinguishes_entities() {
        let k1 = BufferKey {
            entity: dummy_entity(1),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let k2 = BufferKey {
            entity: dummy_entity(2),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        assert_ne!(k1, k2);
    }

    #[test]
    fn buffer_key_distinguishes_generations() {
        let k1 = BufferKey {
            entity: dummy_entity(1),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let k2 = BufferKey {
            generation: 1,
            ..k1
        };
        assert_ne!(k1, k2);
    }

    #[test]
    fn pool_or_insert_creates_buffer_at_requested_size() {
        let pool = OffscreenBufferPool::with_budget(64 * 1024);
        let key = BufferKey {
            entity: dummy_entity(1),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let handle = pool
            .cache
            .borrow_mut()
            .entry(key)
            .or_insert()
            .expect("alloc");
        assert!(!handle.is_invalid());
        let tex = handle.borrow();
        assert_eq!(tex.width, 40);
        assert_eq!(tex.height, 24);
        assert_eq!(tex.format, ColorFormat::RGBA8888);
    }

    #[test]
    fn pool_or_insert_hits_same_key() {
        let pool = OffscreenBufferPool::with_budget(64 * 1024);
        let key = BufferKey {
            entity: dummy_entity(1),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let _h1 = pool
            .cache
            .borrow_mut()
            .entry(key)
            .or_insert()
            .expect("first");
        let stats_after_first = *pool.cache.borrow().cache().stats();
        let _h2 = pool
            .cache
            .borrow_mut()
            .entry(key)
            .or_insert()
            .expect("second");
        let stats_after_second = *pool.cache.borrow().cache().stats();
        // Second call hits, not misses.
        assert_eq!(stats_after_second.miss_count, stats_after_first.miss_count);
        assert_eq!(
            stats_after_second.hit_count,
            stats_after_first.hit_count + 1
        );
    }

    #[test]
    fn pool_or_insert_misses_after_generation_bump() {
        let pool = OffscreenBufferPool::with_budget(64 * 1024);
        let key0 = BufferKey {
            entity: dummy_entity(1),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let _h0 = pool
            .cache
            .borrow_mut()
            .entry(key0)
            .or_insert()
            .expect("gen 0");
        let key1 = BufferKey {
            generation: 1,
            ..key0
        };
        let stats_before = *pool.cache.borrow().cache().stats();
        let _h1 = pool
            .cache
            .borrow_mut()
            .entry(key1)
            .or_insert()
            .expect("gen 1");
        let stats_after = *pool.cache.borrow().cache().stats();
        // generation bump → key not in cache → miss.
        assert_eq!(stats_after.miss_count, stats_before.miss_count + 1);
    }

    #[test]
    fn pool_byte_budget_evicts_lru_when_total_exceeds_limit() {
        // Budget = 8 KB; one 40×24 RGBA buffer is 3840 bytes. Three of
        // them (11.5 KB) won't fit, so the LRU one must leave.
        let pool = OffscreenBufferPool::with_budget(8 * 1024);
        let key = |id, g| BufferKey {
            entity: dummy_entity(id),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: g,
        };
        let _h1 = pool.cache.borrow_mut().entry(key(1, 0)).or_insert();
        let _h2 = pool.cache.borrow_mut().entry(key(2, 0)).or_insert();
        // Touch h1 so h2 is the LRU candidate.
        let _ = pool.cache.borrow_mut().acquire(&key(1, 0));
        let _h3 = pool.cache.borrow_mut().entry(key(3, 0)).or_insert();

        let cache = pool.cache.borrow();
        assert_eq!(cache.cache().len(), 2);
        assert!(cache.cache().current_size() <= 8 * 1024);
        assert_eq!(cache.cache().stats().evict_count, 1);
    }

    #[test]
    fn pool_oversized_entry_returns_invalid_handle_without_growing_cache() {
        // 200×200 RGBA = 160 KB, way past a 4 KB budget.
        let pool = OffscreenBufferPool::with_budget(4 * 1024);
        let key = BufferKey {
            entity: dummy_entity(1),
            w: 200,
            h: 200,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let handle = pool
            .cache
            .borrow_mut()
            .entry(key)
            .or_insert()
            .expect("ctor still runs even when entry won't fit");
        assert!(
            handle.is_invalid(),
            "oversized entry must come back detached"
        );
        assert_eq!(pool.cache.borrow().cache().len(), 0);
    }

    #[test]
    fn offscreen_render_default_is_full_scale() {
        let off = OffscreenRender::default();
        assert_eq!(off.scale, Fixed::ONE);
    }

    #[test]
    fn offscreen_render_with_scale() {
        let off = OffscreenRender::with_scale(Fixed::ONE / 2);
        assert_eq!(off.scale, Fixed::ONE / 2);
    }

    fn run_refs(world: &mut World) {
        super::maintain_widget_texture_refs(world).unwrap();
    }

    #[test]
    fn first_ref_auto_adds_offscreen_render() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let consumer = world.spawn_empty();
        world.insert(consumer, WidgetTextureRef(source));

        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_some());
        assert!(world.get::<OffscreenAutoAdded>(source).is_some());
    }

    #[test]
    fn last_ref_dropped_auto_removes_offscreen_render() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let c1 = world.spawn_empty();
        let c2 = world.spawn_empty();
        world.insert(c1, WidgetTextureRef(source));
        world.insert(c2, WidgetTextureRef(source));
        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_some());

        world.remove::<WidgetTextureRef>(c1);
        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_some());

        world.remove::<WidgetTextureRef>(c2);
        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_none());
        assert!(world.get::<OffscreenAutoAdded>(source).is_none());
    }

    #[test]
    fn user_explicit_offscreen_render_is_never_removed() {
        let mut world = World::default();
        let source = world.spawn_empty();
        // User-explicit: no `OffscreenAutoAdded` marker.
        world.insert(source, OffscreenRender::default());

        let consumer = world.spawn_empty();
        world.insert(consumer, WidgetTextureRef(source));
        run_refs(&mut world);
        assert!(world.get::<OffscreenAutoAdded>(source).is_none());

        world.remove::<WidgetTextureRef>(consumer);
        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_some());
    }

    #[test]
    fn texture_reference_identity_drives_invalidation_and_transform() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::WidgetTransform;

        for generation in [0, 7] {
            let mut world = World::default();
            let first = world.spawn_empty();
            let second = world.spawn_empty();
            let consumer = world.spawn_empty();
            let first_transform = Transform::translate(3.into(), 5.into());
            let second_transform = Transform::translate(11.into(), 13.into());
            world.insert(first, OffscreenGeneration(generation));
            world.insert(second, OffscreenGeneration(generation));
            world.insert(first, WidgetTransform(first_transform));
            world.insert(second, WidgetTransform(second_transform));
            world.insert(consumer, WidgetTextureRef(first));

            run_refs(&mut world);
            assert!(world.has::<Dirty>(consumer));
            assert_eq!(
                world.get::<WidgetTransform>(consumer).unwrap().0,
                first_transform
            );
            world.remove::<Dirty>(consumer);
            run_refs(&mut world);
            assert!(!world.has::<Dirty>(consumer));

            world.insert(consumer, WidgetTextureRef(second));
            run_refs(&mut world);
            assert!(world.has::<Dirty>(consumer));
            assert_eq!(
                world.get::<WidgetTransform>(consumer).unwrap().0,
                second_transform
            );
            assert!(!world.has::<OffscreenRender>(first));
            assert!(world.has::<OffscreenRender>(second));
            world.remove::<Dirty>(consumer);
            run_refs(&mut world);
            assert!(!world.has::<Dirty>(consumer));

            world.insert(consumer, WidgetTextureRef(first));
            world.remove::<WidgetTransform>(first);
            run_refs(&mut world);
            assert!(world.has::<Dirty>(consumer));
            assert!(!world.has::<WidgetTransform>(consumer));
        }
    }

    #[test]
    fn retired_reference_state_is_reclaimed_without_allocating() {
        use crate::test_support::tracked_allocations;
        use crate::ui::dirty::Dirty;

        let mut world = World::default();
        let source = world.spawn_empty();
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        world.insert(source, OffscreenRender::default());
        world.insert(first, WidgetTextureRef(source));
        world.insert(second, WidgetTextureRef(source));
        run_refs(&mut world);
        world.remove::<Dirty>(first);
        world.remove::<Dirty>(second);
        world.remove::<WidgetTextureRef>(first);
        world.remove::<WidgetTextureRef>(second);

        let allocations = tracked_allocations(|| run_refs(&mut world));
        assert_eq!(allocations, 0);
        assert!(!world.has::<WidgetTextureRefState>(first));
        assert!(!world.has::<WidgetTextureRefState>(second));
        assert!(world.has::<OffscreenRender>(source));

        world.insert(first, WidgetTextureRef(source));
        run_refs(&mut world);
        assert!(world.has::<Dirty>(first));
    }

    #[test]
    fn recycled_source_id_has_a_distinct_texture_reference_identity() {
        use crate::ui::dirty::Dirty;

        let mut world = World::default();
        let source = world.spawn_empty();
        let consumer = world.spawn_empty();
        world.insert(consumer, WidgetTextureRef(source));
        run_refs(&mut world);
        world.remove::<Dirty>(consumer);
        assert!(world.despawn(source));
        let replacement = world.spawn_empty();
        assert_eq!(source.id, replacement.id);
        assert_ne!(source.generation, replacement.generation);

        world.insert(consumer, WidgetTextureRef(replacement));
        run_refs(&mut world);
        assert!(world.has::<Dirty>(consumer));
        assert_eq!(
            world.get::<WidgetTextureRefState>(consumer).unwrap().source,
            replacement
        );
    }

    #[test]
    fn unchanged_texture_revision_propagates_dirty_source_transform() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::WidgetTransform;

        let mut world = World::default();
        let source = world.spawn_empty();
        let consumer = world.spawn_empty();
        world.insert(consumer, WidgetTextureRef(source));
        run_refs(&mut world);
        world.remove::<Dirty>(consumer);
        let transform = Transform::translate(17.into(), 19.into());
        world.insert(source, WidgetTransform(transform));
        world.insert(source, Dirty);

        run_refs(&mut world);
        assert!(world.has::<Dirty>(consumer));
        assert_eq!(world.get::<WidgetTransform>(consumer).unwrap().0, transform);
    }
}
