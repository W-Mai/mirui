//! Retained widget output pixels. [`OffscreenRender`] renders an entity's subtree into a buffer covering its local paint bounds, then composites the completed buffer at that logical footprint. A scale below one reduces the buffer resolution; group opacity applies during composition.
//!
//! [`Outputs<WidgetTexture>`](crate::ui::Outputs) owns the pixel storage and published records for complete entity identities. The internal input key includes the source layout rectangle, local paint footprint, effective scale, pixel size, format, and dirty generation. An unchanged key reuses a completed result; rebuilding a same-size output reuses its allocated pixel slot.
//!
//! A texture becomes readable only after its subtree renders successfully. Preparing a referenced source does not composite it into the parent target; the main tree traversal determines composition order. An unfinished or failed write is unavailable through `get`, and failure of another output does not discard an already completed result.
//!
//! Typed callbacks read the resource with `textures: res!(Outputs<WidgetTexture>)`. `textures.get(entity)` returns a borrowed [`WidgetTextureRead`] with the completed CPU texture, source layout rectangle, and local logical bounds from the same publication. The layout rectangle excludes scrolling and external transforms; local bounds include the prepared paint extent relative to that layout origin, rounded outward to cover the pixel footprint. Reads do not produce pixels or register a source dependency. [`App::snapshot_widget`](crate::app::App::snapshot_widget) provides an owned pixel copy when a result must outlive that borrow.
//!
//! Outputs use one pixel slot by default. An owner that explicitly requests history, such as [`TemporalMix`](crate::ui::widgets::TemporalMix), uses two slots and reads its own frozen result through `previous(entity)`. History is the last successful output at the start of the drawing batch, remains fixed across dirty regions, and is independent of input-generation gaps. A first output or an owner without history has no previous result; changed logical dimensions, local footprint, effective scale, pixel dimensions, or format clear old history. Moving the source preserves its local history.
//!
//! The pixel budget includes retained slots, detached in-progress write buffers, and the shared Mirror/Shadow/Glow/TemporalMix workspace. A request that exceeds it is rejected before allocating pixels, and other outputs remain resident. Metadata and other filters' scratch storage are additional costs. The default budget is zero; [`App::with_offscreen_pool_budget`](crate::app::App::with_offscreen_pool_budget) configures it before widget construction.
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
//! The outer renderer must provide an offscreen pixel format and accept texture composition. Subtree production uses the CPU software renderer, including when the outer backend uses a GPU. An explicit `OffscreenRender` node renders inline when output storage is unavailable.
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
pub use output::{WidgetTexture, WidgetTextureRead, WidgetTextureStorage};

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
mod tests {
    use super::*;
    use crate::types::Color;
    use crate::ui::outputs::Outputs;

    fn dummy_entity(id: u32) -> Entity {
        Entity { id, generation: 0 }
    }

    fn texture_key(id: u32, generation: u32) -> BufferKey {
        BufferKey {
            entity: dummy_entity(id),
            layout_rect: Rect {
                w: Fixed::from_int(40),
                h: Fixed::from_int(24),
                ..Rect::ZERO
            },
            local_bounds: Rect::new(0, 0, 40, 24),
            scale: Fixed::ONE,
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation,
        }
    }

    #[test]
    fn texture_is_unavailable_until_successfully_published() {
        let outputs = Outputs::<WidgetTexture>::with_budget(64 * 1024);
        let key = texture_key(1, 0);
        let mut write = outputs.prepare_texture(key).expect("texture storage");
        write.texture_mut().set_pixel(0, 0, &Color::rgb(17, 31, 47));
        assert!(outputs.get(key.entity).is_none());
        assert!(outputs.previous(key.entity).is_none());

        assert!(write.publish());
        assert_eq!(
            outputs.get(key.entity).unwrap().texture().get_pixel(0, 0),
            Color::rgb(17, 31, 47)
        );
    }

