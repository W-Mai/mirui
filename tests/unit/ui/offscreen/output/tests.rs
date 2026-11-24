use super::*;
use crate::render::PlaneLayout;
use crate::test_support::tracked_allocations;
use crate::types::{Color, Fixed};

const TEXTURE_BYTES: usize = 4 * 4 * 4;

#[test]
fn starting_a_write_never_reserves_or_reselects_storage() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    let input = key(1, 1);
    assert_eq!(
        tracked_allocations(|| {
            assert!(outputs.prepare_texture(input).is_none());
        }),
        0
    );
    assert!(outputs.state(input.entity).is_none());
    assert_eq!(outputs.storage().budget.allocated(), 0);
    assert!(outputs.reserve_texture(input, None).unwrap());
    assert_eq!(
        tracked_allocations(|| {
            assert!(outputs.prepare_texture(key(1, 2)).is_none());
            assert!(
                outputs
                    .prepare_texture(BufferKey { w: 8, ..input })
                    .is_none()
            );
            let mut write = outputs.prepare_texture(input).unwrap();
            write
                .texture_mut()
                .pixels_mut()
                .unwrap()
                .bytes_mut()
                .fill(0x55);
            assert!(outputs.prepare_texture(input).is_none());
            assert!(write.publish());
        }),
        0
    );
    assert_eq!(outputs.storage().budget.allocated(), TEXTURE_BYTES);
    assert!(outputs.texture_matches(input).unwrap());
}

#[test]
fn budget_changes_do_not_require_exclusive_output_storage_access() {
    let outputs = Outputs::<WidgetTexture>::with_budget(3 * TEXTURE_BYTES);
    let source = key(1, 1);
    assert!(outputs.prepare_test_texture(source).unwrap().publish());
    let mut plan = outputs.take_texture_plan().unwrap();
    let read = outputs.get(source.entity).unwrap();
    let budget = outputs.storage().budget.clone();
    let pending = budget.reserve(TEXTURE_BYTES).unwrap();
    assert!(outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
    assert_eq!(budget.available(), 0);
    let allocation = pending.commit(TEXTURE_BYTES).unwrap();
    assert!(!outputs.reserve_workspace(&mut plan.workspace, 2 * TEXTURE_BYTES));
    assert_eq!(budget.allocated(), 3 * TEXTURE_BYTES);
    drop(allocation);
    assert!(outputs.reserve_workspace(&mut plan.workspace, 2 * TEXTURE_BYTES));
    assert_eq!(
        read.texture().read_pixels().unwrap().bytes().len(),
        TEXTURE_BYTES
    );
    drop(read);
    outputs.put_texture_plan(plan);
    outputs.trim_memory();
    assert_eq!(budget.allocated(), 0);
    assert_eq!(budget.available(), 3 * TEXTURE_BYTES);
}

#[test]
fn detached_workspaces_keep_their_charge_until_storage_is_dropped() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    assert!(outputs.prepare_test_texture(key(1, 1)).unwrap().publish());
    let budget = outputs.storage().budget.clone();
    let mut plan = outputs.take_texture_plan().unwrap();
    assert!(outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
    drop(outputs);
    assert_eq!(budget.allocated(), TEXTURE_BYTES);
    drop(plan);
    assert_eq!(budget.allocated(), 0);
    assert_eq!(budget.available(), 2 * TEXTURE_BYTES);
}

#[test]
fn rejected_write_returns_storage_without_copying_or_leaking_the_slot() {
    static BYTES: [u8; TEXTURE_BYTES] = [17; TEXTURE_BYTES];
    let outputs = Outputs::<WidgetTexture>::with_budget(TEXTURE_BYTES);
    let source = key(1, 1);
    assert!(outputs.prepare_test_texture(source).unwrap().publish());
    let writable = {
        let mut storage = outputs.storage_mut();
        let slot = &mut storage.textures.get_mut(&source.entity).unwrap().planes[0];
        core::mem::replace(
            &mut slot.as_mut().unwrap().texture,
            Texture::from_static(&BYTES, 4, 4, ColorFormat::RGBA8888).unwrap(),
        )
    };
    let allocations = tracked_allocations(|| {
        assert!(outputs.prepare_test_texture(source).is_none());
    });
    assert_eq!(allocations, 0);
    assert!(outputs.get(source.entity).is_none());
    {
        let mut storage = outputs.storage_mut();
        assert_eq!(storage.budget.allocated(), TEXTURE_BYTES);
        let slots = storage.textures.get_mut(&source.entity).unwrap();
        assert!(!slots.writing);
        let pixels = slots.planes[0]
            .as_ref()
            .unwrap()
            .texture
            .read_pixels()
            .unwrap();
        assert_eq!(pixels.bytes().as_ptr(), BYTES.as_ptr());
        assert_eq!(pixels.bytes(), &BYTES);
        drop(pixels);
        slots.planes[0].as_mut().unwrap().texture = writable;
    }
    assert!(outputs.prepare_test_texture(source).unwrap().publish());
}

#[test]
fn failed_history_read_does_not_publish_the_working_slot() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    let source = key(1, 1);
    outputs.reserve(1).unwrap();
    outputs.set_retained(source.entity, true).unwrap();
    {
        let _batch = outputs.begin_batch();
        assert!(outputs.prepare_test_texture(source).unwrap().publish());
    }
    let _batch = outputs.begin_batch();
    let mut write = outputs.prepare_test_texture(source).unwrap();
    write.texture_mut().stride = usize::MAX;
    assert_eq!(
        write.differs_from_previous(),
        Err(RenderError::InvalidTexture)
    );
    drop(write);
    assert!(outputs.get(source.entity).is_none());
    assert!(outputs.previous(source.entity).is_some());
    assert!(!outputs.storage().textures[&source.entity].writing);
    assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);
}

