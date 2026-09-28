//! Texture upload cache — each `Texture` is uploaded once into an
//! `OffscreenCanvas` so blits skip the wasm/JS boundary per frame.

#![cfg(target_arch = "wasm32")]

use alloc::vec::Vec;

use js_sys::Uint8ClampedArray;
use wasm_bindgen::Clamped;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{ImageData, OffscreenCanvas, OffscreenCanvasRenderingContext2d};

use crate::core::cache::{Cache, HasSize, HashLookup, Lru, MaxSize};
use crate::render::backends::sw::{SwRenderer, SwScratch};
use crate::render::backends::web_glyph_slot::{SlotSelection, select_slot};
use crate::render::backends::web_raster_budget::RasterScratchPlan;
use crate::render::canvas::Canvas;
use crate::render::font::{Font, RasterRunBounds, RasterRunKey};
use crate::render::renderer::{RenderError, RenderResource, TextRunIdentity};
use crate::render::texture::{AlphaMode, ColorFormat, Texture};
use crate::types::{Color, Fixed, Point, Rect, Viewport};

use super::WebScratchResourceUsage;

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

const GLYPH_CACHE_BUDGET: usize = 8 * 1024 * 1024;
const SCRATCH_GENERATION_RGBA_BUDGET: usize = 8 * 1024 * 1024;
pub(super) const SCRATCH_RESIZE_RUST_REQUEST_BUDGET: usize = 8 * 1024 * 1024;

pub type GlyphPool = Cache<
    crate::render::font::RasterRunKey,
    CachedOffscreen,
    Lru,
    HashLookup<crate::render::font::RasterRunKey>,
>;

pub fn new_glyph_pool() -> GlyphPool {
    Cache::builder()
        .max_size(MaxSize::Bytes(GLYPH_CACHE_BUDGET))
        .build()
}

struct BoundedGlyphSlot {
    identity: Option<TextRunIdentity>,
    key: Option<RasterRunKey>,
    rgba: Vec<u8>,
    last_used: u64,
    canvas: OffscreenCanvas,
    context: OffscreenCanvasRenderingContext2d,
    image_data: ImageData,
    js_pixels: Uint8ClampedArray,
    rust_view: Option<Uint8ClampedArray>,
}

pub(super) fn scratch_plan(
    width: u16,
    height: u16,
    slot_count: usize,
) -> Result<RasterScratchPlan, RenderError> {
    RasterScratchPlan::new(
        width,
        height,
        slot_count,
        core::mem::size_of::<BoundedGlyphSlot>(),
        SCRATCH_GENERATION_RGBA_BUDGET,
    )
}

#[derive(Clone, Copy)]
pub(super) struct BoundedGlyphRunKey {
    pub identity: TextRunIdentity,
    pub raster: RasterRunKey,
}

/// Fixed retained RGBA slots with matching browser upload surfaces.
pub struct BoundedGlyphScratch {
    plan: RasterScratchPlan,
    slots: Vec<BoundedGlyphSlot>,
    use_clock: u64,
    evictions: u64,
    sw_scratch: SwScratch,
}