    #[test]
    fn offscreen_render_default_is_fully_opaque() {
        assert_eq!(OffscreenRender::default().opacity, 255);
        assert_eq!(OffscreenRender::new().opacity, 255);
        assert_eq!(OffscreenRender::with_scale(Fixed::HALF).opacity, 255);
    }

    #[test]
    fn offscreen_render_with_opacity_chains() {
        let off = OffscreenRender::new().with_opacity(128);
        assert_eq!(off.scale, Fixed::ONE);
        assert_eq!(off.opacity, 128);

        let off2 = OffscreenRender::with_scale(Fixed::HALF).with_opacity(64);
        assert_eq!(off2.scale, Fixed::HALF);
        assert_eq!(off2.opacity, 64);
    }

    #[test]
    fn output_storage_starts_without_published_textures() {
        let outputs = Outputs::<WidgetTexture>::with_budget(64 * 1024);
        assert!(outputs.get(dummy_entity(1)).is_none());
        assert!(outputs.previous(dummy_entity(1)).is_none());
    }

    #[test]
    fn default_output_budget_rejects_texture_preparation() {
        let outputs = Outputs::<WidgetTexture>::default();
        let key = texture_key(1, 0);
        assert!(outputs.prepare_texture(key).is_none());
        assert!(outputs.get(key.entity).is_none());
    }

    #[test]
    fn buffer_key_distinguishes_entities() {
        let k1 = BufferKey {
            entity: dummy_entity(1),
            ..texture_key(1, 0)
        };
        let k2 = BufferKey {
            entity: dummy_entity(2),
            ..texture_key(2, 0)
        };
        assert_ne!(k1, k2);
    }

    #[test]
    fn buffer_key_distinguishes_generations() {
        let k1 = texture_key(1, 0);
        let k2 = BufferKey {
            generation: 1,
            ..k1
        };
        assert_ne!(k1, k2);
    }

    #[test]
    fn fractional_bounds_round_outward_before_allocating_pixels() {
        let bounds = Rect::new(0, 0, Fixed::from_ratio(7, 4), Fixed::from_ratio(7, 4));
        let key = BufferKey::for_widget(
            dummy_entity(1),
            bounds,
            bounds,
            OffscreenRender::with_scale(Fixed::from_int(2)),
            0,
        );
        assert_eq!((key.w, key.h), (4, 4));
        assert_eq!(key.local_bounds, Rect::new(0, 0, 2, 2));
        assert_eq!(key.layout_rect, bounds);
        assert_eq!(key.scale, Fixed::from_int(2));
    }

    #[test]
    fn negative_origins_and_fractional_scales_preserve_both_pixel_edges() {
        let bounds = Rect::new(
            -Fixed::ONE / 4,
            -Fixed::from_ratio(3, 4),
            Fixed::from_ratio(7, 4),
            Fixed::from_ratio(5, 2),
        );
        for (scale, pixels, physical) in [
            (Fixed::ONE / 8, (2, 2), (-1, -1, 1, 1)),
            (Fixed::HALF, (2, 2), (-1, -1, 1, 1)),
            (Fixed::ONE, (3, 3), (-1, -1, 2, 2)),
            (Fixed::from_ratio(3, 2), (4, 5), (-1, -2, 3, 3)),
            (Fixed::from_int(2), (4, 6), (-1, -2, 3, 4)),
        ] {
            let key = BufferKey::for_widget(
                dummy_entity(1),
                Rect::new(11, 13, 1, 1),
                bounds,
                OffscreenRender::with_scale(scale),
                0,
            );
            assert_eq!((key.w, key.h), pixels);
            assert_eq!(key.scale, scale);
            let scale = Fixed64::from_fixed(scale);
            let left = Fixed64::from_fixed(key.local_bounds.x);
            let top = Fixed64::from_fixed(key.local_bounds.y);
            let right = left + Fixed64::from_fixed(key.local_bounds.w);
            let bottom = top + Fixed64::from_fixed(key.local_bounds.h);
            assert!(left.mul_wide(scale) <= Fixed64::from_int(physical.0));
            assert!(top.mul_wide(scale) <= Fixed64::from_int(physical.1));
            assert!(right.mul_wide(scale) >= Fixed64::from_int(physical.2));
            assert!(bottom.mul_wide(scale) >= Fixed64::from_int(physical.3));
        }

        let bounds = Rect::new(-1, -1, 2, 2);
        for requested_scale in [Fixed::MIN, Fixed::ZERO, Fixed::ONE / 16] {
            let key = BufferKey::for_widget(
                dummy_entity(1),
                bounds,
                bounds,
                OffscreenRender::with_scale(requested_scale),
                0,
            );
            assert_eq!(key.scale, Fixed::ONE / 8);
            assert_eq!((key.w, key.h), (2, 2));
            assert_eq!(key.local_bounds, Rect::new(-8, -8, 16, 16));
        }
    }