pub(super) fn key(id: u32, generation: u32) -> BufferKey {
    BufferKey {
        entity: Entity { id, generation: 0 },
        layout_rect: Rect {
            w: Fixed::from_int(4),
            h: Fixed::from_int(4),
            ..Rect::ZERO
        },
        local_bounds: Rect::new(0, 0, 4, 4),
        scale: Fixed::ONE,
        w: 4,
        h: 4,
        format: ColorFormat::RGBA8888,
        generation,
    }
}

#[test]
fn geometry_belongs_to_successful_pixels_and_position_changes_preserve_history() {
    let texture_bytes = 5 * 5 * 4;
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * texture_bytes);
    let entity = key(1, 1).entity;
    let original = BufferKey::for_widget(
        entity,
        Rect {
            x: Fixed::from_int(11) + Fixed::HALF,
            y: Fixed::from_int(-7),
            w: Fixed::from_int(8) + Fixed::HALF,
            h: Fixed::from_int(8) + Fixed::ONE / 4,
        },
        Rect {
            w: Fixed::from_int(8) + Fixed::HALF,
            h: Fixed::from_int(8) + Fixed::ONE / 4,
            ..Rect::ZERO
        },
        super::super::OffscreenRender::with_scale(Fixed::HALF),
        1,
    );
    let moved = BufferKey {
        layout_rect: Rect {
            x: original.layout_rect.x + Fixed::from_int(19),
            y: original.layout_rect.y - Fixed::HALF,
            ..original.layout_rect
        },
        ..original
    };
    let local_bounds = Rect::new(0, 0, 10, 10);
    assert_eq!(original.local_bounds, local_bounds);
    let original_color = Color::rgb(17, 31, 47);
    let moved_color = Color::rgb(53, 71, 97);
    outputs.reserve(1).unwrap();
    outputs.set_retained(entity, true).unwrap();
    {
        let _batch = outputs.begin_batch();
        let mut write = outputs.prepare_test_texture(original).unwrap();
        write
            .texture_mut()
            .pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &original_color);
        assert!(write.publish());
        let read = outputs.get(entity).unwrap();
        assert_eq!(read.layout_rect(), original.layout_rect);
        assert_eq!(read.local_bounds(), local_bounds);
        assert_eq!((read.texture().width, read.texture().height), (5, 5));
    }
    let _batch = outputs.begin_batch();
    assert!(outputs.texture_matches(original).unwrap());
    assert!(!outputs.texture_matches(moved).unwrap());
    assert!(outputs.reserve_texture(moved, None).unwrap());
    assert!(outputs.get(entity).is_none());
    {
        let previous = outputs.previous(entity).unwrap();
        assert_eq!(previous.layout_rect(), original.layout_rect);
        assert_eq!(previous.local_bounds(), local_bounds);
        assert_eq!(
            previous.texture().read_pixels().unwrap().get_pixel(0, 0),
            original_color
        );
    }

    let mut cancelled = outputs.prepare_test_texture(moved).unwrap();
    cancelled
        .texture_mut()
        .pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &moved_color);
    drop(cancelled);
    assert!(outputs.get(entity).is_none());
    assert_eq!(
        outputs.previous(entity).unwrap().layout_rect(),
        original.layout_rect
    );

    let mut write = outputs.prepare_test_texture(moved).unwrap();
    write
        .texture_mut()
        .pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &moved_color);
    assert!(write.publish());
    assert!(outputs.texture_matches(moved).unwrap());
    assert!(!outputs.texture_matches(original).unwrap());
    let read = outputs.get(entity).unwrap();
    assert_eq!(read.layout_rect(), moved.layout_rect);
    assert_eq!(read.local_bounds(), local_bounds);
    assert_eq!(
        read.texture().read_pixels().unwrap().get_pixel(0, 0),
        moved_color
    );
    let previous = outputs.previous(entity).unwrap();
    assert_eq!(previous.layout_rect(), original.layout_rect);
    assert_eq!(previous.local_bounds(), local_bounds);
    assert_eq!(
        previous.texture().read_pixels().unwrap().get_pixel(0, 0),
        original_color
    );
    assert_eq!(outputs.storage().budget.allocated(), 2 * texture_bytes);
}

