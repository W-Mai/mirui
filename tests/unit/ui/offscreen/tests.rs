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
    let mut write = outputs.prepare_test_texture(key).expect("texture storage");
    write
        .texture_mut()
        .pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &Color::rgb(17, 31, 47));
    assert!(outputs.get(key.entity).is_none());
    assert!(outputs.previous(key.entity).is_none());

    assert!(write.publish());
    assert_eq!(
        outputs
            .get(key.entity)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(0, 0),
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
    assert!(outputs.prepare_test_texture(key).is_none());
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
    let write = outputs.prepare_test_texture(key).expect("texture storage");
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
        .prepare_test_texture(texture_key(1, 0))
        .expect("initial texture storage");
    assert!(write.publish());

    let allocations = crate::test_support::tracked_allocations(|| {
        for generation in 1..8 {
            let _batch = outputs.begin_batch();
            let write = outputs
                .prepare_test_texture(texture_key(1, generation))
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
    assert!(outputs.prepare_test_texture(key).unwrap().publish());

    let mut write = outputs
        .prepare_test_texture(texture_key(1, 7))
        .expect("replacement texture storage");
    write
        .texture_mut()
        .pixels_mut()
        .unwrap()
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
            .prepare_test_texture(texture_key(1, 0))
            .unwrap()
            .publish()
    );
    let pending = outputs.prepare_test_texture(texture_key(2, 0)).unwrap();
    assert!(outputs.get(dummy_entity(1)).is_some());
    assert!(outputs.get(dummy_entity(2)).is_none());

    assert!(outputs.prepare_test_texture(texture_key(3, 0)).is_none());
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
        assert!(outputs.prepare_test_texture(key).is_none());
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
        let mut write = outputs.prepare_test_texture(texture_key(1, 2)).unwrap();
        write
            .texture_mut()
            .pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &first);
        assert!(write.publish());
        assert!(outputs.previous(entity).is_none());
    }
    {
        let _batch = outputs.begin_batch();
        let mut write = outputs.prepare_test_texture(texture_key(1, 11)).unwrap();
        assert_eq!(
            outputs
                .previous(entity)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            first
        );
        write
            .texture_mut()
            .pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &second);
        assert!(write.publish());
        assert_eq!(
            outputs
                .get(entity)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            second
        );
        assert_eq!(
            outputs
                .previous(entity)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            first
        );
        assert!(outputs.prepare_test_texture(texture_key(2, 0)).is_none());
    }
    {
        let _batch = outputs.begin_batch();
        let write = outputs.prepare_test_texture(texture_key(1, 19)).unwrap();
        drop(write);
        assert!(outputs.get(entity).is_none());
        assert_eq!(
            outputs
                .previous(entity)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            second
        );
    }
}

#[test]
fn trimming_outputs_releases_published_textures_and_live_budget() {
    let outputs = Outputs::<WidgetTexture>::with_budget(40 * 24 * 4);
    assert!(
        outputs
            .prepare_test_texture(texture_key(1, 0))
            .unwrap()
            .publish()
    );
    assert!(outputs.prepare_test_texture(texture_key(2, 0)).is_none());

    outputs.trim_memory();
    assert!(outputs.get(dummy_entity(1)).is_none());
    assert!(
        outputs
            .prepare_test_texture(texture_key(2, 0))
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
                .prepare_test_texture(BufferKey {
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
            .prepare_test_texture(BufferKey {
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
                .prepare_test_texture(BufferKey {
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
            .prepare_test_texture(BufferKey {
                entity: source,
                ..texture_key(0, 7)
            })
            .unwrap();
        write
            .texture_mut()
            .pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &expected);
        assert!(write.publish());
        assert!(outputs.previous(source).is_none());
    }
    {
        let _batch = outputs.begin_batch();
        assert_eq!(
            outputs
                .previous(source)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
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