    #[test]
    fn unrepresentable_pixel_or_logical_footprints_are_missing_instead_of_clamped() {
        let layout = Rect::new(0, 0, 1, 1);
        for (bounds, scale) in [
            (Rect::new(0, 0, 65_536, 1), Fixed::ONE),
            (
                Rect::new(0, 0, Fixed::from_int(65_535) + Fixed::ONE / 256, 1),
                Fixed::ONE,
            ),
            (Rect::new(Fixed::MAX, 0, Fixed::ONE, 1), Fixed::ONE),
            (Rect::new(Fixed::MAX, 0, Fixed::MAX, 1), Fixed::MAX),
            (Rect::new(Fixed::MIN, 0, Fixed::MAX, 1), Fixed::MAX),
            (Rect::new(0, 0, Fixed::MAX, 1), Fixed::ONE / 8),
            (Rect::new(0, 0, 0, 1), Fixed::ONE),
            (Rect::new(0, 0, 1, -1), Fixed::ONE),
        ] {
            let key = BufferKey::for_widget(
                dummy_entity(1),
                layout,
                bounds,
                OffscreenRender::with_scale(scale),
                0,
            );
            assert_eq!((key.w, key.h), (0, 0));
            assert_eq!(key.local_bounds, Rect::ZERO);
        }

        let key = BufferKey::for_widget(
            dummy_entity(1),
            layout,
            Rect::new(0, 0, 65_535, 1),
            OffscreenRender::default(),
            0,
        );
        assert_eq!((key.w, key.h), (u16::MAX, 1));
        let key = BufferKey::for_widget(
            dummy_entity(1),
            layout,
            Rect::new(Fixed::MIN, 0, 1, 1),
            OffscreenRender::default(),
            0,
        );
        assert_eq!((key.w, key.h), (1, 1));
        assert_eq!(key.local_bounds, Rect::new(Fixed::MIN, 0, 1, 1));
    }

    #[test]
    fn texture_preparation_preserves_requested_geometry() {
        let outputs = Outputs::<WidgetTexture>::with_budget(64 * 1024);
        let key = texture_key(1, 0);
        let write = outputs.prepare_texture(key).expect("texture storage");
        assert!(write.publish());
        let output = outputs.get(key.entity).expect("published texture");
        let tex = output.texture();
        assert_eq!(tex.width, 40);
        assert_eq!(tex.height, 24);
        assert_eq!(tex.format, ColorFormat::RGBA8888);
    }

    #[test]
    fn stable_texture_rewrites_reuse_prepared_storage() {
        let outputs = Outputs::<WidgetTexture>::with_budget(64 * 1024);
        let write = outputs
            .prepare_texture(texture_key(1, 0))
            .expect("initial texture storage");
        assert!(write.publish());

        let allocations = crate::test_support::tracked_allocations(|| {
            for generation in 1..8 {
                let _batch = outputs.begin_batch();
                let write = outputs
                    .prepare_texture(texture_key(1, generation))
                    .expect("reusable texture storage");
                assert!(write.publish());
                assert!(outputs.get(dummy_entity(1)).is_some());
            }
        });
        assert_eq!(allocations, 0);
    }