#[test]
fn logical_size_changes_clear_history_without_reallocating_unchanged_pixel_shape() {
    for change_width in [true, false] {
        let texture_bytes = 5 * 5 * 4;
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * texture_bytes);
        let entity = key(1, 1).entity;
        let original = BufferKey::for_widget(
            entity,
            Rect {
                x: Fixed::from_int(3),
                y: Fixed::from_int(5),
                w: Fixed::from_int(4) + Fixed::ONE / 4,
                h: Fixed::from_int(4) + Fixed::ONE / 4,
            },
            Rect {
                w: Fixed::from_int(4) + Fixed::ONE / 4,
                h: Fixed::from_int(4) + Fixed::ONE / 4,
                ..Rect::ZERO
            },
            super::super::OffscreenRender::default(),
            2,
        );
        outputs.reserve(1).unwrap();
        outputs.set_retained(entity, true).unwrap();
        for generation in [1, 2] {
            let _batch = outputs.begin_batch();
            assert!(
                outputs
                    .prepare_test_texture(BufferKey {
                        generation,
                        ..original
                    })
                    .unwrap()
                    .publish()
            );
        }
        let mut resized_rect = original.layout_rect;
        if change_width {
            resized_rect.w += Fixed::ONE / 4;
        } else {
            resized_rect.h += Fixed::ONE / 4;
        }
        let resized = BufferKey::for_widget(
            entity,
            resized_rect,
            Rect {
                w: resized_rect.w,
                h: resized_rect.h,
                ..Rect::ZERO
            },
            super::super::OffscreenRender::default(),
            original.generation,
        );
        assert_eq!((resized.w, resized.h), (original.w, original.h));
        let _batch = outputs.begin_batch();
        assert!(outputs.previous(entity).is_some());
        assert!(!outputs.texture_matches(resized).unwrap());

        let allocations = tracked_allocations(|| {
            assert!(outputs.reserve_texture(resized, None).unwrap());
            assert!(outputs.get(entity).is_none());
            assert!(outputs.previous(entity).is_none());
            assert!(outputs.state(entity).unwrap().retained);
            drop(outputs.prepare_test_texture(resized).unwrap());
            assert!(outputs.get(entity).is_none());
            assert!(outputs.previous(entity).is_none());
            assert!(outputs.prepare_test_texture(resized).unwrap().publish());
            let read = outputs.get(entity).unwrap();
            assert_eq!(read.layout_rect(), resized_rect);
            assert_eq!(read.local_bounds(), resized.local_bounds);
            assert!(outputs.previous(entity).is_none());
        });
        assert_eq!(allocations, 0);
        assert_eq!(outputs.storage().budget.allocated(), 2 * texture_bytes);
    }
}

