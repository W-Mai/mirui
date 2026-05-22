//! Per-label texture cache for the SDL GPU backend.
//!
//! SDL2 has no GPU text renderer on the accelerated path, so each label
//! still has to be rasterised by the CPU on first draw. The cache keeps
//! the resulting `SDL_Texture` around so the next frame only needs a
//! single `canvas.copy`. Keyed by `(text_hash, color)`, LRU-bounded.
//!
//! Lifetime dance: `TextureCreator` and `Texture<'creator>` are tied
//! together by a borrowed lifetime. We keep them in the same struct and
//! erase the lifetime to `'static` at storage time, then hand textures
//! out only while the creator is alive. Rust field-drop order (cache
//! before creator, per the struct definition below) guarantees every
//! texture is dropped before its creator, which is what matters for
//! `SDL_DestroyTexture` to be safe.

use alloc::vec::Vec;
use core::cell::RefCell;
use core::hash::{Hash, Hasher};

use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas as SdlCanvas, Texture as SdlTexture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::cache::{LruCache, MaxSize, WithFactory};
use crate::draw::SwRenderer;
use crate::draw::canvas::Canvas as _;
use crate::draw::font::{CHAR_H, CHAR_W};
use crate::draw::texture::{ColorFormat, Texture as MiruiTexture};
use crate::types::{Color, Fixed, Point, Rect};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct LabelKey {
    text_hash: u64,
    color_rgba: u32,
}

// `Handle<V>` hands out `&V`, but `SdlTexture::set_alpha_mod` needs
// `&mut self`, so the cached value is itself a `RefCell`.
type CachedTexture = RefCell<SdlTexture<'static>>;

/// Per-call inputs the rasteriser needs but that aren't part of `LabelKey`
/// (the cache key intentionally hashes `text` to avoid storing the bytes).
struct RasterCtx<'a, 'creator> {
    text: &'a [u8],
    color: &'a Color,
    creator: &'a TextureCreator<WindowContext>,
    raster_buf: &'a mut Vec<u8>,
    _creator_lt: core::marker::PhantomData<&'creator ()>,
}

const DEFAULT_CAPACITY: usize = 128;

type LabelFactory = fn(&LabelKey, RasterCtx<'_, '_>) -> Result<CachedTexture, ()>;

pub struct LabelCache {
    cache:
        WithFactory<LruCache<LabelKey, CachedTexture>, LabelFactory, RasterCtx<'static, 'static>>,
    creator: TextureCreator<WindowContext>,
    raster_buf: Vec<u8>,
}

impl LabelCache {
    pub fn new(creator: TextureCreator<WindowContext>) -> Self {
        Self::with_capacity(creator, DEFAULT_CAPACITY)
    }

