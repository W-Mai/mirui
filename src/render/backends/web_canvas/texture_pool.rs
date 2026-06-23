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

/// Keys a rendered text label on content only — no clip / position —
/// so a label keeps hitting the cache while resize reflows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub text_hash: u64,
    pub family_ptr: usize,
    pub size: u16,
    pub color: u32,
    pub opa: u8,
    pub scale: u16,
}

const GLYPH_BUDGET: usize = 8 * 1024 * 1024;

pub type GlyphPool = Cache<GlyphKey, CachedOffscreen, Lru, HashLookup<GlyphKey>>;

pub fn new_glyph_pool() -> GlyphPool {
    Cache::builder()
        .max_size(MaxSize::Bytes(GLYPH_BUDGET))
        .build()
}

pub fn upload(src: &Texture) -> Option<CachedOffscreen> {
    let rgba = texture_to_rgba8(src)?;
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

fn texture_to_rgba8(src: &Texture) -> Option<alloc::vec::Vec<u8>> {
    let buf = src.buf.as_slice();
    let bpp = src.format.bytes_per_pixel();
    let w = src.width as usize;
    let h = src.height as usize;
    match src.format {
        ColorFormat::RGBA8888 => Some(buf.to_vec()),
        ColorFormat::BGRA8888 => {
            let mut out = alloc::vec::Vec::with_capacity(w * h * 4);
            for chunk in buf.chunks_exact(bpp) {
                out.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
            }
            Some(out)
        }
        ColorFormat::RGB888 => {
            let mut out = alloc::vec::Vec::with_capacity(w * h * 4);
            for chunk in buf.chunks_exact(bpp) {
                out.extend_from_slice(chunk);
                out.push(0xff);
            }
            Some(out)
        }
        // RGB565 unpack not implemented.
        ColorFormat::RGB565 | ColorFormat::RGB565Swapped => None,
    }
}
