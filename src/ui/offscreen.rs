//! Retained widget output pixels. [`OffscreenRender`] renders an entity's subtree into a buffer sized at `ComputedRect × scale`, then composites the completed buffer at the entity's full rectangle. A scale below one reduces the buffer resolution; group opacity applies during composition.
//!
//! [`Outputs<WidgetTexture>`](crate::ui::Outputs) owns the pixel storage and published records for complete entity identities. The internal input key includes size, format, and dirty generation. An unchanged key reuses a completed result; rebuilding a same-size output reuses its allocated pixel slot.
//!
//! A texture becomes readable only after its subtree renders successfully. Preparing a referenced source does not composite it into the parent target; the main tree traversal determines composition order. An unfinished or failed write is unavailable through `get`, and failure of another output does not discard an already completed result.
//!
//! Typed callbacks read the resource with `textures: res!(Outputs<WidgetTexture>)`. `textures.get(entity)` returns a borrowed [`WidgetTextureRead`], whose `texture()` method exposes the completed CPU texture. Reads do not produce pixels or register a source dependency. [`App::snapshot_widget`](crate::app::App::snapshot_widget) provides an owned pixel copy when a result must outlive that borrow.
//!
//! Outputs use one pixel slot by default. An owner that explicitly requests history, such as [`TemporalMix`](crate::ui::widgets::TemporalMix), uses two slots and reads its own frozen result through `previous(entity)`. History is the last successful output at the start of the drawing batch, remains fixed across dirty regions, and is independent of input-generation gaps. A first output or an owner without history has no previous result; incompatible dimensions or format clear old history.
//!
//! The pixel budget includes retained slots and detached in-progress write buffers. A request that exceeds it is rejected before allocating pixels, and other outputs remain resident. Metadata and effect scratch storage are additional costs. The default budget is zero; [`App::with_offscreen_pool_budget`](crate::app::App::with_offscreen_pool_budget) configures it before widget construction.
//!
//! # Usage
//!
//! Configure the budget before composing nodes that request output storage:
//!
//! ```ignore
//! use mirui::prelude::*;
//! use mirui::ui::OffscreenRender;
//!
//! #[compose]
//! fn cached_panel() {
//!     ui! { View (width: 128, height: 64) [OffscreenRender::new()] {} };
//! }
//!
//! let mut app = App::headless(128, 64);
//! app.with_default_widgets();
//! let root = app.spawn_root().id();
//! // Include every concurrently retained pixel slot in the budget.
//! // RGB565 uses two bytes per pixel; RGBA8888 uses four.
//! app.with_offscreen_pool_budget(64 * 1024);
//!
//! app.compose(root, cached_panel());
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
//! A source's [`OffscreenAlphaMode`] selects transparent clearing or initialization from the parent target. Nested `OffscreenRender` and a source carrying `WidgetTransform3D` are unsupported.

use crate::ecs::{Entity, World};
use crate::render::texture::ColorFormat;
use crate::types::Fixed;

/// Mark an entity for offscreen rendering. Insert / remove to toggle.
///
/// Default ([`Self::new`]) renders at full resolution and only buys
/// caching. `with_scale(Fixed::HALF)` renders at half resolution and
/// upscales on present. `with_opacity(128)` blends the cached subtree
/// onto the outer surface at the chosen alpha.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct OffscreenRender {
    /// Render scale relative to the entity's `ComputedRect`. 1.0 keeps
    /// the buffer at the entity's drawn size; 0.5 halves both axes (a
    /// quarter of the pixel count). Values below `Fixed::ONE / 8` are
    /// clamped at render time so `buf_w` / `buf_h` never round to 0.
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

/// Mark on a consumer entity to keep `source`'s OffscreenRender
/// alive. mirui maintains a refcount per source — first ref adds
/// OffscreenRender to source; the last ref removed restores source
/// to its prior state (no OffscreenRender if the user hadn't opted
/// in).
///
/// Effect widgets typically attach this via the view registry's
/// `auto_attach` mechanism, so user code only inserts the effect's
/// main component.
///
/// Maintenance invalidates a consumer on its first reference, a change of source entity or pixel generation, or source dirtiness. Source identity includes the entity generation. A reference absent during maintenance loses its tracking state; reattaching it afterward starts a new observation. Removal and reinsertion before maintenance are coalesced into the final reference.
#[derive(Clone, Copy, Debug)]
pub struct WidgetTextureRef(pub Entity);

impl crate::ecs::Component for WidgetTextureRef {}