    #[allow(dead_code)]
    pub fn with_capacity(creator: TextureCreator<WindowContext>, capacity: usize) -> Self {
        let cache = LruCache::builder()
            .max_size(MaxSize::Count(capacity.max(1)))
            .build();
        Self {
            cache: WithFactory::new(cache, rasterize_label as LabelFactory),
            creator,
            raster_buf: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        canvas: &mut SdlCanvas<Window>,
        pos: &Point,
        text: &[u8],
        clip: &Rect,
        color: &Color,
        opa: u8,
        scale: Fixed,
    ) {
        if text.is_empty() {
            return;
        }
        let key = LabelKey {
            text_hash: hash_bytes(text),
            color_rgba: pack_rgba(color),
        };

        let phys_char_w = (Fixed::from_int(CHAR_W as i32) * scale).to_int().max(1) as u32;
        let phys_char_h = (Fixed::from_int(CHAR_H as i32) * scale).to_int().max(1) as u32;
        let phys_w = phys_char_w * text.len() as u32;
        let phys_h = phys_char_h;

        let dst = sdl2::rect::Rect::new(pos.x.to_int(), pos.y.to_int(), phys_w, phys_h);
        let (cx0, cy0, cx1, cy1) = clip.pixel_bounds();
        let clip_rect = if cx1 > cx0 && cy1 > cy0 {
            Some(sdl2::rect::Rect::new(
                cx0,
                cy0,
                (cx1 - cx0) as u32,
                (cy1 - cy0) as u32,
            ))
        } else {
            None
        };
        let a = ((color.a as u16) * (opa as u16) / 255) as u8;

        let creator = &self.creator;
        let raster_buf = &mut self.raster_buf;
        let ctx = RasterCtx {
            text,
            color,
            creator,
            raster_buf,
            _creator_lt: core::marker::PhantomData,
        };
        let Ok(handle) = (unsafe { acquire_or_create_with_ctx(&mut self.cache, key, ctx) }) else {
            return;
        };

        let mut tex = handle.borrow_mut();
        tex.set_alpha_mod(a);

        if let Some(sdl_clip) = clip_rect {
            canvas.set_clip_rect(sdl_clip);
        } else {
            canvas.set_clip_rect(None);
        }
        let _ = canvas.copy(&tex, None, Some(dst));
        canvas.set_clip_rect(None);
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.cache.cache_mut().clear();
    }

    /// Expose the cache's `TextureCreator` so callers can allocate extra
    /// textures tied to the same renderer (e.g. the blit fast-path
    /// creating a per-frame streaming texture).
    pub fn with_creator<R>(&self, f: impl FnOnce(&TextureCreator<WindowContext>) -> R) -> R {
        f(&self.creator)
    }
}

/// Bridge an `'a`-bounded `RasterCtx` through `WithFactory`, whose
/// `Ctx` type parameter is fixed at `RasterCtx<'static, 'static>`
/// (struct fields can't carry call-site lifetimes).
///
/// # Safety
///
/// Caller must guarantee the `'a` borrows in `ctx` outlive this call.
/// `WithFactory::acquire_or_create` consumes `Ctx` by value on the
/// synchronous call stack and `rasterize_label` (the registered
/// factory) does not store, return, or otherwise let any reference
/// inside `ctx` escape: it only writes through `raster_buf` and reads
/// from `creator` while building the texture, then drops the borrows
/// before returning. The transmute therefore widens lifetimes only
/// for the duration of the call frame, which is sound. If the
/// factory or the cache ever stores `Ctx` past that frame, this
/// becomes use-after-free.
unsafe fn acquire_or_create_with_ctx<'a>(
    cache: &mut WithFactory<
        LruCache<LabelKey, CachedTexture>,
        LabelFactory,
        RasterCtx<'static, 'static>,
    >,
    key: LabelKey,
    ctx: RasterCtx<'a, 'a>,
) -> Result<crate::cache::Handle<CachedTexture>, crate::cache::CacheError<()>> {
    let ctx_static: RasterCtx<'static, 'static> = unsafe { core::mem::transmute(ctx) };
    cache.acquire_or_create(key, ctx_static)
}

fn rasterize_label(_key: &LabelKey, ctx: RasterCtx<'_, '_>) -> Result<CachedTexture, ()> {
    let logical_w = CHAR_W as usize * ctx.text.len();
    let logical_h = CHAR_H as usize;
    let byte_stride = logical_w * 4;
    let byte_len = byte_stride * logical_h;
    ctx.raster_buf.clear();
    ctx.raster_buf.resize(byte_len, 0);
    {
        let tex = MiruiTexture::new(
            ctx.raster_buf,
            logical_w as u16,
            logical_h as u16,
            ColorFormat::RGBA8888,
        );
        let mut sw = SwRenderer::new(tex);
        let area = Rect::new(0, 0, logical_w as u16, logical_h as u16);
        sw.draw_label(
            &Point {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ctx.text,
            &area,
            ctx.color,
            255,
        );
    }

    let mut new_tex = ctx
        .creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, logical_w as u32, logical_h as u32)
        .map_err(|_| ())?;
    new_tex
        .update(None, ctx.raster_buf, byte_stride)
        .map_err(|_| ())?;
    new_tex.set_blend_mode(sdl2::render::BlendMode::Blend);

    // Erase the creator's borrow so the texture can sit inside `LruCache`.
    // Soundness: every texture is dropped before its creator because
    // `LabelCache` lists `cache` before `creator` and Rust drops fields
    // in declaration order.
    let new_tex_static: SdlTexture<'static> = unsafe { core::mem::transmute(new_tex) };
    Ok(RefCell::new(new_tex_static))
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = FxHasher::default();
    bytes.hash(&mut h);
    h.finish()
}

fn pack_rgba(c: &Color) -> u32 {
    ((c.r as u32) << 24) | ((c.g as u32) << 16) | ((c.b as u32) << 8) | (c.a as u32)
}

/// Lightweight non-cryptographic hasher to avoid pulling in ahash/hashbrown
/// feature footprint just for a per-label key. Inlined FxHash variant.
#[derive(Default)]
struct FxHasher(u64);
impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.rotate_left(5) ^ b as u64 ^ 0x27220a95;
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
