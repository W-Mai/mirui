//! Positioned-glyph texture cache for the SDL GPU backend.
//!
//! Coverage runs retain geometry and share uploaded font surfaces. Other representations retain rasterized run textures.
//!
//! Cached SDL textures are private. Both pools drop before their shared device owner, which retains the renderer and texture creator.

use alloc::{rc::Rc, vec::Vec};

use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Texture as SdlTexture;
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_ScaleMode, SDL_Vertex};

use super::device::Device;
use super::sdl_pixel_rect;

use crate::core::cache::{HasSize, LruCache, MaxSize, WithFactory};
use crate::render::SwRenderer;
use crate::render::canvas::Canvas as _;
use crate::render::font::scalar::ScalarSurfaceKey;
use crate::render::font::{Font, GlyphSurface, RasterRunBounds, RasterRunKey};
use crate::render::renderer::RenderError;
use crate::render::texture::{AlphaMode, ColorFormat, Texture as MiruiTexture};
use crate::types::{Color, Fixed, Point, Rect, Transform, Viewport};

mod coverage;
use coverage::CoverageRun;

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
    device: &'a Device,
    raster_buf: &'a mut Vec<u8>,
}

#[derive(Clone, Copy)]
enum RasterContent<'a> {
    Positioned {
        glyphs: &'a [textflow::shaping::PositionedGlyph],
        origin: Point,
    },
}

struct ScalarRasterCtx<'a> {
    surface: GlyphSurface<'a>,
    device: &'a Device,
    raster_buf: &'a mut Vec<u8>,
}

const RUN_BUDGET: usize = 4 * 1024 * 1024;
const SURFACE_BUDGET: usize = 4 * 1024 * 1024;
const MAX_BATCH_INDICES: usize = 12_288;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum RunKey {
    Raster(RasterRunKey),
    Coverage(RasterRunKey),
}

enum CachedRun {
    Raster(SizedSdlTexture),
    Coverage(CoverageRun),
}

impl HasSize for CachedRun {
    fn cache_size(&self) -> usize {
        match self {
            Self::Raster(texture) => texture.cache_size(),
            Self::Coverage(run) => run.cache_size() + COVERAGE_ENTRY_OVERHEAD,
        }
    }
}

const COVERAGE_ENTRY_OVERHEAD: usize =
    core::mem::size_of::<CachedRun>() - core::mem::size_of::<CoverageRun>();

