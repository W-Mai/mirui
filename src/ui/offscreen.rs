//! Retained widget output pixels. [`OffscreenRender`] renders an entity's subtree into a buffer covering its local paint bounds, then composites the completed buffer at that logical footprint. A scale below one reduces the buffer resolution; group opacity applies during composition.
//!
//! [`Outputs<WidgetTexture>`](crate::ui::Outputs) owns the pixel storage and published records for complete entity identities. The internal input key includes the source layout rectangle, local paint footprint, effective scale, pixel size, format, and dirty generation. An unchanged key reuses a completed result; rebuilding a same-size output reuses its allocated pixel slot.
//!
//! A texture becomes readable only after its subtree renders successfully. Preparing a referenced source does not composite it into the parent target; the main tree traversal determines composition order. An unfinished or failed write is unavailable through `get`, and failure of another output does not discard an already completed result.
//!
//! Typed callbacks read the resource with `textures: res!(Outputs<WidgetTexture>)`. `textures.get(entity)` returns a borrowed [`WidgetTextureRead`] with the completed texture, source layout rectangle, and local logical bounds from the same publication. The texture can hold CPU pixels or native backend storage. The layout rectangle excludes scrolling and external transforms; local bounds include the prepared paint extent relative to that layout origin, rounded outward to cover the pixel footprint. Reading the output record does not produce pixels, read back native storage, or register a source dependency. [`Texture::read_pixels`](crate::render::texture::Texture::read_pixels) prepares and borrows CPU pixels through the texture's source; [`App::snapshot_widget`](crate::app::App::snapshot_widget) provides an independent owned copy.
//!
//! Outputs use one pixel slot by default. An owner that explicitly requests history, such as [`TemporalMix`](crate::ui::widgets::TemporalMix), uses two slots and reads its own frozen result through `previous(entity)`. History is the last successful output at the start of the drawing batch, remains fixed across dirty regions, and is independent of input-generation gaps. A first output or an owner without history has no previous result; changed logical dimensions, local footprint, effective scale, pixel dimensions, or format clear old history. Moving the source preserves its local history.
//!
//! [`App::take_widget_output`](crate::app::App::take_widget_output) consumes an independent completed output as [`WidgetTextureOwned`], retaining its geometry, original storage and output-budget charge. Current consumers, history and active drawing/storage use prevent extraction. The operation synchronizes observations without rendering a new frame. The source remains configured through `OffscreenRender`; native access retains its backend lifetime and thread requirements. Retained storage keeps its original charge until its storage owners release it, including after the source or App has been removed. Thread-bound native cleanup follows owner maintenance.
//!
//! The output budget includes retained slots, in-progress writes, source CPU copies and staging, and shared paint and native-binding workspace. Native admission checks storage and workspace before executing the View; insufficient native capacity selects software production without replaying the View. Backend destination-fallback buffers and upload caches retain their renderer-factory limits; metadata and other filters' scratch storage are additional costs. The default output budget is zero; [`App::with_offscreen_pool_budget`](crate::app::App::with_offscreen_pool_budget) configures it before widget construction.
//!
//! # Usage
//!
//! Configure the budget before composing nodes that request output storage:
//!
//! ```
//! use mirui::prelude::*;
//! use mirui::ui::OffscreenRender;
//!
//! #[compose]
//! fn cached_panel() {
//!     ui!(View(width: 128, height: 64) [OffscreenRender::new()] {});
//! }
//!
//! let mut app = App::headless(128, 64);
//! app.with_default_widgets();
//! let root = app.spawn_root().id();
//! // Include every concurrently retained pixel slot in the budget.
//! // Local RGBA output uses four bytes per pixel.
//! app.with_offscreen_pool_budget(64 * 1024);
//!
//! app.compose(root, cached_panel());
//! app.render().expect("render cached panel");
//! ```
//!
//! # When it pays off
//!
//! A subtree with unchanged pixels can reuse its completed output when another widget's damage covers it. Frequently changing subtrees still pay for rasterization and a subsequent composition step, so output storage alone does not make them cheaper to render.
//!
//! # Rendering constraints
//!
//! The outer renderer must provide an offscreen pixel format and accept texture composition. Subtree production uses native targets when the backend supports the required operations and the storage and workspace fit; otherwise it selects the CPU software renderer before drawing. Backend execution errors propagate without retrying the View on another renderer. An explicit `OffscreenRender` node renders inline when output storage is unavailable.
//!
//! Outputs contain local transparent pixels, independently of the parent framebuffer. Nested outputs and current dependencies are prepared in dependency order before composition; cyclic output dependencies are unavailable.