/// Internal marker on a source entity that received `OffscreenRender`
/// from `maintain_widget_texture_refs` (not from user code). Only
/// auto-added entries get removed when the last `WidgetTextureRef`
/// goes away; user-explicit `OffscreenRender` is left alone.
#[derive(Clone, Copy, Debug)]
pub struct OffscreenAutoAdded;

impl crate::ecs::Component for OffscreenAutoAdded {}

/// Mark on an `OffscreenRender` source so its buffer initialises to
/// fully-transparent black instead of a copy of the framebuffer
/// underneath. Effect widgets that need the buffer's alpha channel
/// to encode the source's actual silhouette (zero outside the
/// widget's drawn pixels) attach this — without it, alpha
/// extraction sees the framebuffer's alpha bleeding through and
/// produces a rectangular silhouette that ignores `border_radius`.
#[derive(Clone, Copy, Debug)]
pub struct OffscreenAlphaMode {
    /// `true` ⇒ buffer is cleared to RGBA `(0, 0, 0, 0)` before the
    /// subtree renders. `false` ⇒ buffer is pre-seeded from the
    /// framebuffer (default — keeps anti-aliased edges blending against
    /// the existing background).
    pub clear_transparent: bool,
}

impl OffscreenAlphaMode {
    pub const fn clear_transparent() -> Self {
        Self {
            clear_transparent: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WidgetTextureRefState {
    source: Entity,
    generation: u32,
}

impl crate::ecs::Component for WidgetTextureRefState {}

/// Input identity for one widget texture. Entity identity, pixel dimensions, format, and content generation determine whether the published result can be reused; historical output selection is independent of this key.
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) struct BufferKey {
    pub entity: Entity,
    pub w: u16,
    pub h: u16,
    pub format: ColorFormat,
    pub generation: u32,
}

impl BufferKey {
    pub(crate) fn for_widget(
        entity: Entity,
        rect: crate::types::Rect,
        offscreen: OffscreenRender,
        generation: u32,
        backend_format: ColorFormat,
        clear_transparent: bool,
    ) -> Self {
        let scale = offscreen.scale.max(Fixed::ONE / 8);
        let width = Fixed::from_int(rect.w.to_int().max(1)) * scale;
        let height = Fixed::from_int(rect.h.to_int().max(1)) * scale;
        Self {
            entity,
            w: width.to_int().max(1).min(u16::MAX as i32) as u16,
            h: height.to_int().max(1).min(u16::MAX as i32) as u16,
            format: if clear_transparent {
                ColorFormat::RGBA8888
            } else {
                backend_format
            },
            generation,
        }
    }
}

mod output;
pub use output::{WidgetTexture, WidgetTextureRead, WidgetTextureStorage};

pub(crate) fn reconcile_texture_outputs(world: &World) {
    if let Some(outputs) = world.resource::<crate::ui::Outputs<WidgetTexture>>() {
        outputs.reconcile_textures(
            |entity| world.get::<OffscreenRender>(entity).is_some(),
            |entity| !super::branch::is_hidden_in_tree(world, entity),
        );
    }
}

/// Walk every `WidgetTextureRef` and reconcile each referenced
/// source's `OffscreenRender` state. Auto-add when a source gains
/// its first ref; auto-remove when the last ref drops.
pub fn maintain_widget_texture_refs(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    use alloc::vec::Vec;
    use hashbrown::HashMap;

    let mut state_index = 0;
    while let Some(consumer) = world
        .storage::<WidgetTextureRefState>()
        .and_then(|storage| storage.entities().get(state_index))
        .copied()
    {
        if world.has::<WidgetTextureRef>(consumer) {
            state_index += 1;
        } else {
            // Sparse removal moves the last entry into the current index.
            world.remove::<WidgetTextureRefState>(consumer);
        }
    }

    // Cheap path: if neither component is in use, the system has
    // nothing to do — common until any effect widget gets attached.
    let any_ref = world.query::<WidgetTextureRef>().iter().next().is_some();
    let any_auto = world.query::<OffscreenAutoAdded>().iter().next().is_some();
    if !any_ref && !any_auto {
        reconcile_texture_outputs(world);
        return Ok(());
    }

    let mut counts: HashMap<Entity, u32> = HashMap::new();
    for (_e, r) in world.query::<WidgetTextureRef>().iter() {
        *counts.entry(r.0).or_insert(0) += 1;
    }

    let mut to_add: Vec<Entity> = Vec::new();
    for (&source, &n) in &counts {
        if n > 0 && world.get::<OffscreenRender>(source).is_none() {
            to_add.push(source);
        }
    }
    for source in to_add {
        world.insert(source, OffscreenRender::default());
        world.insert(source, OffscreenAutoAdded);
    }

    let auto_entries: Vec<Entity> = world
        .query::<OffscreenAutoAdded>()
        .iter()
        .map(|(e, _)| e)
        .collect();
    for source in auto_entries {
        // Keep self-attached entries: an effect widget that holds a
        // `WidgetTextureRef` may also need its own `OffscreenRender`
        // (e.g. TemporalMix uses its own buffer for IIR feedback).
        // Such an entity isn't anyone's source, so the refcount check
        // alone would tear its buffer down.
        if counts.get(&source).copied().unwrap_or(0) == 0
            && world.get::<WidgetTextureRef>(source).is_none()
        {
            world.remove::<OffscreenRender>(source);
            world.remove::<OffscreenAutoAdded>(source);
        }
    }

    let pairs: Vec<(Entity, Entity)> = world
        .query::<WidgetTextureRef>()
        .iter()
        .map(|(e, r)| (e, r.0))
        .collect();
    for (consumer, source) in pairs {
        let g_now = world
            .get::<OffscreenGeneration>(source)
            .map(|g| g.0)
            .unwrap_or(0);
        let state = WidgetTextureRefState {
            source,
            generation: g_now,
        };
        let changed = world.get::<WidgetTextureRefState>(consumer) != Some(&state);
        let source_dirty = world.get::<super::dirty::Dirty>(source).is_some();
        let source_transform = world
            .get::<crate::ui::widgets::WidgetTransform>(source)
            .copied();
        // Source movement can change composed geometry without changing pixel generation.
        if changed || source_dirty {
            world.insert(consumer, super::dirty::Dirty);
            world.insert(consumer, state);
            // Mirror the source's WidgetTransform onto the consumer
            // so the dirty walker computes the consumer's screen
            // bbox at the source's actual painted position, not at
            // the consumer's static layout slot. Without this the
            // dirty rect doesn't track the source's animation and
            // old shadow / mirror pixels stick around as the source
            // moves.
            if let Some(tf) = source_transform {
                world.insert(consumer, tf);
            } else {
                world.remove::<crate::ui::widgets::WidgetTransform>(consumer);
            }
        }
    }
    reconcile_texture_outputs(world);
    Ok(())
}

crate::ecs::system::framework_system!(
    pub maintain_widget_texture_refs,
    PreRender,
    expect = [WidgetTextureRef, OffscreenAutoAdded, WidgetTextureRefState],
);

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
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let k2 = BufferKey {
            entity: dummy_entity(2),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        assert_ne!(k1, k2);
    }

    #[test]
    fn buffer_key_distinguishes_generations() {
        let k1 = BufferKey {
            entity: dummy_entity(1),
            w: 40,
            h: 24,
            format: ColorFormat::RGBA8888,
            generation: 0,
        };
        let k2 = BufferKey {
            generation: 1,
            ..k1
        };
        assert_ne!(k1, k2);
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

    fn run_refs(world: &mut World) {
        super::maintain_widget_texture_refs(world).unwrap();
    }

    #[test]
    fn first_ref_auto_adds_offscreen_render() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let consumer = world.spawn_empty();
        world.insert(consumer, WidgetTextureRef(source));

        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_some());
        assert!(world.get::<OffscreenAutoAdded>(source).is_some());
    }

