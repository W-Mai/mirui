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

use crate::cache::{LruCache, MaxSize, WithFactory};
use crate::draw::texture::{ColorFormat, Texture};
use crate::ecs::Entity;
use crate::types::Fixed;
use core::cell::RefCell;

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
}
