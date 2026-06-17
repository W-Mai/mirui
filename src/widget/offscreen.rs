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
//! // 1. Size the pool. Default is `Bytes(0)` (disabled), so the cache
//! //    never grows and OffscreenRender silently falls through to
//! //    inline. Pick a budget that fits a couple of buffers:
//! //    `width × height × bytes_per_pixel`. RGB565 is 2, RGBA8888 is 4.
//! app.with_offscreen_pool_budget(64 * 1024);
//!
//! // 2. Tag the entity whose subtree should be cached.
//! world.insert(panel, OffscreenRender::default());
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

use crate::cache::{Handle, LruCache, MaxSize, WithFactory};
use crate::ecs::{Entity, World};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::Fixed;
use core::cell::{Ref, RefCell};

/// Mark an entity for offscreen rendering. Insert / remove to toggle.
///
/// Default ([`Self::new`]) renders at full resolution and only buys
/// caching. `with_scale(Fixed::HALF)` renders at half resolution and
/// upscales on present.
#[derive(Clone, Copy, Debug)]
pub struct OffscreenRender {
    /// Render scale relative to the entity's `ComputedRect`. 1.0 keeps
    /// the buffer at the entity's drawn size; 0.5 halves both axes (a
    /// quarter of the pixel count). Values below `Fixed::ONE / 8` are
    /// clamped at render time so `buf_w` / `buf_h` never round to 0.
    pub scale: Fixed,
}

impl Default for OffscreenRender {
    fn default() -> Self {
        Self::new()
    }
}

impl OffscreenRender {
    pub const fn new() -> Self {
        Self { scale: Fixed::ONE }
    }

    pub const fn with_scale(scale: Fixed) -> Self {
        Self { scale }
    }
}

/// Cache-invalidation counter. Bumped by the dirty walker when any
/// descendant of an `OffscreenRender` entity carries `Dirty`. Lives on
/// the same entity as `OffscreenRender`; default 0 on first render.
#[derive(Clone, Copy, Debug, Default)]
pub struct OffscreenGeneration(pub u32);

/// Mark on a consumer entity to keep `source`'s OffscreenRender
/// alive. mirui maintains a refcount per source — first ref adds
/// OffscreenRender to source; the last ref removed restores source
/// to its prior state (no OffscreenRender if the user hadn't opted
/// in).
///
/// Effect widgets typically attach this via the view registry's
/// `auto_attach` mechanism, so user code only inserts the effect's
/// main component.
#[derive(Clone, Copy, Debug)]
pub struct WidgetTextureRef(pub Entity);

/// Internal marker on a source entity that received `OffscreenRender`
/// from `maintain_widget_texture_refs` (not from user code). Only
/// auto-added entries get removed when the last `WidgetTextureRef`
/// goes away; user-explicit `OffscreenRender` is left alone.
#[derive(Clone, Copy, Debug)]
pub struct OffscreenAutoAdded;

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

/// Internal marker on a consumer entity tracking the source's last-
/// seen `OffscreenGeneration`, so the consumer can be marked Dirty
/// when the source's buffer changes.
#[derive(Clone, Copy, Debug, Default)]
pub struct WidgetTextureRefPrevGen(pub u32);

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

/// LRU pool of offscreen buffers, sized by total byte budget rather
/// than entry count: the cap reflects the heap held by all live
/// Texture allocations, not just the slot count. Inserted into the
/// `World` as a resource by `App::with_factory` with budget `0`,
/// which disables the cache and routes every `OffscreenRender` entity
/// through inline rendering; set a real value via
/// [`crate::app::App::with_offscreen_pool_budget`].
pub struct OffscreenBufferPool {
    // RefCell so render_system can borrow the pool mutably while
    // holding `&World`. Each cached value is itself a RefCell<Texture>
    // because the inner SwRenderer borrows the buffer's bytes mutably
    // for the duration of the subtree render.
    pub(crate) cache:
        RefCell<WithFactory<LruCache<BufferKey, RefCell<Texture<'static>>>, BufferCtor>>,
    // Format used by the most recent buffer write. `None` until the
    // first render. Effect widgets / `World::texture_of` read this to
    // reconstruct the BufferKey without holding a Renderer reference.
    pub(crate) last_format: core::cell::Cell<Option<ColorFormat>>,
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
            last_format: core::cell::Cell::new(None),
        }
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