    #[test]
    fn last_ref_dropped_auto_removes_offscreen_render() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let c1 = world.spawn_empty();
        let c2 = world.spawn_empty();
        world.insert(c1, WidgetTextureRef(source));
        world.insert(c2, WidgetTextureRef(source));
        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_some());

        world.remove::<WidgetTextureRef>(c1);
        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_some());

        world.remove::<WidgetTextureRef>(c2);
        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_none());
        assert!(world.get::<OffscreenAutoAdded>(source).is_none());
    }

    #[test]
    fn user_explicit_offscreen_render_is_never_removed() {
        let mut world = World::default();
        let source = world.spawn_empty();
        // User-explicit: no `OffscreenAutoAdded` marker.
        world.insert(source, OffscreenRender::default());

        let consumer = world.spawn_empty();
        world.insert(consumer, WidgetTextureRef(source));
        run_refs(&mut world);
        assert!(world.get::<OffscreenAutoAdded>(source).is_none());

        world.remove::<WidgetTextureRef>(consumer);
        run_refs(&mut world);
        assert!(world.get::<OffscreenRender>(source).is_some());
    }

    #[test]
    fn texture_reference_identity_drives_invalidation_and_transform() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::WidgetTransform;

        for generation in [0, 7] {
            let mut world = World::default();
            let first = world.spawn_empty();
            let second = world.spawn_empty();
            let consumer = world.spawn_empty();
            let first_transform = Transform::translate(3.into(), 5.into());
            let second_transform = Transform::translate(11.into(), 13.into());
            world.insert(first, OffscreenGeneration(generation));
            world.insert(second, OffscreenGeneration(generation));
            world.insert(first, WidgetTransform(first_transform));
            world.insert(second, WidgetTransform(second_transform));
            world.insert(consumer, WidgetTextureRef(first));

            run_refs(&mut world);
            assert!(world.has::<Dirty>(consumer));
            assert_eq!(
                world.get::<WidgetTransform>(consumer).unwrap().0,
                first_transform
            );
            world.remove::<Dirty>(consumer);
            run_refs(&mut world);
            assert!(!world.has::<Dirty>(consumer));

            world.insert(consumer, WidgetTextureRef(second));
            run_refs(&mut world);
            assert!(world.has::<Dirty>(consumer));
            assert_eq!(
                world.get::<WidgetTransform>(consumer).unwrap().0,
                second_transform
            );
            assert!(!world.has::<OffscreenRender>(first));
            assert!(world.has::<OffscreenRender>(second));
            world.remove::<Dirty>(consumer);
            run_refs(&mut world);
            assert!(!world.has::<Dirty>(consumer));

            world.insert(consumer, WidgetTextureRef(first));
            world.remove::<WidgetTransform>(first);
            run_refs(&mut world);
            assert!(world.has::<Dirty>(consumer));
            assert!(!world.has::<WidgetTransform>(consumer));
        }
    }

    #[test]
    fn retired_reference_state_is_reclaimed_without_allocating() {
        use crate::test_support::tracked_allocations;
        use crate::ui::dirty::Dirty;

        let mut world = World::default();
        let source = world.spawn_empty();
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        world.insert(source, OffscreenRender::default());
        world.insert(first, WidgetTextureRef(source));
        world.insert(second, WidgetTextureRef(source));
        run_refs(&mut world);
        world.remove::<Dirty>(first);
        world.remove::<Dirty>(second);
        world.remove::<WidgetTextureRef>(first);
        world.remove::<WidgetTextureRef>(second);

        let allocations = tracked_allocations(|| run_refs(&mut world));
        assert_eq!(allocations, 0);
        assert!(!world.has::<WidgetTextureRefState>(first));
        assert!(!world.has::<WidgetTextureRefState>(second));
        assert!(world.has::<OffscreenRender>(source));

        world.insert(first, WidgetTextureRef(source));
        run_refs(&mut world);
        assert!(world.has::<Dirty>(first));
    }

    #[test]
    fn recycled_source_id_has_a_distinct_texture_reference_identity() {
        use crate::ui::dirty::Dirty;

        let mut world = World::default();
        let source = world.spawn_empty();
        let consumer = world.spawn_empty();
        world.insert(consumer, WidgetTextureRef(source));
        run_refs(&mut world);
        world.remove::<Dirty>(consumer);
        assert!(world.despawn(source));
        let replacement = world.spawn_empty();
        assert_eq!(source.id, replacement.id);
        assert_ne!(source.generation, replacement.generation);

        world.insert(consumer, WidgetTextureRef(replacement));
        run_refs(&mut world);
        assert!(world.has::<Dirty>(consumer));
        assert_eq!(
            world.get::<WidgetTextureRefState>(consumer).unwrap().source,
            replacement
        );
    }

    #[test]
    fn unchanged_texture_revision_propagates_dirty_source_transform() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::WidgetTransform;

        let mut world = World::default();
        let source = world.spawn_empty();
        let consumer = world.spawn_empty();
        world.insert(consumer, WidgetTextureRef(source));
        run_refs(&mut world);
        world.remove::<Dirty>(consumer);
        let transform = Transform::translate(17.into(), 19.into());
        world.insert(source, WidgetTransform(transform));
        world.insert(source, Dirty);

        run_refs(&mut world);
        assert!(world.has::<Dirty>(consumer));
        assert_eq!(world.get::<WidgetTransform>(consumer).unwrap().0, transform);
    }
}
