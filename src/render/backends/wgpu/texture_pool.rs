//! GPU side cache for `wgpu::Texture` uploads — keyed on the source
//! buffer pointer + dimensions, evicted by mirui's LRU cache. Static
//! assets (`IMG_THUMBS_UP` and friends) hit on every frame after the
//! first upload; dynamic buffers naturally miss every frame and fall
//! back to re-upload, same cost as the uncached path.

use crate::core::cache::{Cache, HasSize, HashLookup, Lru, MaxSize};
use crate::render::texture::{ColorFormat, Texture};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureKey {
    ptr: usize,
    len: usize,
    width: u16,
    height: u16,
    format: ColorFormat,
}

impl TextureKey {
    pub fn from(src: &Texture) -> Self {
        let buf = src.buf.as_slice();
        Self {
            ptr: buf.as_ptr() as usize,
            len: buf.len(),
            width: src.width,
            height: src.height,
            format: src.format,
        }
    }
}

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
const TEXTURE_BUDGET: usize = 16 * 1024 * 1024;

pub type TexturePool = Cache<TextureKey, CachedTexture, Lru, HashLookup<TextureKey>>;

pub fn new_pool() -> TexturePool {
    Cache::builder()
        .max_size(MaxSize::Bytes(TEXTURE_BUDGET))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_collides_when_buffer_view_matches() {
        let buf = [0u8; 16];
        let a = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        let b = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        assert_eq!(TextureKey::from(&a), TextureKey::from(&b));
    }

    #[test]
    fn transient_flag_survives_texture_construction() {
        let buf = [0u8; 16];
        let t = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).with_transient(true);
        assert!(t.transient);
        let default = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        assert!(!default.transient);
    }
}
