//! Positioned-glyph texture cache for the SDL GPU backend.
//!
//! SDL2 has no GPU text renderer on the accelerated path, so each glyph run
//! still has to be rasterised by the CPU on first draw. The cache keeps
//! the resulting `SDL_Texture` around so the next frame only needs a
//! single textured quad. Entries are keyed by placement, font and color.
//!
//! Lifetime dance: `TextureCreator` and `Texture<'creator>` are tied
//! together by a borrowed lifetime. We keep them in the same struct and
//! erase the lifetime to `'static` at storage time, then hand textures
//! out only while the creator is alive. Rust field-drop order (cache
//! before creator, per the struct definition below) guarantees every
//! texture is dropped before its creator, which is what matters for
//! `SDL_DestroyTexture` to be safe.

use alloc::vec::Vec;

use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas as SdlCanvas, Texture as SdlTexture, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

use crate::core::cache::{HasSize, LruCache, MaxSize, WithFactory};
use crate::render::SwRenderer;
use crate::render::canvas::Canvas as _;
use crate::render::font::{Font, RasterRunBounds, RasterRunKey};
use crate::render::texture::{ColorFormat, Texture as MiruiTexture};
use crate::types::{Color, Fixed, Point, Rect, Transform, Viewport};

/// SdlTexture wrapper carrying the rasterised byte count so the cache
/// framework can read its size. Newtype-only because `SdlTexture` is
/// from `sdl2` and orphan rules block a direct `HasSize` impl. Bytes
/// are recorded once at rasterise time; the GPU-side allocation isn't
/// observable from here.
struct SizedSdlTexture {
    tex: SdlTexture<'static>,
    byte_len: usize,
}

impl HasSize for SizedSdlTexture {
    fn cache_size(&self) -> usize {
        self.byte_len
    }
}

/// Per-call inputs the rasteriser needs but that aren't part of the cache key.
struct RasterCtx<'a> {
    content: RasterContent<'a>,
    font: &'a Font,
    color: &'a Color,
    bounds: RasterRunBounds,
    scale: Fixed,
    creator: &'a TextureCreator<WindowContext>,
    raster_buf: &'a mut Vec<u8>,
}

#[derive(Clone, Copy)]
enum RasterContent<'a> {
    Positioned {
        glyphs: &'a [textflow::shaping::PositionedGlyph],
        origin: Point,
    },
}

const GLYPH_BUDGET: usize = 8 * 1024 * 1024;

