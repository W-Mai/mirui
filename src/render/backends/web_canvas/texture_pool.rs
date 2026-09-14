//! Texture upload cache — each `Texture` is uploaded once into an
//! `OffscreenCanvas` so blits skip the wasm/JS boundary per frame.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::Clamped;
use wasm_bindgen::JsCast;
use web_sys::{ImageData, OffscreenCanvas, OffscreenCanvasRenderingContext2d};

use crate::core::cache::{Cache, HasSize, HashLookup, Lru, MaxSize};
use crate::render::texture::{ColorFormat, Texture};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureKey {
    ptr: usize,
    len: usize,
    width: u16,
    height: u16,
    format: ColorFormat,
    stride: usize,
    revision: u64,
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
            stride: src.stride,
            revision: src.cache_revision,
        }
    }
}

/// Newtype so `HasSize` can be impl'd — `OffscreenCanvas` is foreign.
pub struct CachedOffscreen {
    pub canvas: OffscreenCanvas,
    pub width: u16,
    pub height: u16,
}

impl HasSize for CachedOffscreen {
    fn cache_size(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }
}

const TEXTURE_BUDGET: usize = 16 * 1024 * 1024;

pub type TexturePool = Cache<TextureKey, CachedOffscreen, Lru, HashLookup<TextureKey>>;

pub fn new_pool() -> TexturePool {
    Cache::builder()
        .max_size(MaxSize::Bytes(TEXTURE_BUDGET))
        .build()
}

const GLYPH_BUDGET: usize = 8 * 1024 * 1024;

pub type GlyphPool = Cache<
    crate::render::font::RasterRunKey,
    CachedOffscreen,
    Lru,
    HashLookup<crate::render::font::RasterRunKey>,
>;

pub fn new_glyph_pool() -> GlyphPool {
    Cache::builder()
        .max_size(MaxSize::Bytes(GLYPH_BUDGET))
        .build()
}

pub fn upload(src: &Texture) -> Option<CachedOffscreen> {
    let rgba = src.rgba8_pixels()?;
    let canvas = OffscreenCanvas::new(src.width as u32, src.height as u32).ok()?;
    let ctx = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<OffscreenCanvasRenderingContext2d>()
        .ok()?;
    let image_data = ImageData::new_with_u8_clamped_array_and_sh(
        Clamped(&rgba),
        src.width as u32,
        src.height as u32,
    )
    .ok()?;
    ctx.put_image_data(&image_data, 0.0, 0.0).ok()?;
    Some(CachedOffscreen {
        canvas,
        width: src.width,
        height: src.height,
    })
}