#[test]
fn local_mapping_changes_clear_history_and_reuse_existing_pixel_slots() {
    for (bounds, scale) in [
        (Rect::new(-1, 0, 4, 4), Fixed::ONE),
        (Rect::new(0, -1, 4, 4), Fixed::ONE),
        (Rect::new(0, 0, 8, 8), Fixed::HALF),
    ] {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let original = key(1, 2);
        let entity = original.entity;
        outputs.reserve(1).unwrap();
        outputs.set_retained(entity, true).unwrap();
        for generation in [1, 2] {
            let _batch = outputs.begin_batch();
            let mut write = outputs
                .prepare_test_texture(BufferKey {
                    generation,
                    ..original
                })
                .unwrap();
            write
                .texture_mut()
                .pixels_mut()
                .unwrap()
                .set_pixel(0, 0, &Color::rgb(7, 11, 17));
            assert!(write.publish());
        }
        let changed = BufferKey::for_widget(
            entity,
            original.layout_rect,
            bounds,
            super::super::OffscreenRender::with_scale(scale),
            original.generation,
        );
        assert_eq!((changed.w, changed.h), (original.w, original.h));
        assert_eq!(changed.layout_rect, original.layout_rect);
        assert_ne!(changed.local_bounds, original.local_bounds);
        let _batch = outputs.begin_batch();
        assert!(outputs.previous(entity).is_some());

        let allocations = tracked_allocations(|| {
            assert!(!outputs.texture_matches(changed).unwrap());
            assert!(outputs.reserve_texture(changed, None).unwrap());
            assert!(outputs.get(entity).is_none());
            assert!(outputs.previous(entity).is_none());
            drop(outputs.prepare_test_texture(changed).unwrap());
            assert!(outputs.get(entity).is_none());
            assert!(outputs.previous(entity).is_none());
            assert!(outputs.prepare_test_texture(changed).unwrap().publish());
            let read = outputs.get(entity).unwrap();
            assert_eq!(read.local_bounds(), changed.local_bounds);
            assert_eq!(read.layout_rect(), original.layout_rect);
            assert!(outputs.previous(entity).is_none());
        });
        assert_eq!(allocations, 0);
        assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);
    }
}

#[test]
fn scale_changes_clear_history_even_when_the_quantized_footprint_is_unchanged() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    let entity = key(1, 1).entity;
    let bounds = Rect::new(
        Fixed::ONE / 256,
        Fixed::ONE / 256,
        Fixed::from_ratio(3, 256),
        Fixed::from_ratio(3, 256),
    );
    let original = BufferKey::for_widget(
        entity,
        Rect::new(0, 0, 4, 4),
        bounds,
        super::super::OffscreenRender::with_scale(Fixed::from_int(256) + Fixed::HALF),
        2,
    );
    let changed = BufferKey::for_widget(
        entity,
        original.layout_rect,
        bounds,
        super::super::OffscreenRender::with_scale(Fixed::from_int(256) + Fixed::from_ratio(3, 4)),
        original.generation,
    );
    assert_eq!((original.w, original.h), (4, 4));
    assert_eq!((changed.w, changed.h), (original.w, original.h));
    assert_eq!(changed.local_bounds, original.local_bounds);
    assert_ne!(changed.scale, original.scale);
    outputs.reserve(1).unwrap();
    outputs.set_retained(entity, true).unwrap();
    for generation in [1, 2] {
        let _batch = outputs.begin_batch();
        assert!(
            outputs
                .prepare_test_texture(BufferKey {
                    generation,
                    ..original
                })
                .unwrap()
                .publish()
        );
    }
    let _batch = outputs.begin_batch();
    assert!(outputs.previous(entity).is_some());
    let allocations = tracked_allocations(|| {
        assert!(!outputs.texture_matches(changed).unwrap());
        assert!(outputs.reserve_texture(changed, None).unwrap());
        assert!(outputs.get(entity).is_none());
        assert!(outputs.previous(entity).is_none());
        assert!(outputs.prepare_test_texture(changed).unwrap().publish());
        assert!(outputs.previous(entity).is_none());
    });
    assert_eq!(allocations, 0);
    assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);
}