    #[test]
    fn cancelled_texture_write_does_not_publish_partial_pixels() {
        let outputs = Outputs::<WidgetTexture>::with_budget(64 * 1024);
        let key = texture_key(1, 0);
        assert!(outputs.prepare_texture(key).unwrap().publish());

        let mut write = outputs
            .prepare_texture(texture_key(1, 7))
            .expect("replacement texture storage");
        write
            .texture_mut()
            .set_pixel(0, 0, &Color::rgb(200, 100, 50));
        assert!(outputs.get(key.entity).is_none());
        drop(write);
        assert!(outputs.get(key.entity).is_none());
        assert!(outputs.previous(key.entity).is_none());
    }

    #[test]
    fn texture_budget_rejects_new_outputs_without_evicting_live_sources() {
        // Two 40 by 24 RGBA textures fit; a third exceeds the live budget.
        let outputs = Outputs::<WidgetTexture>::with_budget(8 * 1024);
        assert!(
            outputs
                .prepare_texture(texture_key(1, 0))
                .unwrap()
                .publish()
        );
        let pending = outputs.prepare_texture(texture_key(2, 0)).unwrap();
        assert!(outputs.get(dummy_entity(1)).is_some());
        assert!(outputs.get(dummy_entity(2)).is_none());

        assert!(outputs.prepare_texture(texture_key(3, 0)).is_none());
        assert!(outputs.get(dummy_entity(1)).is_some());
        assert!(pending.publish());
        assert!(outputs.get(dummy_entity(2)).is_some());
        assert!(outputs.get(dummy_entity(3)).is_none());
    }

    #[test]
    fn oversized_texture_is_rejected_without_allocating_pixels() {
        let outputs = Outputs::<WidgetTexture>::with_budget(4 * 1024);
        outputs.reserve(1).expect("output metadata capacity");
        let key = BufferKey {
            entity: dummy_entity(1),
            layout_rect: Rect {
                w: Fixed::from_int(200),
                h: Fixed::from_int(200),
                ..Rect::ZERO
            },
            local_bounds: Rect::new(0, 0, 200, 200),
            scale: Fixed::ONE,
            w: 200,
            h: 200,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let allocations = crate::test_support::tracked_allocations(|| {
            assert!(outputs.prepare_texture(key).is_none());
        });
        assert_eq!(allocations, 0);
        assert!(outputs.get(key.entity).is_none());
    }

    #[test]
    fn history_freezes_successful_outputs_across_input_revision_gaps() {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * 40 * 24 * 4);
        let entity = dummy_entity(1);
        outputs.reserve(1).expect("output metadata capacity");
        outputs.set_retained(entity, true).expect("history slot");
        let first = Color::rgb(200, 10, 20);
        let second = Color::rgb(30, 40, 220);
        {
            let _batch = outputs.begin_batch();
            let mut write = outputs.prepare_texture(texture_key(1, 2)).unwrap();
            write.texture_mut().set_pixel(0, 0, &first);
            assert!(write.publish());
            assert!(outputs.previous(entity).is_none());
        }
        {
            let _batch = outputs.begin_batch();
            let mut write = outputs.prepare_texture(texture_key(1, 11)).unwrap();
            assert_eq!(
                outputs.previous(entity).unwrap().texture().get_pixel(0, 0),
                first
            );
            write.texture_mut().set_pixel(0, 0, &second);
            assert!(write.publish());
            assert_eq!(
                outputs.get(entity).unwrap().texture().get_pixel(0, 0),
                second
            );
            assert_eq!(
                outputs.previous(entity).unwrap().texture().get_pixel(0, 0),
                first
            );
            assert!(outputs.prepare_texture(texture_key(2, 0)).is_none());
        }
        {
            let _batch = outputs.begin_batch();
            let write = outputs.prepare_texture(texture_key(1, 19)).unwrap();
            drop(write);
            assert!(outputs.get(entity).is_none());
            assert_eq!(
                outputs.previous(entity).unwrap().texture().get_pixel(0, 0),
                second
            );
        }
    }

