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
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_ScaleMode, SDL_Vertex};

use crate::core::cache::{HasSize, LruCache, MaxSize, WithFactory};
use crate::render::SwRenderer;
use crate::render::canvas::Canvas as _;
use crate::render::font::{
    Font, FontFaceId, FontSurfaceId, GlyphSurface, RasterRunBounds, RasterRunKey,
};
use crate::render::renderer::RenderError;
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ScalarSurfaceKey {
    face_id: FontFaceId,
    revision: u64,
    surface_id: FontSurfaceId,
    width: u32,
    height: u32,
    stride: u32,
    layout: mirx::image::SampleLayout,
}

impl ScalarSurfaceKey {
    fn new(font: &Font, surface: GlyphSurface<'_>) -> Self {
        Self {
            face_id: font.face_id(),
            revision: font.revision(),
            surface_id: surface.id(),
            width: surface.width(),
            height: surface.height(),
            stride: surface.stride(),
            layout: surface.sample_layout(),
        }
    }
}

struct ScalarRasterCtx<'a> {
    surface: GlyphSurface<'a>,
    creator: &'a TextureCreator<WindowContext>,
    raster_buf: &'a mut Vec<u8>,
}

const RUN_BUDGET: usize = 4 * 1024 * 1024;
const SURFACE_BUDGET: usize = 4 * 1024 * 1024;