#[test]
fn workspace_growth_shares_pixel_budget_and_preserves_other_outputs_on_rejection() {
    let outputs = Outputs::<WidgetTexture>::with_budget(3 * TEXTURE_BYTES);
    let source = key(1, 1);
    assert!(outputs.prepare_test_texture(source).unwrap().publish());
    let mut plan = outputs.take_texture_plan().unwrap();
    assert!(outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
    assert_eq!(
        outputs.storage().budget.allocated(),
        TEXTURE_BYTES + plan.workspace.capacity()
    );

    assert!(outputs.reserve_workspace(&mut plan.workspace, 2 * TEXTURE_BYTES));
    assert_eq!(plan.workspace.capacity(), 2 * TEXTURE_BYTES);
    assert_eq!(outputs.storage().budget.allocated(), 3 * TEXTURE_BYTES);
    assert!(!outputs.reserve_workspace(&mut plan.workspace, 3 * TEXTURE_BYTES));
    assert_eq!(plan.workspace.capacity(), 2 * TEXTURE_BYTES);
    assert_eq!(outputs.storage().budget.allocated(), 3 * TEXTURE_BYTES);
    assert!(outputs.get(source.entity).is_some());
    assert!(!outputs.reserve_texture(key(2, 1), None).unwrap());

    assert!(outputs.reserve_workspace(&mut plan.workspace, 0));
    assert_eq!(plan.workspace.capacity(), 0);
    assert_eq!(outputs.storage().budget.allocated(), TEXTURE_BYTES);
    assert!(
        outputs
            .prepare_test_texture(BufferKey { w: 8, ..key(2, 1) })
            .unwrap()
            .publish()
    );
    outputs.put_texture_plan(plan);
    assert_eq!(outputs.storage().budget.allocated(), 3 * TEXTURE_BYTES);
}

#[test]
fn warm_workspace_leases_can_read_outputs_and_paint_without_allocating() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    let source = key(1, 1);
    let expected = Color::rgba(17, 31, 47, 63);
    let mut write = outputs.prepare_test_texture(source).unwrap();
    write
        .texture_mut()
        .pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &expected);
    assert!(write.publish());
    let layout = PlaneLayout::packed(source.w, source.h, source.format).unwrap();
    let mut plan = outputs.take_texture_plan().unwrap();
    assert!(outputs.reserve_workspace(&mut plan.workspace, layout.required_bytes()));
    outputs.put_texture_plan(plan);

    let allocations = tracked_allocations(|| {
        for _ in 0..16 {
            let mut plan = outputs.take_texture_plan().unwrap();
            assert!(outputs.reserve_workspace(&mut plan.workspace, layout.required_bytes()));
            {
                let source_read = outputs.get(source.entity).unwrap();
                assert!(plan.workspace.with_texture(layout, |texture| {
                    texture.pixels_mut().unwrap().set_pixel(
                        0,
                        0,
                        &source_read.texture().read_pixels().unwrap().get_pixel(0, 0),
                    );
                    assert_eq!(texture.read_pixels().unwrap().get_pixel(0, 0), expected);
                }));
            }
            outputs.put_texture_plan(plan);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);
}

#[test]
fn trimming_an_active_plan_keeps_workspace_and_write_lease_budget_until_return() {
    let outputs = Outputs::<WidgetTexture>::with_budget(3 * TEXTURE_BYTES);
    let input = key(1, 1);
    let pending = outputs.prepare_test_texture(input).unwrap();
    let mut plan = outputs.take_texture_plan().unwrap();
    assert!(outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
    assert!(outputs.reserve_workspace(&mut plan.target_workspace, TEXTURE_BYTES));
    let layout = PlaneLayout::packed(input.w, input.h, input.format).unwrap();
    plan.target_workspace
        .with_bytes(TEXTURE_BYTES, |bytes| {
            bytes.fill(0x5a);
            assert!(plan.workspace.with_texture(layout, |texture| {
                outputs.trim_memory();
                assert_eq!(outputs.storage().budget.allocated(), 3 * TEXTURE_BYTES);
                assert_eq!(plan.workspace.capacity(), TEXTURE_BYTES);
                assert!(!outputs.reserve_texture(key(2, 1), None).unwrap());
                texture
                    .pixels_mut()
                    .unwrap()
                    .set_pixel(0, 0, &Color::rgb(11, 29, 47));
            }));
            assert!(bytes.iter().all(|byte| *byte == 0x5a));
            Ok(())
        })
        .unwrap();
    assert!(!outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
    assert!(!outputs.reserve_workspace(&mut plan.target_workspace, TEXTURE_BYTES));
    drop(pending);
    assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);
    outputs.put_texture_plan(plan);
    assert_eq!(outputs.storage().budget.allocated(), 0);

    let plan = outputs.take_texture_plan().unwrap();
    assert_eq!(plan.workspace.capacity(), 0);
    assert_eq!(plan.target_workspace.capacity(), 0);
    outputs.put_texture_plan(plan);
    assert!(outputs.prepare_test_texture(key(2, 1)).unwrap().publish());
}

#[test]
fn trimming_a_stored_plan_releases_its_workspace_immediately() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    let mut plan = outputs.take_texture_plan().unwrap();
    assert!(outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
    assert!(outputs.reserve_workspace(&mut plan.target_workspace, TEXTURE_BYTES));
    outputs.put_texture_plan(plan);
    assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);
    outputs.trim_memory();
    assert_eq!(outputs.storage().budget.allocated(), 0);
    let plan = outputs.take_texture_plan().unwrap();
    assert_eq!(plan.workspace.capacity(), 0);
    assert_eq!(plan.target_workspace.capacity(), 0);
    outputs.put_texture_plan(plan);
    assert!(outputs.prepare_test_texture(key(1, 1)).unwrap().publish());
}

#[test]
fn trimming_borrowed_output_keeps_its_pixels_and_budget_until_a_later_trim() {
    let outputs = Outputs::<WidgetTexture>::with_budget(TEXTURE_BYTES);
    let input = key(1, 1);
    let mut write = outputs.prepare_test_texture(input).unwrap();
    let expected = Color::rgb(11, 23, 37);
    write
        .texture_mut()
        .pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &expected);
    assert!(write.publish());
    let read = outputs.get(input.entity).unwrap();
    let pixels = read.texture().read_pixels().unwrap();
    let allocations = tracked_allocations(|| {
        for _ in 0..8 {
            outputs.trim_memory();
            assert_eq!(pixels.get_pixel(0, 0), expected);
            assert_eq!(outputs.storage().budget.allocated(), TEXTURE_BYTES);
            assert!(outputs.texture_matches(input).unwrap());
        }
    });
    assert_eq!(allocations, 0);
    drop(pixels);
    drop(read);
    outputs.trim_memory();
    assert!(outputs.get(input.entity).is_none());
    assert_eq!(outputs.storage().budget.allocated(), 0);
}

