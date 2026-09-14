//! GPU side cache for `wgpu::Texture` uploads — keyed on the source
//! buffer pointer + layout, evicted by mirui's LRU cache. Static
//! assets (`IMG_THUMBS_UP` and friends) hit on every frame after the
//! first upload. Reused dynamic buffers are marked transient and bypass
//! this pointer-based cache.

use crate::core::cache::{Cache, HasSize, HashLookup, Lru, MaxSize};
use crate::render::font::{FontFaceId, FontSurfaceId, GlyphSurface};
use crate::render::texture::{ColorFormat, Texture};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureKey {
    ptr: usize,
    len: usize,
    width: u16,
    height: u16,
    format: ColorFormat,
    stride: usize,
}

impl TextureKey {
    pub fn cacheable(src: &Texture) -> Option<Self> {
        (!src.transient).then(|| Self::from(src))
    }

    pub fn from(src: &Texture) -> Self {
        let buf = src.buf.as_slice();
        Self {
            ptr: buf.as_ptr() as usize,
            len: buf.len(),
            width: src.width,
            height: src.height,
            format: src.format,
            stride: src.stride,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScalarSurfaceKey {
    face_id: FontFaceId,
    revision: u64,
    surface_id: FontSurfaceId,
    width: u32,
    height: u32,
    stride: u32,
    layout: mirx::image::SampleLayout,
}

impl ScalarSurfaceKey {
    pub fn new(face_id: FontFaceId, revision: u64, surface: GlyphSurface<'_>) -> Self {
        Self {
            face_id,
            revision,
            surface_id: surface.id(),
            width: surface.width(),
            height: surface.height(),
            stride: surface.stride(),
            layout: surface.sample_layout(),
        }
    }
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
    fn key_separates_strides_over_one_buffer() {
        let buf = [0u8; 24];
        let a = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        let mut b = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        b.stride = 12;
        assert_ne!(TextureKey::from(&a), TextureKey::from(&b));
    }

    #[test]
    fn transient_flag_survives_texture_construction() {
        let buf = [0u8; 16];
        let t = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).with_transient(true);
        assert!(t.transient);
        assert!(TextureKey::cacheable(&t).is_none());
        let default = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        assert!(!default.transient);
        assert_eq!(
            TextureKey::cacheable(&default),
            Some(TextureKey::from(&default))
        );
    }

    #[test]
    fn scalar_surface_revision_invalidates_the_gpu_entry() {
        let samples = [0u8; 16];
        let surface = GlyphSurface::new(
            &samples,
            4,
            4,
            4,
            mirx::image::SampleLayout::A8,
            mirx::types::ByteAlignment::ONE,
            FontSurfaceId::new(7),
        )
        .unwrap();
        let face = FontFaceId::new(11);

        assert_ne!(
            ScalarSurfaceKey::new(face, 2, surface),
            ScalarSurfaceKey::new(face, 3, surface)
        );
    }
}