impl BoundedGlyphScratch {
    pub fn new(
        plan: RasterScratchPlan,
        usage: &mut WebScratchResourceUsage,
    ) -> Result<Self, RenderError> {
        plan.checked_resize_peak(None, SCRATCH_RESIZE_RUST_REQUEST_BUDGET)?;
        let bytes = plan.rgba_bytes_per_slot;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(plan.slots)
            .map_err(|_| RenderError::ResourceLimit(RenderResource::Target))?;
        usage.rust_slot_capacity_bytes =
            slots.capacity() * core::mem::size_of::<BoundedGlyphSlot>();
        for _ in 0..plan.slots {
            let mut rgba = Vec::new();
            rgba.try_reserve_exact(bytes)
                .map_err(|_| RenderError::ResourceLimit(RenderResource::Target))?;
            usage.rust_slot_capacity_bytes = usage
                .rust_slot_capacity_bytes
                .saturating_add(rgba.capacity());
            rgba.resize(bytes, 0);
            let canvas = OffscreenCanvas::new(u32::from(plan.width), u32::from(plan.height))
                .map_err(|_| RenderError::BackendFailure)?;
            usage.offscreen_canvases += 1;
            usage.canvas_nominal_pixel_bytes =
                usage.canvas_nominal_pixel_bytes.saturating_add(bytes);
            let context = canvas
                .get_context("2d")
                .map_err(|_| RenderError::BackendFailure)?
                .ok_or(RenderError::BackendFailure)?
                .dyn_into::<OffscreenCanvasRenderingContext2d>()
                .map_err(|_| RenderError::BackendFailure)?;
            let image_data = ImageData::new_with_sw(u32::from(plan.width), u32::from(plan.height))
                .map_err(|_| RenderError::BackendFailure)?;
            usage.image_data_objects += 1;
            usage.image_data_pixel_bytes = usage.image_data_pixel_bytes.saturating_add(bytes);
            // web-sys's typed `ImageData::data()` getter copies into a Rust Vec.
            let js_pixels = js_sys::Reflect::get(image_data.as_ref(), &JsValue::from_str("data"))
                .map_err(|_| RenderError::BackendFailure)?
                .dyn_into::<Uint8ClampedArray>()
                .map_err(|_| RenderError::BackendFailure)?;
            if js_pixels.length() as usize != bytes {
                return Err(RenderError::BackendFailure);
            }
            slots.push(BoundedGlyphSlot {
                identity: None,
                key: None,
                rgba,
                last_used: 0,
                canvas,
                context,
                image_data,
                js_pixels,
                rust_view: None,
            });
        }
        for slot in &mut slots {
            // SAFETY: each RGBA Vec keeps its allocation and length for the
            // lifetime of the slot. A memory.grow detaches this JS view; the
            // upload path replaces it before reading the pixels again.
            slot.rust_view = Some(unsafe { Uint8ClampedArray::view(&slot.rgba) });
        }

        Ok(Self {
            plan,
            slots,
            use_clock: 0,
            evictions: 0,
            sw_scratch: SwScratch::new(),
        })
    }

    pub fn dimensions(&self) -> (u16, u16) {
        (self.plan.width, self.plan.height)
    }