#[test]
fn reservation_preserves_pixels_until_the_write_is_detached() {
    let outputs = Outputs::<WidgetTexture>::with_budget(TEXTURE_BYTES);
    let first = key(1, 1);
    let expected = Color::rgb(17, 39, 73);
    let mut write = outputs.prepare_test_texture(first).unwrap();
    write
        .texture_mut()
        .pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &expected);
    assert!(write.publish());

    let allocations =
        tracked_allocations(|| assert!(outputs.reserve_texture(first, None).unwrap()));
    assert_eq!(allocations, 0);
    assert!(outputs.texture_matches(first).unwrap());
    assert_eq!(
        outputs
            .get(first.entity)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(0, 0),
        expected
    );

    assert!(outputs.reserve_texture(key(1, 2), None).unwrap());
    assert!(outputs.get(first.entity).is_none());
    {
        let storage = outputs.storage();
        let slots = storage.textures.get(&first.entity).unwrap();
        assert!(!slots.writing);
        assert_eq!(
            slots.planes[0]
                .as_ref()
                .unwrap()
                .texture
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            expected
        );
    }
    let write = outputs.prepare_test_texture(key(1, 2)).unwrap();
    assert!(write.publish());
    assert_eq!(
        outputs
            .get(first.entity)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(0, 0),
        Color::rgba(0, 0, 0, 0)
    );
}

#[test]
fn a_reserved_working_set_writes_without_allocating_and_counts_both_history_slots() {
    let outputs = Outputs::<WidgetTexture>::with_budget(3 * TEXTURE_BYTES);
    let source = key(1, 1);
    let retained = key(2, 1);
    outputs.reserve(1).unwrap();
    outputs.set_retained(retained.entity, true).unwrap();
    let previous = Color::rgb(11, 29, 47);
    {
        let _batch = outputs.begin_batch();
        assert!(outputs.reserve_texture(retained, None).unwrap());
        let mut write = outputs.prepare_texture(retained).unwrap();
        write
            .texture_mut()
            .pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &previous);
        assert!(write.publish());
    }
    let _batch = outputs.begin_batch();
    assert!(outputs.reserve_texture(source, None).unwrap());
    assert!(outputs.reserve_texture(key(2, 2), None).unwrap());
    assert_eq!(outputs.storage().budget.allocated(), 3 * TEXTURE_BYTES);
    assert!(!outputs.reserve_texture(key(3, 1), None).unwrap());
    assert!(
        outputs
            .storage()
            .textures
            .values()
            .all(|slots| !slots.writing)
    );

    let allocations = tracked_allocations(|| {
        assert!(outputs.prepare_texture(source).unwrap().publish());
        let mut write = outputs.prepare_texture(key(2, 2)).unwrap();
        {
            let source_read = outputs.get(source.entity).unwrap();
            assert_eq!(source_read.texture().width, source.w);
            let history = outputs.previous(retained.entity).unwrap();
            write.texture_mut().pixels_mut().unwrap().set_pixel(
                0,
                0,
                &history.texture().read_pixels().unwrap().get_pixel(0, 0),
            );
        }
        assert!(write.publish());
    });
    assert_eq!(allocations, 0);
    assert_eq!(
        outputs
            .get(retained.entity)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(0, 0),
        previous
    );
    assert_eq!(
        outputs
            .previous(retained.entity)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(0, 0),
        previous
    );
    assert_eq!(outputs.storage().budget.allocated(), 3 * TEXTURE_BYTES);
}

