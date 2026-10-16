//! GPU side cache for `wgpu::Texture` uploads — keyed on the source
//! buffer pointer + layout, evicted by mirui's LRU cache. Static
//! assets (`IMG_THUMBS_UP` and friends) hit on every frame after the
//! first upload. Reused dynamic buffers are marked transient and bypass
//! this pointer-based cache.

use crate::core::cache::{Cache, HasSize, HashLookup, Lru, MaxSize};
use crate::render::font::scalar::ScalarSurfaceKey;
pub(super) use crate::render::texture::TextureKey;

/// Cache value newtype so `HasSize` reports real GPU bytes (RGBA8 × w
/// × h) for the byte budget — `wgpu::Texture` is upstream so we can't
/// `impl HasSize` on it directly.
pub struct CachedTexture(pub wgpu::Texture);

impl HasSize for CachedTexture {
    fn cache_size(&self) -> usize {
        let size = self.0.size();
        (size.width as usize) * (size.height as usize) * 4
    }
}

/// 16 MiB GPU texture cache. Roughly 64 ARGB icons of 256×256 each, or
/// a couple of 1024-wide screenshots; eviction is LRU once full.
pub(super) const TEXTURE_BUDGET: usize = 16 * 1024 * 1024;

pub type TexturePool = Cache<TextureKey, CachedTexture, Lru, HashLookup<TextureKey>>;

pub fn new_pool(bytes: usize) -> TexturePool {
    Cache::builder().max_size(MaxSize::Bytes(bytes)).build()
}

pub struct CachedScalarSurface(pub wgpu::Texture);

impl HasSize for CachedScalarSurface {
    fn cache_size(&self) -> usize {
        let size = self.0.size();
        (size.width as usize) * (size.height as usize)
    }
}

pub type ScalarSurfacePool =
    Cache<ScalarSurfaceKey, CachedScalarSurface, Lru, HashLookup<ScalarSurfaceKey>>;

pub fn new_scalar_surface_pool() -> ScalarSurfacePool {
    Cache::builder()
        .max_size(MaxSize::Bytes(TEXTURE_BUDGET))
        .build()
}