    pub fn plan(&self) -> RasterScratchPlan {
        self.plan
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    pub fn fits(&self, width: u16, height: u16) -> bool {
        width <= self.plan.width && height <= self.plan.height
    }

    pub fn canvas(&self, index: usize) -> &OffscreenCanvas {
        &self.slots[index].canvas
    }

    pub fn upload_bytes(&self) -> usize {
        self.slots[0].rgba.len()
    }

    pub fn resident_rust_bytes(&self) -> usize {
        self.slots.capacity() * core::mem::size_of::<BoundedGlyphSlot>()
            + self
                .slots
                .iter()
                .map(|slot| slot.rgba.capacity())
                .sum::<usize>()
    }

    pub fn resident_usage(&self) -> WebScratchResourceUsage {
        WebScratchResourceUsage {
            rust_slot_capacity_bytes: self.resident_rust_bytes(),
            offscreen_canvases: self.slots.len(),
            image_data_objects: self.slots.len(),
            image_data_pixel_bytes: self.plan.rgba_bytes_total,
            canvas_nominal_pixel_bytes: self.plan.rgba_bytes_total,
        }
    }

    pub fn rasterize_and_upload(
        &mut self,
        run: BoundedGlyphRunKey,
        bounds: RasterRunBounds,
        scale: Fixed,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &Font,
        color: &Color,
    ) -> Result<(usize, bool), RenderError> {
        if bounds.width == 0 || bounds.height == 0 || !self.fits(bounds.width, bounds.height) {
            return Err(RenderError::InvalidGeometry);
        }
        let selection = select_slot(
            self.slots
                .iter()
                .map(|slot| (slot.identity, slot.key, slot.last_used)),
            run.identity,
            run.raster,
        )
        .ok_or(RenderError::InvalidGeometry)?;
        let index = match selection {
            SlotSelection::Hit(index)
            | SlotSelection::Rewrite(index)
            | SlotSelection::Free(index)
            | SlotSelection::Replace(index) => index,
        };
        self.use_clock = self.use_clock.wrapping_add(1);
        let slot = &mut self.slots[index];
        if selection == SlotSelection::Hit(index) {
            slot.last_used = self.use_clock;
            return Ok((index, true));
        }
        if selection == SlotSelection::Replace(index) {
            self.evictions = self.evictions.saturating_add(1);
        }
        slot.identity = Some(run.identity);
        slot.key = None;
        let stride = usize::from(self.plan.width) * 4;
        let used_row_bytes = usize::from(bounds.width) * 4;
        for row in slot
            .rgba
            .chunks_exact_mut(stride)
            .take(usize::from(bounds.height))
        {
            row[..used_row_bytes].fill(0);
        }
        {
            let mut texture = Texture::new(
                &mut slot.rgba,
                bounds.width,
                bounds.height,
                ColorFormat::RGBA8888,
            );
            texture.stride = stride;
            texture.alpha_mode = AlphaMode::Blend;
            let mut sw = SwRenderer::with_scratch(texture, &mut self.sw_scratch);
            sw.viewport = Viewport::new(bounds.width, bounds.height, scale);
            let origin = Point {
                x: -bounds.offset.x,
                y: -bounds.offset.y,
            };
            let full = Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: bounds.size.x,
                h: bounds.size.y,
            };
            let raster_color = Color { a: 255, ..*color };
            sw.draw_glyph_run(&origin, glyphs, font, &full, &raster_color, 255);
        }

        // `drawImage` samples just the occupied rectangle. Duplicate its last
        // row and column across the retained surface to avoid stale texels at
        // the right and bottom edges under browser interpolation.
        for y in 0..usize::from(bounds.height) {
            let row_start = y * stride;
            let edge_start = row_start + used_row_bytes - 4;
            let edge = [
                slot.rgba[edge_start],
                slot.rgba[edge_start + 1],
                slot.rgba[edge_start + 2],
                slot.rgba[edge_start + 3],
            ];
            for x in usize::from(bounds.width)..usize::from(self.plan.width) {
                let dst = row_start + x * 4;
                slot.rgba[dst..dst + 4].copy_from_slice(&edge);
            }
        }
        let bottom_start = (usize::from(bounds.height) - 1) * stride;
        for y in usize::from(bounds.height)..usize::from(self.plan.height) {
            slot.rgba
                .copy_within(bottom_start..bottom_start + stride, y * stride);
        }
        if slot
            .rust_view
            .as_ref()
            .is_none_or(|view| view.byte_length() as usize != slot.rgba.len())
        {
            // SAFETY: the RGBA Vec is fixed in size and never reallocated.
            // The old view may have been detached by Wasm memory growth.
            slot.rust_view = Some(unsafe { Uint8ClampedArray::view(&slot.rgba) });
        }
        slot.js_pixels
            .set(slot.rust_view.as_ref().expect("initialized view"), 0);
        slot.context
            .put_image_data(&slot.image_data, 0.0, 0.0)
            .map_err(|_| RenderError::BackendFailure)?;
        slot.key = Some(run.raster);
        slot.last_used = self.use_clock;
        Ok((index, false))
    }
}

pub fn upload(src: &Texture) -> Option<CachedOffscreen> {
    let rgba = src.rgba8_upload_pixels()?;
    let canvas = OffscreenCanvas::new(src.width as u32, src.height as u32).ok()?;
    let ctx = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<OffscreenCanvasRenderingContext2d>()
        .ok()?;
    let image_data = ImageData::new_with_u8_clamped_array_and_sh(
        Clamped(rgba.as_ref()),
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