#[test]
fn matching_retained_reservation_does_not_enable_another_write_in_the_batch() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    let input = key(1, 1);
    outputs.reserve(1).unwrap();
    outputs.set_retained(input.entity, true).unwrap();
    let _batch = outputs.begin_batch();
    assert!(outputs.reserve_texture(input, None).unwrap());
    assert!(outputs.prepare_test_texture(input).unwrap().publish());
    assert!(outputs.reserve_texture(input, None).unwrap());
    assert!(!outputs.reserve_texture(key(1, 2), None).unwrap());
    assert!(outputs.prepare_test_texture(input).is_none());
    assert!(outputs.texture_matches(input).unwrap());
}

#[test]
fn zero_shape_releases_retained_pixels_and_restores_without_old_history() {
    for (w, h) in [(0, 4), (4, 0)] {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let entity = key(1, 1).entity;
        outputs.reserve(1).unwrap();
        outputs.set_retained(entity, true).unwrap();
        for generation in [1, 2] {
            let _batch = outputs.begin_batch();
            assert!(
                outputs
                    .prepare_test_texture(key(1, generation))
                    .unwrap()
                    .publish()
            );
        }
        assert!(outputs.get(entity).is_some());
        assert!(outputs.previous(entity).is_some());
        assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);

        assert!(
            !outputs
                .reserve_texture(BufferKey { w, h, ..key(1, 3) }, None)
                .unwrap()
        );
        assert!(outputs.get(entity).is_none());
        assert!(outputs.previous(entity).is_none());
        assert!(outputs.state(entity).unwrap().retained);
        assert_eq!(outputs.storage().budget.allocated(), 0);

        let replacement = BufferKey { w: 8, ..key(2, 1) };
        assert!(outputs.prepare_test_texture(replacement).unwrap().publish());
        assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);
        assert!(
            outputs
                .prepare_test_texture(BufferKey {
                    h: 0,
                    ..replacement
                })
                .is_none()
        );
        assert_eq!(outputs.storage().budget.allocated(), 0);

        let _batch = outputs.begin_batch();
        assert!(outputs.reserve_texture(key(1, 4), None).unwrap());
        assert!(outputs.prepare_test_texture(key(1, 4)).unwrap().publish());
        assert!(outputs.get(entity).is_some());
        assert!(outputs.previous(entity).is_none());
    }
}

#[test]
fn zero_shape_discards_an_active_lease_without_releasing_its_live_budget() {
    for publish in [false, true] {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let input = key(1, 1);
        outputs.reserve(1).unwrap();
        outputs.set_retained(input.entity, true).unwrap();
        {
            let _batch = outputs.begin_batch();
            assert!(outputs.prepare_test_texture(input).unwrap().publish());
        }
        let _batch = outputs.begin_batch();
        let pending = outputs.prepare_test_texture(key(1, 2)).unwrap();
        assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);
        assert!(
            !outputs
                .reserve_texture(BufferKey { w: 0, ..key(1, 3) }, None)
                .unwrap()
        );
        assert!(outputs.get(input.entity).is_none());
        assert!(outputs.previous(input.entity).is_none());
        assert_eq!(outputs.storage().budget.allocated(), TEXTURE_BYTES);
        assert!(!outputs.reserve_texture(key(1, 4), None).unwrap());
        assert!(outputs.prepare_test_texture(key(2, 1)).unwrap().publish());
        assert!(!outputs.reserve_texture(key(3, 1), None).unwrap());
        if publish {
            assert!(!pending.publish());
        } else {
            drop(pending);
        }
        assert!(outputs.get(input.entity).is_none());
        assert!(outputs.previous(input.entity).is_none());
        assert!(outputs.prepare_test_texture(key(3, 1)).unwrap().publish());
    }
}

