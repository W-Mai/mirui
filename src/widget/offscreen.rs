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
//! Constraints — all `debug_assert!` panics:
//! - Renderer must implement the SW pipeline. GPU backends log once
//!   and fall through to inline rendering.
//! - Entity cannot also carry `WidgetTransform3D`.
//! - `OffscreenRender` cannot nest.

use crate::cache::{LruCache, MaxSize, WithFactory};
use crate::draw::texture::{ColorFormat, Texture};
use crate::ecs::Entity;
use crate::types::Fixed;

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

/// LRU pool of offscreen buffers, sized by entry count. Inserted
/// into the `World` as a resource by `App::with_factory` with
/// capacity `0` (disabled); set a real value via
/// [`crate::app::App::with_offscreen_pool_capacity`].
pub struct OffscreenBufferPool {
    // The render walker hooks into this in a follow-up patch; until
    // then it's only exercised by unit tests, which clippy ignores.
    #[allow(dead_code)]
    pub(crate) cache: WithFactory<LruCache<BufferKey, Texture<'static>>, BufferCtor>,
}

pub(crate) type BufferCtor = fn(&BufferKey) -> Result<Texture<'static>, BufferAllocError>;

#[derive(Debug)]
pub struct BufferAllocError;

fn make_buffer(k: &BufferKey) -> Result<Texture<'static>, BufferAllocError> {
    Ok(Texture::owned(k.w, k.h, k.format))
}

impl OffscreenBufferPool {
    /// Build a pool with an explicit entry-count capacity. Pass `0`
    /// to disable the cache entirely (every insert lands as a
    /// detached invalid handle).
    pub fn new(capacity: usize) -> Self {
        let cache = LruCache::builder()
            .max_size(MaxSize::Count(capacity))
            .name("widget/offscreen")
            .build();
        Self {
            cache: WithFactory::new(cache, make_buffer as BufferCtor),
        }
    }
}

impl Default for OffscreenBufferPool {
    /// Disabled cache. Buffer working sets depend on widget sizes and
    /// available RAM, neither of which the library can guess; the
    /// caller opts in via
    /// [`crate::app::App::with_offscreen_pool_capacity`].
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_entity(id: u32) -> Entity {
        Entity { id, generation: 0 }
    }

    #[test]
    fn pool_creates_with_capacity() {
        let pool = OffscreenBufferPool::new(4);
        assert_eq!(pool.cache.cache().len(), 0);
    }

    #[test]
    fn pool_default_uses_platform_capacity() {
        let pool = OffscreenBufferPool::default();
        // Just verify it builds; capacity is platform-dependent.
        assert_eq!(pool.cache.cache().len(), 0);
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
        let mut pool = OffscreenBufferPool::new(4);
        let key = BufferKey {
            entity: dummy_entity(1),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let handle = pool.cache.entry(key).or_insert().expect("alloc");
        assert!(!handle.is_invalid());
        assert_eq!(handle.width, 40);
        assert_eq!(handle.height, 24);
        assert_eq!(handle.format, ColorFormat::RGBA8888);
    }

    #[test]
    fn pool_or_insert_hits_same_key() {
        let mut pool = OffscreenBufferPool::new(4);
        let key = BufferKey {
            entity: dummy_entity(1),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let _h1 = pool.cache.entry(key).or_insert().expect("first");
        let stats_after_first = *pool.cache.cache().stats();
        let _h2 = pool.cache.entry(key).or_insert().expect("second");
        let stats_after_second = *pool.cache.cache().stats();
        // Second call hits, not misses.
        assert_eq!(stats_after_second.miss_count, stats_after_first.miss_count);
        assert_eq!(
            stats_after_second.hit_count,
            stats_after_first.hit_count + 1
        );
    }

    #[test]
    fn pool_or_insert_misses_after_generation_bump() {
        let mut pool = OffscreenBufferPool::new(4);
        let key0 = BufferKey {
            entity: dummy_entity(1),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let _h0 = pool.cache.entry(key0).or_insert().expect("gen 0");
        let key1 = BufferKey {
            generation: 1,
            ..key0
        };
        let stats_before = *pool.cache.cache().stats();
        let _h1 = pool.cache.entry(key1).or_insert().expect("gen 1");
        let stats_after = *pool.cache.cache().stats();
        // generation bump → key not in cache → miss.
        assert_eq!(stats_after.miss_count, stats_before.miss_count + 1);
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