type LabelCtor = fn(&RasterRunKey, RasterCtx<'_>) -> Result<SizedSdlTexture, ()>;
type ScalarCtor = fn(&ScalarSurfaceKey, ScalarRasterCtx<'_>) -> Result<SizedSdlTexture, ()>;

pub struct LabelCache {
    cache: WithFactory<LruCache<RasterRunKey, SizedSdlTexture>, LabelCtor>,
    scalar_cache: WithFactory<LruCache<ScalarSurfaceKey, SizedSdlTexture>, ScalarCtor>,
    creator: TextureCreator<WindowContext>,
    raster_buf: Vec<u8>,
    scalar_buf: Vec<u8>,
    glyph_vertices: Vec<SDL_Vertex>,
    glyph_indices: Vec<i32>,
}

impl LabelCache {
    pub fn new(creator: TextureCreator<WindowContext>) -> Self {
        let cache = LruCache::builder()
            .max_size(MaxSize::Bytes(RUN_BUDGET))
            .name("sdl_gpu/label")
            .build();
        let scalar_cache = LruCache::builder()
            .max_size(MaxSize::Bytes(SURFACE_BUDGET))
            .name("sdl_gpu/glyph_surface")
            .build();
        Self {
            cache: WithFactory::new(cache, rasterize_label as LabelCtor),
            scalar_cache: WithFactory::new(scalar_cache, rasterize_scalar_surface as ScalarCtor),
            creator,
            raster_buf: Vec::new(),
            scalar_buf: Vec::new(),
            glyph_vertices: Vec::new(),
            glyph_indices: Vec::new(),
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
    ) -> Result<(), RenderError> {
        let raster_scale = viewport.scale() * transform.raster_scale();
        let output_ppem = crate::render::font::output_ppem(font.size, raster_scale);
        let Some(bounds) = font.raster_run_bounds(glyphs, output_ppem, raster_scale) else {
            return Ok(());
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
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_posed_glyph_run(
        &mut self,
        canvas: &mut SdlCanvas<Window>,
        pos: Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        frames: &[textflow::placement::GlyphFrame],
        font: &Font,
        transform: Transform,
        clip: Rect,
        color: Color,
        opa: u8,
        viewport: Viewport,
    ) -> Result<(), RenderError> {
        const GLYPHS_PER_BATCH: usize = 2_048;

        let requested_size = font.size.max(1);
        let raster_scale = viewport.scale() * transform.raster_scale();
        let output_ppem = crate::render::font::output_ppem(requested_size, raster_scale);
        let mut active: Option<(ScalarSurfaceKey, GlyphSurface<'_>)> = None;
        self.glyph_vertices.clear();
        self.glyph_indices.clear();

        for (positioned, frame) in glyphs.iter().zip(frames) {
            let Some(raster) =
                font.raster_for_output(positioned.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let Some(region) = raster
                .region
                .filter(|region| region.width() > 0 && region.height() > 0)
            else {
                continue;
            };
            let coverage = match raster.representation.kind() {
                mirx::font::FontRepresentationKind::Coverage { bits } => {
                    crate::render::font::scalar::alpha_bits(raster.surface.sample_layout())
                        == Some(bits)
                }
                _ => false,
            };
            if !coverage {
                if let Some((key, surface)) = active.take() {
                    self.flush_scalar_batch(canvas, key, surface, clip, viewport)?;
                }
                let tangent = Point {
                    x: crate::types::fixed::from_textflow(frame.unit_tangent.x),
                    y: crate::types::fixed::from_textflow(frame.unit_tangent.y),
                };
                let pose = Transform {
                    m00: tangent.x,
                    m01: Fixed::ZERO - tangent.y,
                    tx: pos.x + crate::types::fixed::from_textflow(frame.local_origin.x),
                    m10: tangent.y,
                    m11: tangent.x,
                    ty: pos.y + crate::types::fixed::from_textflow(frame.local_origin.y),
                };
                let glyph = [textflow::shaping::PositionedGlyph::new(
                    positioned.glyph_id(),
                    textflow::shaping::FlowPoint { x: 0, y: 0 },
                )];
                self.draw_glyph_run(
                    canvas,
                    &font.line_origin_for_baseline(Point::ZERO),
                    &glyph,
                    font,
                    &transform.compose(&pose),
                    &clip,
                    &color,
                    opa,
                    viewport,
                )?;
                continue;
            }

            let key = ScalarSurfaceKey::new(font, raster.surface);
            if active.map(|(active, _)| active) != Some(key)
                || self.glyph_indices.len() / 6 >= GLYPHS_PER_BATCH
            {
                if let Some((key, surface)) = active.take() {
                    self.flush_scalar_batch(canvas, key, surface, clip, viewport)?;
                }
                active = Some((key, raster.surface));
            }
            let Some(quad) = raster.posed_quad(pos, *frame, requested_size, transform) else {
                continue;
            };
            self.push_scalar_glyph(quad, region, raster.surface, color, opa, viewport);
        }
        if let Some((key, surface)) = active {
            self.flush_scalar_batch(canvas, key, surface, clip, viewport)
        } else {
            Ok(())
        }
    }

    fn push_scalar_glyph(
        &mut self,
        quad: crate::render::font::RasterQuad,
        region: mirx::image::Region,
        surface: GlyphSurface<'_>,
        color: Color,
        opa: u8,
        viewport: Viewport,
    ) {
        let Ok(base) = i32::try_from(self.glyph_vertices.len()) else {
            return;
        };
        let Some((vertices, indices)) =
            Self::scalar_glyph_geometry(base, quad, region, surface, color, opa, viewport)
        else {
            return;
        };
        self.glyph_vertices.extend_from_slice(&vertices);
        self.glyph_indices.extend_from_slice(&indices);
    }

    fn scalar_glyph_geometry(
        base: i32,
        quad: crate::render::font::RasterQuad,
        region: mirx::image::Region,
        surface: GlyphSurface<'_>,
        color: Color,
        opa: u8,
        viewport: Viewport,
    ) -> Option<([SDL_Vertex; 4], [i32; 6])> {
        let points = quad
            .transform
            .apply_rect(quad.rect)
            .map(|point| viewport.point_to_physical(point));
        let width = surface.width() as f32;
        let height = surface.height() as f32;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let x0 = (region.x() as f32 + 0.5) / width;
        let y0 = (region.y() as f32 + 0.5) / height;
        let x1 = (region.x().saturating_add(region.width()) as f32 - 0.5) / width;
        let y1 = (region.y().saturating_add(region.height()) as f32 - 0.5) / height;
        let tint = SDL_Color {
            r: color.r,
            g: color.g,
            b: color.b,
            a: ((u16::from(color.a) * u16::from(opa) + 127) / 255) as u8,
        };
        let vertex = |point: Point, uv: [f32; 2]| SDL_Vertex {
            position: SDL_FPoint {
                x: point.x.to_f32(),
                y: point.y.to_f32(),
            },
            color: tint,
            tex_coord: SDL_FPoint { x: uv[0], y: uv[1] },
        };
        Some((
            [
                vertex(points[0], [x0, y0]),
                vertex(points[1], [x1, y0]),
                vertex(points[2], [x1, y1]),
                vertex(points[3], [x0, y1]),
            ],
            [base, base + 1, base + 2, base, base + 2, base + 3],
        ))
    }

    fn flush_scalar_batch(
        &mut self,
        canvas: &mut SdlCanvas<Window>,
        key: ScalarSurfaceKey,
        surface: GlyphSurface<'_>,
        clip: Rect,
        viewport: Viewport,
    ) -> Result<(), RenderError> {
        if self.glyph_indices.is_empty() {
            return Ok(());
        }
        let creator = &self.creator;
        let raster_buf = &mut self.scalar_buf;
        let Ok(handle) = self.scalar_cache.entry(key).or_insert_with(|ctor, key| {
            ctor(
                key,
                ScalarRasterCtx {
                    surface,
                    creator,
                    raster_buf,
                },
            )
        }) else {
            self.glyph_vertices.clear();
            self.glyph_indices.clear();
            return Err(RenderError::BackendFailure);
        };
        let (x0, y0, x1, y1) = viewport.rect_to_physical(clip).pixel_bounds();
        canvas.set_clip_rect(
            (x1 > x0 && y1 > y0)
                .then(|| sdl2::rect::Rect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32)),
        );
        let rendered = unsafe {
            sdl2_sys::SDL_RenderGeometry(
                canvas.raw(),
                handle.tex.raw(),
                self.glyph_vertices.as_ptr(),
                self.glyph_vertices.len() as _,
                self.glyph_indices.as_ptr(),
                self.glyph_indices.len() as _,
            ) == 0
        };
        canvas.set_clip_rect(None);
        self.glyph_vertices.clear();
        self.glyph_indices.clear();
        rendered.then_some(()).ok_or(RenderError::BackendFailure)
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
    ) -> Result<(), RenderError> {
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
            return Err(RenderError::BackendFailure);
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
        let rendered = unsafe {
            sdl2_sys::SDL_RenderGeometry(
                canvas.raw(),
                handle.tex.raw(),
                vertices.as_ptr(),
                vertices.len() as _,
                indices.as_ptr(),
                indices.len() as _,
            ) == 0
        };
        canvas.set_clip_rect(None);
        rendered.then_some(()).ok_or(RenderError::BackendFailure)
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.cache.cache_mut().clear();
        self.scalar_cache.cache_mut().clear();
    }

    /// Read-only `CacheInspect` view onto the underlying cache. Used by
    /// `SdlGpuSurface` to surface label-cache stats without exposing
    /// the cached texture type.
    pub(super) fn as_inspect(&self) -> &dyn crate::core::cache::CacheInspect {
        self.cache.cache()
    }

    pub(super) fn scalar_inspect(&self) -> &dyn crate::core::cache::CacheInspect {
        self.scalar_cache.cache()
    }

    /// Expose the cache's `TextureCreator` so callers can allocate extra
    /// textures tied to the same renderer (e.g. the blit fast-path
    /// creating a per-frame streaming texture).
    pub fn with_creator<R>(&self, f: impl FnOnce(&TextureCreator<WindowContext>) -> R) -> R {
        f(&self.creator)
    }
}

fn rasterize_scalar_surface(
    _key: &ScalarSurfaceKey,
    ctx: ScalarRasterCtx<'_>,
) -> Result<SizedSdlTexture, ()> {
    crate::render::font::scalar::unpack_surface(ctx.surface, ctx.raster_buf).ok_or(())?;
    let pixels = usize::try_from(ctx.surface.width())
        .ok()
        .and_then(|width| {
            usize::try_from(ctx.surface.height())
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(())?;
    ctx.raster_buf.resize(pixels.checked_mul(4).ok_or(())?, 0);
    for index in (0..pixels).rev() {
        let alpha = ctx.raster_buf[index];
        let offset = index * 4;
        ctx.raster_buf[offset..offset + 4].copy_from_slice(&[255, 255, 255, alpha]);
    }
    let width = ctx.surface.width();
    let height = ctx.surface.height();
    let stride = usize::try_from(width)
        .map_err(|_| ())?
        .checked_mul(4)
        .ok_or(())?;
    let mut texture = ctx
        .creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, width, height)
        .map_err(|_| ())?;
    texture
        .update(None, ctx.raster_buf, stride)
        .map_err(|_| ())?;
    texture.set_blend_mode(sdl2::render::BlendMode::Blend);
    if unsafe {
        sdl2_sys::SDL_SetTextureScaleMode(texture.raw(), SDL_ScaleMode::SDL_ScaleModeLinear)
    } != 0
    {
        return Err(());
    }
    let texture: SdlTexture<'static> = unsafe { core::mem::transmute(texture) };
    Ok(SizedSdlTexture {
        tex: texture,
        byte_len: pixels * 4,
    })
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
    if unsafe {
        sdl2_sys::SDL_SetTextureScaleMode(new_tex.raw(), SDL_ScaleMode::SDL_ScaleModeLinear)
    } != 0
    {
        return Err(());
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_geometry_keeps_pose_region_tint_and_display_scale() {
        let surface = GlyphSurface::new(
            &[0; 64],
            8,
            8,
            8,
            mirx::image::SampleLayout::A8,
            mirx::types::ByteAlignment::ONE,
            FontSurfaceId::new(3),
        )
        .unwrap();
        let quad = crate::render::font::RasterQuad {
            rect: Rect::new(1, 2, 4, 3),
            transform: Transform::translate(Fixed::from_int(5), Fixed::from_int(7)),
        };
        let region = mirx::image::Region::new(2, 1, 4, 3).unwrap();

        let (vertices, indices) = LabelCache::scalar_glyph_geometry(
            4,
            quad,
            region,
            surface,
            Color::rgba(10, 20, 30, 128),
            128,
            Viewport::new(100, 100, Fixed::from_int(2)),
        )
        .unwrap();

        assert_eq!(indices, [4, 5, 6, 4, 6, 7]);
        assert_eq!(
            (vertices[0].position.x, vertices[0].position.y),
            (12.0, 18.0)
        );
        assert_eq!(
            (vertices[2].position.x, vertices[2].position.y),
            (20.0, 24.0)
        );
        assert_eq!(
            (vertices[0].tex_coord.x, vertices[0].tex_coord.y),
            (0.3125, 0.1875)
        );
        assert_eq!(
            (vertices[2].tex_coord.x, vertices[2].tex_coord.y),
            (0.6875, 0.4375)
        );
        assert_eq!(
            (
                vertices[0].color.r,
                vertices[0].color.g,
                vertices[0].color.b,
                vertices[0].color.a,
            ),
            (10, 20, 30, 64)
        );
    }
}