use crate::ecs::{Entity, World};
use crate::render::texture::ColorFormat;
use crate::types::{Fixed, Fixed64, Rect};

/// Mark an entity for offscreen rendering. Insert / remove to toggle.
///
/// Default ([`Self::new`]) renders at full resolution and only buys
/// caching. `with_scale(Fixed::HALF)` renders at half resolution and
/// upscales on present. `with_opacity(128)` blends the cached subtree
/// onto the outer surface at the chosen alpha.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct OffscreenRender {
    /// Render scale for the entity's local paint bounds. A value of 1.0 keeps the logical resolution; 0.5 halves both axes before outward pixel rounding. Values below `Fixed::ONE / 8` are clamped at render time.
    pub scale: Fixed,
    /// Group opacity applied when the cached buffer is blitted onto the
    /// outer surface (not a per-pixel filter inside the subtree).
    pub opacity: u8,
}

impl crate::ecs::Component for OffscreenRender {}

/// Select a fallback opacity when either member of a paired composition is unavailable.
pub(crate) struct CompositeFallback {
    pub required: crate::ecs::Entity,
    pub opacity: u8,
}
impl crate::ecs::Component for CompositeFallback {}

impl Default for OffscreenRender {
    fn default() -> Self {
        Self::new()
    }
}

impl OffscreenRender {
    pub const fn new() -> Self {
        Self {
            scale: Fixed::ONE,
            opacity: 255,
        }
    }

    pub const fn with_scale(scale: Fixed) -> Self {
        Self {
            scale,
            opacity: 255,
        }
    }

    pub const fn with_opacity(mut self, opacity: u8) -> Self {
        self.opacity = opacity;
        self
    }
}

/// Cache-invalidation counter. Bumped by the dirty walker when any
/// descendant of an `OffscreenRender` entity carries `Dirty`. Lives on
/// the same entity as `OffscreenRender`; default 0 on first render.
#[derive(Clone, Copy, Debug, Default)]
pub struct OffscreenGeneration(pub u32);

impl crate::ecs::Component for OffscreenGeneration {}

/// Input identity for one widget texture. Entity identity, source layout rectangle, local pixel footprint, effective scale, pixel dimensions, format, and content generation determine whether the published result can be reused; historical output selection is independent of this key.
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) struct BufferKey {
    pub entity: Entity,
    pub layout_rect: Rect,
    pub local_bounds: Rect,
    pub scale: Fixed,
    pub w: u16,
    pub h: u16,
    pub format: ColorFormat,
    pub generation: u32,
}

impl BufferKey {
    pub(crate) fn for_widget(
        entity: Entity,
        layout_rect: Rect,
        local_bounds: Rect,
        offscreen: OffscreenRender,
        generation: u32,
    ) -> Self {
        let scale = offscreen.scale.max(Fixed::ONE / 8);
        let geometry =
            quantized_axis(local_bounds.x, local_bounds.w, scale).and_then(|(x, width, w)| {
                let (y, height, h) = quantized_axis(local_bounds.y, local_bounds.h, scale)?;
                Some((
                    Rect {
                        x,
                        y,
                        w: width,
                        h: height,
                    },
                    w,
                    h,
                ))
            });
        let (local_bounds, w, h) = geometry.unwrap_or((Rect::ZERO, 0, 0));
        Self {
            entity,
            layout_rect,
            local_bounds,
            scale,
            w,
            h,
            format: ColorFormat::RGBA8888,
            generation,
        }
    }
}