type ScalarCtor =
    fn(&ScalarSurfaceKey, ScalarRasterCtx<'_>) -> Result<SizedSdlTexture, RenderError>;

pub struct LabelCache {
    cache: LruCache<RunKey, CachedRun>,
    scalar_cache: WithFactory<LruCache<ScalarSurfaceKey, SizedSdlTexture>, ScalarCtor>,
    device: Rc<Device>,
    generation: u64,
    raster_buf: Vec<u8>,
    scalar_buf: Vec<u8>,
    glyph_vertices: Vec<SDL_Vertex>,
    glyph_indices: Vec<i32>,
}

impl LabelCache {
    pub(super) fn new(device: Rc<Device>) -> Self {
        let cache = LruCache::builder()
            .max_size(MaxSize::Bytes(RUN_BUDGET))
            .name("sdl_gpu/label")
            .build();
        let scalar_cache = LruCache::builder()
            .max_size(MaxSize::Bytes(SURFACE_BUDGET))
            .name("sdl_gpu/glyph_surface")
            .build();
        Self {
            cache,
            scalar_cache: WithFactory::new(scalar_cache, rasterize_scalar_surface as ScalarCtor),
            generation: device.generation(),
            device,
            raster_buf: Vec::new(),
            scalar_buf: Vec::new(),
            glyph_vertices: Vec::new(),
            glyph_indices: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_raster_run(
        &mut self,
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
    pub fn draw_glyph_run(
        &mut self,
        pos: &Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &Font,
        transform: &Transform,
        clip: &Rect,
        color: &Color,
        opa: u8,
        viewport: Viewport,
    ) -> Result<(), RenderError> {
        if glyphs.is_empty() {
            return Ok(());
        }
        if !self.draw_coverage_run(*pos, glyphs, font, *transform, *clip, *color, opa, viewport)? {
            self.draw_raster_run(pos, glyphs, font, transform, clip, color, opa, viewport)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_coverage_run(
        &mut self,
        pos: Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &Font,
        transform: Transform,
        clip: Rect,
        color: Color,
        opa: u8,
        viewport: Viewport,
    ) -> Result<bool, RenderError> {
        let scale = viewport.scale() * transform.raster_scale();
        if scale <= Fixed::ZERO {
            return Ok(true);
        }
        let key = RunKey::Coverage(font.raster_run_key(glyphs, &Color::rgb(255, 255, 255), scale));
        let cached = self
            .cache
            .acquire(&key)
            .filter(|entry| matches!(&**entry, CachedRun::Coverage(run) if run.matches(glyphs)));
        let handle = match cached {
            Some(handle) => handle,
            None => {
                self.cache.drop(&key);
                let Some(run) =
                    CoverageRun::prepare(glyphs, font, scale, self.device.max_texture_size)?
                else {
                    return Ok(false);
                };
                self.cache
                    .entry(key)
                    .or_insert_with(|| CachedRun::Coverage(run))
            }
        };
        let CachedRun::Coverage(run) = &*handle else {
            unreachable!("coverage cache key");
        };
        let output = crate::render::font::output_ppem(font.size, scale);
        let logical = Point {
            x: pos.x + run.offset.x,
            y: pos.y + run.offset.y,
        };
        let origin = viewport.point_to_physical(transform.apply_point(logical));
        let factor = viewport.scale().to_f32() / scale.to_f32();
        let (xx, xy, yx, yy) = (
            transform.m00.to_f32() * factor,
            transform.m01.to_f32() * factor,
            transform.m10.to_f32() * factor,
            transform.m11.to_f32() * factor,
        );
        let tint = SDL_Color {
            r: color.r,
            g: color.g,
            b: color.b,
            a: glyph_opacity(color.a, opa),
        };
        for batch in &run.batches {
            let raster = font
                .raster_for_output(batch.glyph, font.size.max(1), output)
                .ok_or(RenderError::InvalidTexture)?;
            if batch.key != ScalarSurfaceKey::new(font.face_id(), font.revision(), raster.surface) {
                return Err(RenderError::InvalidTexture);
            }
            self.glyph_vertices.clear();
            self.glyph_vertices
                .extend(
                    run.vertices[batch.vertices.clone()]
                        .iter()
                        .map(|vertex| SDL_Vertex {
                            position: SDL_FPoint {
                                x: origin.x.to_f32()
                                    + vertex.position.x * xx
                                    + vertex.position.y * xy,
                                y: origin.y.to_f32()
                                    + vertex.position.x * yx
                                    + vertex.position.y * yy,
                            },
                            color: tint,
                            tex_coord: vertex.tex_coord,
                        }),
                );
            self.draw_scalar_batch(
                batch.key,
                raster.surface,
                &run.indices[batch.indices.clone()],
                clip,
                viewport,
            )?;
        }
        self.glyph_vertices.clear();
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_posed_glyph_run(
        &mut self,
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
            if !scalar_admitted(raster, self.device.max_texture_size) {
                if let Some((key, surface)) = active.take() {
                    self.flush_scalar_batch(key, surface, clip, viewport)?;
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
                self.draw_raster_run(
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

            let key = ScalarSurfaceKey::new(font.face_id(), font.revision(), raster.surface);
            if active.map(|(active, _)| active) != Some(key)
                || self.glyph_indices.len() + 54 > MAX_BATCH_INDICES
            {
                if let Some((key, surface)) = active.take() {
                    self.flush_scalar_batch(key, surface, clip, viewport)?;
                }
                active = Some((key, raster.surface));
            }
            let Some(quad) = raster.posed_quad(pos, *frame, requested_size, transform) else {
                continue;
            };
            self.push_scalar_glyph(quad, region, raster.surface, color, opa, viewport);
        }
        if let Some((key, surface)) = active {
            self.flush_scalar_batch(key, surface, clip, viewport)
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
    ) -> Option<([SDL_Vertex; 16], [i32; 54])> {
        let points = quad
            .transform
            .apply_rect(quad.rect)
            .map(|point| viewport.point_to_physical(point));
        let width = surface.width() as f32;
        let height = surface.height() as f32;
        if width <= 0.0 || height <= 0.0 || region.width() == 0 || region.height() == 0 {
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
            a: glyph_opacity(color.a, opa),
        };
        // Split at texel centers: interpolate at the original scale, then clamp the edge strips.
        let columns = [
            0.0,
            0.5 / region.width() as f32,
            1.0 - 0.5 / region.width() as f32,
            1.0,
        ];
        let rows = [
            0.0,
            0.5 / region.height() as f32,
            1.0 - 0.5 / region.height() as f32,
            1.0,
        ];
        let us = [x0, x0, x1, x1];
        let vs = [y0, y0, y1, y1];
        let vertices = core::array::from_fn(|index| {
            let column = index % 4;
            let row = index / 4;
            let position = |axis: fn(Point) -> Fixed| {
                axis(points[0]).to_f32()
                    + (axis(points[1]) - axis(points[0])).to_f32() * columns[column]
                    + (axis(points[3]) - axis(points[0])).to_f32() * rows[row]
            };
            SDL_Vertex {
                position: SDL_FPoint {
                    x: position(|point| point.x),
                    y: position(|point| point.y),
                },
                color: tint,
                tex_coord: SDL_FPoint {
                    x: us[column],
                    y: vs[row],
                },
            }
        });
        let indices = core::array::from_fn(|index| {
            let cell = index / 6;
            let origin = base + (cell / 3 * 4 + cell % 3) as i32;
            origin + [0, 1, 5, 0, 5, 4][index % 6]
        });
        Some((vertices, indices))
    }

    fn flush_scalar_batch(
        &mut self,
        key: ScalarSurfaceKey,
        surface: GlyphSurface<'_>,
        clip: Rect,
        viewport: Viewport,
    ) -> Result<(), RenderError> {
        let mut indices = core::mem::take(&mut self.glyph_indices);
        let result = self.draw_scalar_batch(key, surface, &indices, clip, viewport);
        indices.clear();
        self.glyph_indices = indices;
        self.glyph_vertices.clear();
        result
    }

    fn draw_scalar_batch(
        &mut self,
        key: ScalarSurfaceKey,
        surface: GlyphSurface<'_>,
        indices: &[i32],
        clip: Rect,
        viewport: Viewport,
    ) -> Result<(), RenderError> {
        if indices.is_empty() {
            return Ok(());
        }
        let physical_clip = viewport.rect_to_physical(clip);
        let Some(clip) = sdl_pixel_rect(&physical_clip, &physical_clip) else {
            return Ok(());
        };
        let device = &self.device;
        let raster_buf = &mut self.scalar_buf;
        let handle = self
            .scalar_cache
            .entry(key)
            .or_insert_with(|ctor, key| {
                ctor(
                    key,
                    ScalarRasterCtx {
                        surface,
                        device,
                        raster_buf,
                    },
                )
            })
            .map_err(cache_error)?;
        let mut canvas = device.canvas()?;
        canvas.set_clip_rect(clip);
        let rendered = unsafe {
            sdl2_sys::SDL_RenderGeometry(
                canvas.raw(),
                handle.tex.raw(),
                self.glyph_vertices.as_ptr(),
                self.glyph_vertices.len() as _,
                indices.as_ptr(),
                indices.len() as _,
            ) == 0
        };
        canvas.set_clip_rect(None);
        rendered.then_some(()).ok_or(RenderError::BackendFailure)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_cached(
        &mut self,
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
        let Some(clip_rect) = sdl_pixel_rect(clip, clip) else {
            return Ok(());
        };
        let a = glyph_opacity(color.a, opa);

        let device = &self.device;
        let raster_buf = &mut self.raster_buf;
        let handle = self
            .cache
            .entry(RunKey::Raster(key))
            .or_try_insert_with(|| {
                let ctx = RasterCtx {
                    content,
                    font,
                    color,
                    bounds,
                    scale,
                    device,
                    raster_buf,
                };
                rasterize_label(&key, ctx).map(CachedRun::Raster)
            })?;

        let CachedRun::Raster(handle) = &*handle else {
            unreachable!("raster cache key");
        };

        let mut canvas = device.canvas()?;
        canvas.set_clip_rect(clip_rect);
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

    pub(super) fn matches(&self, device: &Rc<Device>) -> bool {
        Rc::ptr_eq(&self.device, device) && self.generation == device.generation()
    }

    pub(super) fn as_inspect(&self) -> &dyn crate::core::cache::CacheInspect {
        &self.cache
    }

    pub(super) fn scalar_inspect(&self) -> &dyn crate::core::cache::CacheInspect {
        self.scalar_cache.cache()
    }
}

fn glyph_opacity(alpha: u8, opacity: u8) -> u8 {
    ((u16::from(alpha) * u16::from(opacity)) / 255) as u8
}

fn surface_fits(surface: GlyphSurface<'_>, limit: (u32, u32)) -> bool {
    let (width, height) = (surface.width(), surface.height());
    width > 0
        && height > 0
        && (limit.0 == 0 || width <= limit.0)
        && (limit.1 == 0 || height <= limit.1)
        && (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .is_some_and(|bytes| bytes <= SURFACE_BUDGET)
}

fn scalar_admitted(raster: crate::render::font::RasterGlyph<'_>, limit: (u32, u32)) -> bool {
    matches!(raster.representation.kind(), mirx::font::FontRepresentationKind::Coverage { bits }
        if crate::render::font::scalar::alpha_bits(raster.surface.sample_layout()) == Some(bits))
        && surface_fits(raster.surface, limit)
}

fn cache_error(error: crate::core::cache::CacheError<RenderError>) -> RenderError {
    match error {
        crate::core::cache::CacheError::Factory(error) => error,
        crate::core::cache::CacheError::Disabled | crate::core::cache::CacheError::TooLarge => {
            RenderError::BackendFailure
        }
    }
}

fn rasterize_scalar_surface(
    _key: &ScalarSurfaceKey,
    ctx: ScalarRasterCtx<'_>,
) -> Result<SizedSdlTexture, RenderError> {
    if !surface_fits(ctx.surface, ctx.device.max_texture_size) {
        return Err(RenderError::ResourceLimit(
            crate::render::RenderResource::Texture,
        ));
    }
    crate::render::font::scalar::unpack_surface(ctx.surface, ctx.raster_buf)
        .ok_or(RenderError::InvalidTexture)?;
    let pixels = usize::try_from(ctx.surface.width())
        .ok()
        .and_then(|width| {
            usize::try_from(ctx.surface.height())
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(RenderError::InvalidTexture)?;
    ctx.raster_buf
        .resize(pixels.checked_mul(4).ok_or(RenderError::InvalidTexture)?, 0);
    for index in (0..pixels).rev() {
        let alpha = ctx.raster_buf[index];
        let offset = index * 4;
        ctx.raster_buf[offset..offset + 4].copy_from_slice(&[255, 255, 255, alpha]);
    }
    let width = ctx.surface.width();
    let height = ctx.surface.height();
    let stride = usize::try_from(width)
        .map_err(|_| RenderError::InvalidTexture)?
        .checked_mul(4)
        .ok_or(RenderError::InvalidTexture)?;
    let _lease = ctx.device.canvas()?;
    let mut texture = ctx
        .device
        .creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, width, height)
        .map_err(|_| RenderError::BackendFailure)?;
    texture
        .update(None, ctx.raster_buf, stride)
        .map_err(|_| RenderError::BackendFailure)?;
    texture.set_blend_mode(sdl2::render::BlendMode::Blend);
    if unsafe {
        sdl2_sys::SDL_SetTextureScaleMode(texture.raw(), SDL_ScaleMode::SDL_ScaleModeLinear)
    } != 0
    {
        return Err(RenderError::BackendFailure);
    }
    let texture: SdlTexture<'static> = unsafe { core::mem::transmute(texture) };
    Ok(SizedSdlTexture {
        tex: texture,
        byte_len: pixels * 4,
    })
}

fn rasterize_label(
    _key: &RasterRunKey,
    ctx: RasterCtx<'_>,
) -> Result<SizedSdlTexture, RenderError> {
    let width = ctx.bounds.width;
    let height = ctx.bounds.height;
    let byte_stride = usize::from(width) * 4;
    let byte_len = byte_stride * usize::from(height);
    let (max_width, max_height) = ctx.device.max_texture_size;
    if byte_len > RUN_BUDGET
        || (max_width > 0 && u32::from(width) > max_width)
        || (max_height > 0 && u32::from(height) > max_height)
    {
        return Err(RenderError::ResourceLimit(
            crate::render::RenderResource::Texture,
        ));
    }
    rasterize_run_pixels(
        ctx.content,
        ctx.font,
        *ctx.color,
        ctx.bounds,
        ctx.scale,
        ctx.raster_buf,
    )?;

    let _lease = ctx.device.canvas()?;
    let mut new_tex = ctx
        .device
        .creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, width.into(), height.into())
        .map_err(|_| RenderError::BackendFailure)?;
    new_tex
        .update(None, ctx.raster_buf, byte_stride)
        .map_err(|_| RenderError::BackendFailure)?;
    new_tex.set_blend_mode(sdl2::render::BlendMode::Blend);
    if unsafe {
        sdl2_sys::SDL_SetTextureScaleMode(new_tex.raw(), SDL_ScaleMode::SDL_ScaleModeLinear)
    } != 0
    {
        return Err(RenderError::BackendFailure);
    }

    // The cache drops each texture before its creator.
    let new_tex_static: SdlTexture<'static> = unsafe { core::mem::transmute(new_tex) };
    Ok(SizedSdlTexture {
        tex: new_tex_static,
        byte_len,
    })
}

fn rasterize_run_pixels(
    content: RasterContent<'_>,
    font: &Font,
    color: Color,
    bounds: RasterRunBounds,
    scale: Fixed,
    pixels: &mut Vec<u8>,
) -> Result<(), crate::render::RenderError> {
    pixels.resize(
        usize::from(bounds.width) * usize::from(bounds.height) * 4,
        0,
    );
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[color.r, color.g, color.b, 0]);
    }
    {
        let tex = MiruiTexture::new(pixels, bounds.width, bounds.height, ColorFormat::RGBA8888)?;
        let mut sw = SwRenderer::new(tex).with_alpha_mode(AlphaMode::Blend);
        sw.viewport = Viewport::new(bounds.width, bounds.height, scale);
        let area = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: bounds.size.x,
            h: bounds.size.y,
        };
        let RasterContent::Positioned { glyphs, origin } = content;
        let raster_color = Color { a: 255, ..color };
        sw.draw_glyph_run(&origin, glyphs, font, &area, &raster_color, 255);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::font::FontSurfaceId;

    #[test]
    fn scalar_admission_checks_actual_representation_and_surface_budget() {
        let font = Font::bitmap_8x8();
        let mut raster = font
            .raster_for_output(font.map_char('A').unwrap(), 8, 8)
            .unwrap();
        assert!(scalar_admitted(raster, (8, 760)));
        assert!(!scalar_admitted(raster, (7, 760)));
        assert!(!scalar_admitted(raster, (8, 759)));
        let bytes = alloc::vec![0; 1024 * 1025];
        raster.surface = GlyphSurface::new(
            &bytes,
            1024,
            1025,
            1024,
            mirx::image::SampleLayout::A8,
            mirx::types::ByteAlignment::ONE,
            FontSurfaceId::new(99),
        )
        .unwrap();
        assert!(!scalar_admitted(raster, (4096, 4096)));
        let sdf = Font::from_mirx(
            "Play",
            24,
            include_bytes!("../../../gallery/demos/assets/play_ui.mirx"),
            &mirx::reader::PayloadLimits::HOST,
        )
        .unwrap();
        assert!(!scalar_admitted(
            sdf.raster_for_output(sdf.map_char('A').unwrap(), 24, 48)
                .unwrap(),
            (4096, 4096)
        ));
    }

    #[test]
    fn glyph_opacity_preserves_run_rounding_and_low_alpha() {
        for (alpha, opacity, expected) in [
            (0, 255, 0),
            (1, 128, 0),
            (127, 128, 63),
            (128, 255, 128),
            (255, 255, 255),
        ] {
            assert_eq!(glyph_opacity(alpha, opacity), expected);
        }
    }

    #[test]
    fn cached_runs_preserve_coverage_and_filterable_rgb() {
        let mut pixels = Vec::new();
        for size in [14, 48] {
            let font = Font::from_mirx(
                "UI",
                size,
                include_bytes!("../../../gallery/demos/assets/misans_ui.mirx"),
                &mirx::reader::PayloadLimits::HOST,
            )
            .unwrap();
            let glyphs = [textflow::shaping::PositionedGlyph::new(
                font.map_char('A').unwrap(),
                textflow::shaping::FlowPoint { x: 0, y: 0 },
            )];
            let scale = Fixed::from_int(2);
            let bounds = font.raster_run_bounds(&glyphs, size * 2, scale).unwrap();
            let origin = Point {
                x: -bounds.offset.x,
                y: -bounds.offset.y,
            };
            let color = Color::rgba(146, 204, 255, 128);
            rasterize_run_pixels(
                RasterContent::Positioned {
                    glyphs: &glyphs,
                    origin,
                },
                &font,
                color,
                bounds,
                scale,
                &mut pixels,
            )
            .unwrap();
            assert!(
                pixels
                    .chunks_exact(4)
                    .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
            );
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
            for pixel in pixels.chunks_exact(4) {
                assert_eq!(&pixel[..3], &[color.r, color.g, color.b]);
            }
        }
    }

    #[test]
    fn scalar_sampling_preserves_texel_scale_and_clamps_each_region() {
        let surface = GlyphSurface::new(
            &[0; 64],
            8,
            8,
            8,
            mirx::image::SampleLayout::A8,
            mirx::types::ByteAlignment::ONE,
            FontSurfaceId::new(1),
        )
        .unwrap();
        for width in [1, 2, 4] {
            let quad = crate::render::font::RasterQuad {
                rect: Rect::new(0, 0, width, width),
                transform: Transform::IDENTITY,
            };
            let region = mirx::image::Region::new(2, 2, width as u32, width as u32).unwrap();
            let (vertices, _) = LabelCache::scalar_glyph_geometry(
                0,
                quad,
                region,
                surface,
                Color::rgb(255, 255, 255),
                255,
                Viewport::new(20, 20, Fixed::ONE),
            )
            .unwrap();
            for step in 0..=32 {
                let position = width as f32 * step as f32 / 32.0;
                let expected = (2.0 + position.clamp(0.5, width as f32 - 0.5)) / 8.0;
                for vertical in [false, true] {
                    let axis = |index: usize| {
                        if vertical {
                            (
                                vertices[index * 4].position.y,
                                vertices[index * 4].tex_coord.y,
                            )
                        } else {
                            (vertices[index].position.x, vertices[index].tex_coord.x)
                        }
                    };
                    let (left, right) = (0..3)
                        .map(|index| (axis(index), axis(index + 1)))
                        .find(|(left, right)| {
                            right.0 > left.0 && position >= left.0 && position <= right.0
                        })
                        .unwrap();
                    let actual =
                        left.1 + (right.1 - left.1) * (position - left.0) / (right.0 - left.0);
                    assert!(
                        (actual - expected).abs() < 1e-6,
                        "width={width}, position={position}"
                    );
                }
            }
        }
    }

    #[test]
    fn empty_clips_do_not_become_unrestricted_sdl_clips() {
        for rect in [
            Rect::new(1, 2, 0, 3),
            Rect::new(1, 2, 3, 0),
            Rect::new(1, 2, -1, 3),
        ] {
            assert!(sdl_pixel_rect(&rect, &rect).is_none());
        }
        assert_eq!(
            sdl_pixel_rect(&Rect::new(-1, 2, 3, 4), &Rect::new(-1, 2, 3, 4)).unwrap(),
            sdl2::rect::Rect::new(-1, 2, 3, 4)
        );
    }

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

        assert_eq!(&indices[..6], &[4, 5, 9, 4, 9, 8]);
        assert!(indices.into_iter().all(|index| (4..20).contains(&index)));
        assert_eq!(
            (vertices[0].position.x, vertices[0].position.y),
            (12.0, 18.0)
        );
        assert_eq!(
            (vertices[15].position.x, vertices[15].position.y),
            (20.0, 24.0)
        );
        assert_eq!(
            (vertices[0].tex_coord.x, vertices[0].tex_coord.y),
            (0.3125, 0.1875)
        );
        assert_eq!(
            (vertices[15].tex_coord.x, vertices[15].tex_coord.y),
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