type LabelCtor = fn(&RasterRunKey, RasterCtx<'_>) -> Result<SizedSdlTexture, ()>;

pub struct LabelCache {
    cache: WithFactory<LruCache<RasterRunKey, SizedSdlTexture>, LabelCtor>,
    creator: TextureCreator<WindowContext>,
    raster_buf: Vec<u8>,
}

impl LabelCache {
    pub fn new(creator: TextureCreator<WindowContext>) -> Self {
        let cache = LruCache::builder()
            .max_size(MaxSize::Bytes(GLYPH_BUDGET))
            .name("sdl_gpu/label")
            .build();
        Self {
            cache: WithFactory::new(cache, rasterize_label as LabelCtor),
            creator,
            raster_buf: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_glyph_run(
        &mut self,
        canvas: &mut SdlCanvas<Window>,
        pos: &Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &Font,
        transform: &Transform,
        clip: &Rect,
        color: &Color,
        opa: u8,
        viewport: Viewport,
    ) {
        let raster_scale = viewport.scale() * transform.raster_scale();
        let output_ppem = crate::render::font::output_ppem(font.size, raster_scale);
        let Some(bounds) = font.raster_run_bounds(glyphs, output_ppem, raster_scale) else {
            return;
        };
        let key = font.raster_run_key(glyphs, color, raster_scale);
        let rect = Rect {
            x: pos.x + bounds.offset.x,
            y: pos.y + bounds.offset.y,
            w: bounds.size.x,
            h: bounds.size.y,
        };
        let logical_quad = transform.apply_rect(rect);
        let quad = logical_quad.map(|point| viewport.point_to_physical(point));
        let phys_clip = viewport.rect_to_physical(*clip);
        self.draw_cached(
            canvas,
            &quad,
            RasterContent::Positioned {
                glyphs,
                origin: Point {
                    x: -bounds.offset.x,
                    y: -bounds.offset.y,
                },
            },
            bounds,
            raster_scale,
            key,
            font,
            &phys_clip,
            color,
            opa,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_cached(
        &mut self,
        canvas: &mut SdlCanvas<Window>,
        quad: &[Point; 4],
        content: RasterContent<'_>,
        bounds: RasterRunBounds,
        scale: Fixed,
        key: RasterRunKey,
        font: &Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
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
        let Ok(handle) = self.cache.entry(key).or_insert_with(|ctor, k| {
            let ctx = RasterCtx {
                content,
                font,
                color,
                bounds,
                scale,
                creator,
                raster_buf,
            };
            ctor(k, ctx)
        }) else {
            return;
        };

        if let Some(sdl_clip) = clip_rect {
            canvas.set_clip_rect(sdl_clip);
        } else {
            canvas.set_clip_rect(None);
        }
        let tint = SDL_Color {
            r: 255,
            g: 255,
            b: 255,
            a,
        };
        let vertex = |point: Point, u, v| SDL_Vertex {
            position: SDL_FPoint {
                x: point.x.to_f32(),
                y: point.y.to_f32(),
            },
            color: tint,
            tex_coord: SDL_FPoint { x: u, y: v },
        };
        let vertices = [
            vertex(quad[0], 0.0, 0.0),
            vertex(quad[1], 1.0, 0.0),
            vertex(quad[2], 1.0, 1.0),
            vertex(quad[3], 0.0, 1.0),
        ];
        let indices: [i32; 6] = [0, 1, 2, 0, 2, 3];
        unsafe {
            sdl2_sys::SDL_RenderGeometry(
                canvas.raw(),
                handle.tex.raw(),
                vertices.as_ptr(),
                vertices.len() as _,
                indices.as_ptr(),
                indices.len() as _,
            );
        }
        canvas.set_clip_rect(None);
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.cache.cache_mut().clear();
    }

    /// Read-only `CacheInspect` view onto the underlying cache. Used by
    /// `SdlGpuSurface` to surface label-cache stats without exposing
    /// the cached texture type.
    pub(super) fn as_inspect(&self) -> &dyn crate::core::cache::CacheInspect {
        self.cache.cache()
    }

    /// Expose the cache's `TextureCreator` so callers can allocate extra
    /// textures tied to the same renderer (e.g. the blit fast-path
    /// creating a per-frame streaming texture).
    pub fn with_creator<R>(&self, f: impl FnOnce(&TextureCreator<WindowContext>) -> R) -> R {
        f(&self.creator)
    }
}

fn rasterize_label(_key: &RasterRunKey, ctx: RasterCtx<'_>) -> Result<SizedSdlTexture, ()> {
    let width = ctx.bounds.width;
    let height = ctx.bounds.height;
    let byte_stride = usize::from(width) * 4;
    let byte_len = byte_stride * usize::from(height);
    ctx.raster_buf.clear();
    ctx.raster_buf.resize(byte_len, 0);
    {
        let tex = MiruiTexture::new(ctx.raster_buf, width, height, ColorFormat::RGBA8888);
        let mut sw = SwRenderer::new(tex);
        sw.viewport = crate::types::Viewport::new(width, height, ctx.scale);
        let area = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: ctx.bounds.size.x,
            h: ctx.bounds.size.y,
        };
        let RasterContent::Positioned { glyphs, origin } = ctx.content;
        let raster_color = Color {
            a: 255,
            ..*ctx.color
        };
        sw.draw_glyph_run(&origin, glyphs, ctx.font, &area, &raster_color, 255);
    }

    let mut new_tex = ctx
        .creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, width.into(), height.into())
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
    Ok(SizedSdlTexture {
        tex: new_tex_static,
        byte_len,
    })
}