/// Borrow guard for an entity's rendered texture cached in the
/// [`OffscreenBufferPool`]. Holding it keeps the buffer alive in the
/// pool (it counts as a live reference for LRU eviction). Drop
/// before the next render so the cache can advance generations
/// normally.
#[derive(Clone)]
pub struct TextureSnapshot {
    handle: Handle<RefCell<Texture<'static>>>,
}

impl TextureSnapshot {
    /// Borrow the underlying texture immutably.
    ///
    /// Panics if the texture is concurrently borrowed mutably (this
    /// only happens if user code retains a snapshot across the next
    /// render — drop snapshots before the frame ends).
    pub fn borrow(&self) -> Ref<'_, Texture<'static>> {
        self.handle.get().borrow()
    }
}

fn texture_for(world: &World, entity: Entity, generation_offset: i64) -> Option<TextureSnapshot> {
    let pool = world.resource::<OffscreenBufferPool>()?;
    let format = pool.last_format.get()?;
    let off = world.get::<OffscreenRender>(entity)?;
    let rect = world.get::<super::ComputedRect>(entity)?.0;
    let scale = off.scale.max(Fixed::ONE / 8);
    let buf_w_f = Fixed::from_int(rect.w.to_int().max(1)) * scale;
    let buf_h_f = Fixed::from_int(rect.h.to_int().max(1)) * scale;
    let w = buf_w_f.to_int().max(1).min(u16::MAX as i32) as u16;
    let h = buf_h_f.to_int().max(1).min(u16::MAX as i32) as u16;

    let g_now = world
        .get::<OffscreenGeneration>(entity)
        .map(|g| g.0)
        .unwrap_or(0);
    let g = if generation_offset >= 0 {
        g_now.checked_add(generation_offset as u32)?
    } else {
        g_now.checked_sub((-generation_offset) as u32)?
    };

    let key = BufferKey {
        entity,
        w,
        h,
        format,
        generation: g,
    };
    let handle = pool.cache.borrow_mut().acquire(&key)?;
    Some(TextureSnapshot { handle })
}

/// Extension trait that lives on `World` for ergonomic access to
/// rendered widget textures from inside an effect widget's view fn.
pub trait WidgetTextureAccess {
    /// The entity's rendered texture from the current frame.
    /// Returns `None` until at least one render happens after the
    /// source got `OffscreenRender` (user-explicit or auto-added via
    /// [`WidgetTextureRef`]).
    fn texture_of(&self, entity: Entity) -> Option<TextureSnapshot>;

    /// The entity's rendered texture from the previous frame. For
    /// effects that mix two frames (TemporalMix) or run before the
    /// source in z-order.
    ///
    /// Returns `None` on the first frame after opt-in (no prev
    /// buffer yet) or when the prev buffer has been evicted.
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
#[crate::system(order = PRE_RENDER, expect = [WidgetTextureRef, OffscreenAutoAdded])]
pub fn maintain_widget_texture_refs(world: &mut World) {
    use alloc::vec::Vec;
    use hashbrown::HashMap;

    // Cheap path: if neither component is in use, the system has
    // nothing to do — common until any effect widget gets attached.
    let any_ref = world.query::<WidgetTextureRef>().iter().next().is_some();
    let any_auto = world.query::<OffscreenAutoAdded>().iter().next().is_some();
    if !any_ref && !any_auto {
        return;
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

    // Generation-change detection: mark consumer Dirty when source's
    // OffscreenGeneration moved since last frame, so the consumer's
    // view fn re-runs against the fresh source texture.
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
        let g_prev = world
            .get::<WidgetTextureRefPrevGen>(consumer)
            .map(|g| g.0)
            .unwrap_or(0);
        let source_dirty = world.get::<super::dirty::Dirty>(source).is_some();
        let source_transform = world
            .get::<crate::components::WidgetTransform>(source)
            .copied();
        if g_now != g_prev || source_dirty {
            // `g_now != g_prev` catches buffer-content changes; the
            // `source_dirty` clause catches translation / rotation
            // animations on the source — those re-render at a new
            // screen position without bumping the buffer's
            // generation, so consumers that compose the source's
            // transform onto their blit need to repaint even when
            // the source's pixels haven't changed.
            world.insert(consumer, super::dirty::Dirty);
            world.insert(consumer, WidgetTextureRefPrevGen(g_now));
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
                world.remove::<crate::components::WidgetTransform>(consumer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_entity(id: u32) -> Entity {
        Entity { id, generation: 0 }
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
        super::maintain_widget_texture_refs(world);
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
}