    #[test]
    fn trimming_outputs_releases_published_textures_and_live_budget() {
        let outputs = Outputs::<WidgetTexture>::with_budget(40 * 24 * 4);
        assert!(
            outputs
                .prepare_texture(texture_key(1, 0))
                .unwrap()
                .publish()
        );
        assert!(outputs.prepare_texture(texture_key(2, 0)).is_none());

        outputs.trim_memory();
        assert!(outputs.get(dummy_entity(1)).is_none());
        assert!(
            outputs
                .prepare_texture(texture_key(2, 0))
                .unwrap()
                .publish()
        );
    }

    #[test]
    fn retired_sources_release_their_output_and_pixel_budget() {
        for despawn in [false, true] {
            let mut world = World::default();
            world.insert_resource(Outputs::<WidgetTexture>::with_budget(40 * 24 * 4));
            let source = world.spawn_empty();
            world.insert(source, OffscreenRender::default());
            {
                let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
                let write = outputs
                    .prepare_texture(BufferKey {
                        entity: source,
                        ..texture_key(0, 1)
                    })
                    .unwrap();
                assert!(write.publish());
                assert!(outputs.get(source).is_some());
            }

            if despawn {
                assert!(world.despawn(source));
            } else {
                world.remove::<OffscreenRender>(source);
            }
            reconcile_texture_outputs(&world);

            let replacement = world.spawn_empty();
            world.insert(replacement, OffscreenRender::default());
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert!(outputs.get(source).is_none());
            let write = outputs
                .prepare_texture(BufferKey {
                    entity: replacement,
                    ..texture_key(0, 1)
                })
                .expect("retired source released the pixel budget");
            assert!(write.publish());
            assert!(outputs.get(replacement).is_some());
            assert!(outputs.get(source).is_none());
        }
    }

    #[test]
    fn hidden_branch_clears_output_history_and_preserves_retention_demand() {
        let mut world = World::default();
        world.insert_resource(Outputs::<WidgetTexture>::with_budget(2 * 40 * 24 * 4));
        let parent = world.spawn_empty();
        let source = world.spawn_empty();
        world.insert(source, crate::ui::Parent(parent));
        world.insert(source, OffscreenRender::default());
        {
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            outputs.reserve(1).unwrap();
            outputs.set_retained(source, true).unwrap();
            for generation in 1..=2 {
                let _batch = outputs.begin_batch();
                let write = outputs
                    .prepare_texture(BufferKey {
                        entity: source,
                        ..texture_key(0, generation)
                    })
                    .unwrap();
                assert!(write.publish());
            }
            assert!(outputs.get(source).is_some());
            assert!(outputs.previous(source).is_some());
        }

        world.insert(parent, crate::ui::Hidden);
        reconcile_texture_outputs(&world);
        {
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert!(outputs.get(source).is_none());
            assert!(outputs.previous(source).is_none());
            assert!(outputs.state(source).unwrap().retained);
        }

        world.remove::<crate::ui::Hidden>(parent);
        reconcile_texture_outputs(&world);
        let expected = Color::rgb(29, 53, 101);
        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        {
            let _batch = outputs.begin_batch();
            assert!(outputs.previous(source).is_none());
            let mut write = outputs
                .prepare_texture(BufferKey {
                    entity: source,
                    ..texture_key(0, 7)
                })
                .unwrap();
            write.texture_mut().set_pixel(0, 0, &expected);
            assert!(write.publish());
            assert!(outputs.previous(source).is_none());
        }
        {
            let _batch = outputs.begin_batch();
            assert_eq!(
                outputs.previous(source).unwrap().texture().get_pixel(0, 0),
                expected
            );
        }
    }