fn quantized_axis(origin: Fixed, extent: Fixed, scale: Fixed) -> Option<(Fixed, Fixed, u16)> {
    if extent <= Fixed::ZERO {
        return None;
    }
    let origin = Fixed64::from_fixed(origin);
    let end = origin + Fixed64::from_fixed(extent);
    let scale = Fixed64::from_fixed(scale);
    // Q24.8 endpoints and scale fit Q48.16 after wide multiplication.
    let first = origin.mul_wide(scale).to_int();
    let last = end.mul_wide(scale);
    let floor = last.to_int();
    let last = floor.checked_add(i64::from(last > Fixed64::from_int(floor)))?;
    let pixels = u16::try_from(last.checked_sub(first)?).ok()?;
    if pixels == 0 {
        return None;
    }
    let origin = outward_logical_edge(first, scale, false)?;
    let end = outward_logical_edge(last, scale, true)?;
    let extent = checked_geometry_fixed(Fixed64::from_fixed(end) - Fixed64::from_fixed(origin))?;
    (extent > Fixed::ZERO).then_some((origin, extent, pixels))
}

fn outward_logical_edge(pixel: i64, scale: Fixed64, upper: bool) -> Option<Fixed> {
    // Fixed64::from_int shifts by 16; reject an unrepresentable endpoint first.
    pixel.checked_mul(65_536)?;
    let pixel = Fixed64::from_int(pixel);
    let candidate = checked_geometry_fixed(pixel.div_wide(scale))?;
    let candidate = Fixed64::from_fixed(candidate);
    let projected = candidate.mul_wide(scale);
    let quantum = Fixed64::from_ratio(1, 256);
    // Division and narrowing can truncate across a pixel boundary, including a negative edge.
    let candidate = if upper && projected < pixel {
        candidate + quantum
    } else if !upper && projected > pixel {
        candidate - quantum
    } else {
        candidate
    };
    checked_geometry_fixed(candidate)
}

fn checked_geometry_fixed(value: Fixed64) -> Option<Fixed> {
    (value >= Fixed64::from_fixed(Fixed::MIN) && value <= Fixed64::from_fixed(Fixed::MAX))
        .then(|| value.to_fixed())
}

mod output;
#[cfg(feature = "perf")]
pub(crate) use output::AdmissionTrace;
pub(crate) use output::TextureBaseline;
pub use output::{WidgetTexture, WidgetTextureOwned, WidgetTextureRead, WidgetTextureStorage};

pub(crate) fn texture_requested(world: &World, entity: Entity) -> bool {
    world.is_alive(entity)
        && world
            .resource::<crate::ui::Outputs<WidgetTexture>>()
            .is_some_and(|outputs| outputs.is_requested(entity))
}

pub(crate) fn has_texture_requests(world: &World) -> bool {
    world
        .resource::<crate::ui::Outputs<WidgetTexture>>()
        .is_some_and(crate::ui::Outputs::has_requests)
}

pub(crate) fn config(world: &World, entity: Entity) -> Option<OffscreenRender> {
    world
        .get::<OffscreenRender>(entity)
        .copied()
        .or_else(|| texture_requested(world, entity).then(OffscreenRender::default))
}

pub(crate) fn reconcile_texture_outputs(world: &World) {
    if let Some(outputs) = world.resource::<crate::ui::Outputs<WidgetTexture>>() {
        outputs.reconcile_textures(
            |entity| world.get::<OffscreenRender>(entity).is_some(),
            |entity| {
                !super::branch::is_hidden_in_tree(world, entity)
                    && world
                        .resource::<super::WidgetRoot>()
                        .is_none_or(|root| super::branch::is_in_subtree(world, root.0, entity))
            },
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ui/offscreen/tests.rs"]
mod tests;