#[test]
fn taking_the_plan_releases_the_storage_borrow_before_production() {
    let outputs = Outputs::<WidgetTexture>::with_budget(TEXTURE_BYTES);
    let plan = outputs.take_texture_plan().unwrap();
    assert!(outputs.take_texture_plan().is_none());
    let input = key(1, 1);
    assert!(outputs.reserve_texture(input, None).unwrap());
    assert!(outputs.prepare_test_texture(input).unwrap().publish());
    assert!(outputs.get(input.entity).is_some());
    outputs.put_texture_plan(plan);
    assert!(outputs.take_texture_plan().is_some());
}

#[test]
fn releasing_history_keeps_current_and_reclaims_the_spare_pixel_slot() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    let entity = key(1, 1).entity;
    outputs.reserve(1).unwrap();
    outputs.set_retained(entity, true).unwrap();
    for generation in [1, 2] {
        let _batch = outputs.begin_batch();
        let mut write = outputs.prepare_test_texture(key(1, generation)).unwrap();
        write.texture_mut().pixels_mut().unwrap().set_pixel(
            0,
            0,
            &Color::rgb(10, 20, generation as u8),
        );
        assert!(write.publish());
    }
    outputs.set_retained(entity, false).unwrap();
    outputs.reconcile_textures(|candidate| candidate == entity, |_| true);
    assert_eq!(
        outputs
            .get(entity)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(0, 0),
        Color::rgb(10, 20, 2)
    );
    assert!(outputs.previous(entity).is_none());
    assert!(outputs.prepare_test_texture(key(2, 1)).unwrap().publish());
    assert!(outputs.prepare_test_texture(key(1, 3)).unwrap().publish());
    assert_eq!(outputs.storage().budget.allocated(), 2 * TEXTURE_BYTES);
}

#[test]
fn concurrent_write_is_rejected_without_replacing_the_active_lease() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    let first_key = key(1, 1);
    let expected = Color::rgb(19, 37, 71);
    let mut first = outputs.prepare_test_texture(first_key).unwrap();
    first
        .texture_mut()
        .pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &expected);

    assert!(outputs.prepare_test_texture(key(1, 2)).is_none());
    assert!(outputs.get(first_key.entity).is_none());
    assert!(first.publish());
    assert!(outputs.texture_matches(first_key).unwrap());
    assert_eq!(
        outputs
            .get(first_key.entity)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(0, 0),
        expected
    );
}

#[test]
fn trimming_an_active_write_defers_release_until_its_lease_finishes() {
    for publish in [false, true] {
        let outputs = Outputs::<WidgetTexture>::with_budget(TEXTURE_BYTES);
        let first_key = key(1, 1);
        let pending = outputs.prepare_test_texture(first_key).unwrap();
        outputs.trim_memory();

        assert!(outputs.get(first_key.entity).is_none());
        assert!(outputs.prepare_test_texture(key(2, 1)).is_none());
        if publish {
            assert!(!pending.publish());
        } else {
            drop(pending);
        }

        assert!(outputs.get(first_key.entity).is_none());
        assert!(outputs.prepare_test_texture(key(2, 1)).unwrap().publish());
    }
}

#[test]
fn retained_output_is_written_once_per_batch() {
    let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
    let entity = key(1, 1).entity;
    outputs.reserve(1).unwrap();
    outputs.set_retained(entity, true).unwrap();
    let first_color = Color::rgb(200, 10, 20);
    let second_color = Color::rgb(30, 40, 220);
    {
        let _batch = outputs.begin_batch();
        let mut write = outputs.prepare_test_texture(key(1, 1)).unwrap();
        write
            .texture_mut()
            .pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &first_color);
        assert!(write.publish());
    }
    {
        let _batch = outputs.begin_batch();
        let mut write = outputs.prepare_test_texture(key(1, 2)).unwrap();
        write
            .texture_mut()
            .pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &second_color);
        assert!(write.publish());

        assert!(outputs.prepare_test_texture(key(1, 3)).is_none());
        assert_eq!(
            outputs
                .get(entity)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            second_color
        );
        assert_eq!(
            outputs
                .previous(entity)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            first_color
        );
    }
    {
        let _batch = outputs.begin_batch();
        assert_eq!(
            outputs
                .previous(entity)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            second_color
        );
    }
}