    #[test]
    fn offscreen_render_default_is_full_scale() {
        let off = OffscreenRender::default();
        assert_eq!(off.scale, Fixed::ONE);
    }

    #[test]
    fn offscreen_render_with_scale() {
        let off = OffscreenRender::with_scale(Fixed::ONE / 2);
        assert_eq!(off.scale, Fixed::ONE / 2);
    }

    fn dependency_world() -> World {
        let mut world = World::default();
        let mut views = crate::ui::ViewRegistry::default();
        views.insert(crate::ui::widgets::mirror::view());
        world.insert_resource(views);
        world
    }

    fn reconcile(world: &mut World) {
        crate::ui::ViewRegistry::reconcile_observations(world);
    }

    #[test]
    fn dependencies_are_inactive_without_their_view_registration() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let owner = world.spawn(crate::ui::widgets::MirrorOf::new(source));
        assert!(!texture_requested(&world, source));
        let mut views = crate::ui::ViewRegistry::default();
        views.insert(crate::ui::widgets::mirror::view());
        world.insert_resource(views);
        reconcile(&mut world);
        assert!(texture_requested(&world, source));
        assert!(world.get::<OffscreenRender>(source).is_none());
        world.remove::<crate::ui::widgets::MirrorOf>(owner);
        reconcile(&mut world);
        assert!(!texture_requested(&world, source));
    }

    #[test]
    fn shared_requests_do_not_modify_explicit_output_configuration() {
        let mut world = dependency_world();
        let source = world.spawn(OffscreenRender::with_scale(Fixed::HALF).with_opacity(123));
        let first = world.spawn(crate::ui::widgets::MirrorOf::new(source));
        let second = world.spawn(crate::ui::widgets::MirrorOf::new(source));
        reconcile(&mut world);
        assert!(texture_requested(&world, source));
        world.remove::<crate::ui::widgets::MirrorOf>(first);
        reconcile(&mut world);
        assert!(texture_requested(&world, source));
        world.remove::<crate::ui::widgets::MirrorOf>(second);
        reconcile(&mut world);
        assert!(!texture_requested(&world, source));
        let explicit = world.get::<OffscreenRender>(source).unwrap();
        assert_eq!(explicit.scale, Fixed::HALF);
        assert_eq!(explicit.opacity, 123);
        assert!(config(&world, source).is_some());
    }

    #[test]
    fn mutable_source_rebinds_without_replacing_the_consumers_transform() {
        use crate::types::Transform;
        use crate::ui::widgets::{MirrorOf, WidgetTransform};

        let mut world = dependency_world();
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        let transform = Transform::translate(11.into(), 13.into());
        let owner = world.spawn((MirrorOf::new(first), WidgetTransform(transform)));
        reconcile(&mut world);
        assert!(texture_requested(&world, first));
        world.get_mut::<MirrorOf>(owner).unwrap().source = second;
        reconcile(&mut world);
        assert!(!texture_requested(&world, first));
        assert!(texture_requested(&world, second));
        assert_eq!(world.get::<WidgetTransform>(owner).unwrap().0, transform);
        assert!(world.has::<crate::ui::dirty::VisualDirty>(owner));
    }

    #[test]
    fn recycled_entity_ids_do_not_inherit_output_demand() {
        let mut world = dependency_world();
        let source = world.spawn_empty();
        let owner = world.spawn(crate::ui::widgets::MirrorOf::new(source));
        reconcile(&mut world);
        assert!(texture_requested(&world, source));
        world.despawn(source);
        let replacement = world.spawn_empty();
        assert_eq!(source.id, replacement.id);
        assert_ne!(source, replacement);
        reconcile(&mut world);
        assert!(!texture_requested(&world, replacement));
        assert!(!texture_requested(&world, source));
        world
            .get_mut::<crate::ui::widgets::MirrorOf>(owner)
            .unwrap()
            .source = replacement;
        reconcile(&mut world);
        assert!(texture_requested(&world, replacement));
    }
}
