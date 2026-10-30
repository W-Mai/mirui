use super::*;
use crate::render::texture::Texture;
use crate::test_support::tracked_allocations;

type TestDevice = Device<sdl2::surface::Surface<'static>>;

#[path = "target_tests/source.rs"]
pub(crate) mod source;

fn device() -> Rc<TestDevice> {
    let canvas = sdl2::surface::Surface::new(16, 16, sdl2::pixels::PixelFormatEnum::RGBA32)
        .unwrap()
        .into_canvas()
        .unwrap();
    let creator = canvas.texture_creator();
    Rc::new(Device::new(canvas, creator).unwrap())
}

#[cfg(feature = "sync-cache")]
#[test]
fn transferable_targets_keep_driver_access_local_and_close_before_worker_release() {
    use crate::render::{budget::MemoryBudget, target::TargetStorage};
    use std::sync::Barrier;
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Target<sdl2::surface::Surface<'static>>>();
    let device = device();
    let erased: Box<dyn core::any::Any> = target(&device);
    let mut source = erased
        .downcast::<Target<sdl2::surface::Surface<'static>>>()
        .ok()
        .unwrap();
    let bytes = source.allocated_bytes();
    let limit = bytes + TargetStorage::OWNER_BYTES;
    let budget = MemoryBudget::new(limit);
    let receipt = TargetStorage::new(
        budget.reserve(bytes).unwrap().commit(bytes).unwrap(),
        budget.reserve(0).unwrap(),
        budget
            .reserve(TargetStorage::OWNER_BYTES)
            .unwrap()
            .commit(TargetStorage::OWNER_BYTES)
            .unwrap(),
    );
    source.retain_storage(receipt).unwrap();
    let binding = Binding::begin(&device, &*source, TargetUpdate::Rebuild).unwrap();
    fill(&device, sdl2::rect::Rect::new(0, 0, 4, 4), (0, 255, 0, 255));
    binding.finish(&device, true).unwrap();
    assert_eq!(
        pixels(&*source).read_pixels().unwrap().get_pixel(1, 1),
        crate::types::Color::rgb(0, 255, 0)
    );
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let source = source;
            source.validate().unwrap();
            let mut bytes = [0; 64];
            let mut destination = Texture::new(&mut bytes, 4, 4, ColorFormat::RGBA8888).unwrap();
            assert_eq!(
                source.read_pixels_into(
                    source.descriptor().region(),
                    &mut destination.pixels_mut().unwrap()
                ),
                Err(RenderError::WrongThread)
            );
            barrier.wait();
            barrier.wait();
            assert_eq!(source.validate(), Err(RenderError::InvalidTexture));
            assert_eq!(
                source.read_pixels_into(
                    source.descriptor().region(),
                    &mut destination.pixels_mut().unwrap()
                ),
                Err(RenderError::InvalidTexture)
            );
            assert_eq!(tracked_allocations(|| drop(source)), 0);
        });
        barrier.wait();
        assert_eq!(tracked_allocations(|| device.close_targets().unwrap()), 0);
        assert_eq!(
            Rc::strong_count(&device),
            1,
            "closed tokens do not retain the raw device"
        );
        assert_ne!(budget.allocated(), 0, "worker metadata remains charged");
        barrier.wait();
        worker.join().unwrap();
    });
    drop(device);
    assert_eq!(budget.available(), limit);
}

#[cfg(feature = "sync-cache")]
#[test]
fn worker_retirement_releases_raw_storage_only_at_owner_maintenance() {
    let device = device();
    let erased: Box<dyn core::any::Any> = target(&device);
    let source = erased
        .downcast::<Target<sdl2::surface::Surface<'static>>>()
        .ok()
        .unwrap();
    assert_eq!(Rc::strong_count(&device), 2);
    std::thread::spawn(move || drop(source)).join().unwrap();
    assert_eq!(Rc::strong_count(&device), 2);
    assert_eq!(tracked_allocations(|| device.collect_targets().unwrap()), 0);
    assert_eq!(Rc::strong_count(&device), 1);
}

#[cfg(feature = "sync-cache")]
#[test]
fn batch_plans_allow_owner_metadata_to_be_admitted_by_an_earlier_target() {
    use crate::render::{budget::MemoryBudget, target::TargetStorage};
    let device = device();
    let layout = PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap();
    let cold_plan = plan(&device, layout).unwrap().unwrap();
    let mut first = create(&device, &cold_plan).unwrap();
    let limit = 2 * (cold_plan.storage_bytes + TargetStorage::OWNER_BYTES);
    let budget = MemoryBudget::new(limit);
    let attach = |target: &mut dyn BackendTarget| {
        let bytes = target.allocated_bytes();
        target
            .retain_storage(TargetStorage::new(
                budget.reserve(bytes).unwrap().commit(bytes).unwrap(),
                budget.reserve(0).unwrap(),
                budget
                    .reserve(TargetStorage::OWNER_BYTES)
                    .unwrap()
                    .commit(TargetStorage::OWNER_BYTES)
                    .unwrap(),
            ))
            .unwrap();
    };
    attach(&mut *first);
    let warm_plan = plan(&device, layout).unwrap().unwrap();
    assert!(warm_plan.storage_bytes < cold_plan.storage_bytes);
    let mut second = create(&device, &cold_plan).unwrap();
    assert_eq!(second.allocated_bytes(), warm_plan.storage_bytes);
    attach(&mut *second);
    assert!(can_reuse(&device, &*first, &warm_plan).unwrap());
    assert!(can_reuse(&device, &*second, &warm_plan).unwrap());
    drop(first);
    drop(second);
    device.collect_targets().unwrap();
    assert_eq!(Rc::strong_count(&device), 1);
    drop(device);
    assert_eq!(budget.available(), limit);
}

fn target(device: &Rc<TestDevice>) -> Box<dyn BackendTarget> {
    let plan = plan(
        device,
        PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap(),
    )
    .unwrap()
    .unwrap();
    create(device, &plan).unwrap()
}

fn fill(device: &TestDevice, rect: sdl2::rect::Rect, color: (u8, u8, u8, u8)) {
    let mut canvas = device.canvas().unwrap();
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
        color.0, color.1, color.2, color.3,
    ));
    canvas.fill_rect(rect).unwrap();
}

fn pixels(target: &dyn BackendTarget) -> Texture<'static> {
    let mut image = Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap();
    target
        .read_pixels_into(
            target.descriptor().region(),
            &mut image.pixels_mut().unwrap(),
        )
        .unwrap();
    image
}

#[test]
fn source_region_preserves_coordinates_padding_and_alpha() {
    let device = device();
    let source = target(&device);
    let binding = Binding::begin(&device, &*source, TargetUpdate::Rebuild).unwrap();
    fill(&device, sdl2::rect::Rect::new(0, 0, 2, 4), (255, 0, 0, 128));
    fill(&device, sdl2::rect::Rect::new(2, 0, 2, 4), (0, 255, 0, 255));
    binding.finish(&device, true).unwrap();
    let full = pixels(&*source);
    let full = full.read_pixels().unwrap();
    let mut bytes = [0xa5; 20];
    let mut dst = Texture::new(&mut bytes, 2, 2, ColorFormat::RGBA8888)
        .unwrap()
        .with_stride(12)
        .unwrap();
    let area = crate::render::texture::Region::new(1, 1, 2, 2).unwrap();
    let allocations = tracked_allocations(|| {
        source
            .read_pixels_into(area, &mut dst.pixels_mut().unwrap())
            .unwrap();
    });
    assert_eq!(allocations, 0);
    let actual = dst.read_pixels().unwrap();
    for y in 0..2 {
        for x in 0..2 {
            assert_eq!(actual.get_pixel(x, y), full.get_pixel(x + 1, y + 1));
        }
    }
    assert_eq!(&actual.bytes()[8..12], &[0xa5; 4]);
}

#[test]
fn retained_frame_is_restored_after_nested_outputs_without_reallocation() {
    let device = device();
    let outer = target(&device);
    let inner = target(&device);
    assert!(device.prepare_persistent_target((16, 16)).unwrap());
    let allocations = tracked_allocations(|| {
        for _ in 0..8 {
            assert!(device.prepare_persistent_target((16, 16)).unwrap());
            assert!(device.begin_persistent_frame().unwrap());
            let frame = {
                let canvas = device.canvas().unwrap();
                core::ptr::NonNull::new(unsafe { sdl2::sys::SDL_GetRenderTarget(canvas.raw()) })
                    .unwrap()
            };
            fill(
                &device,
                sdl2::rect::Rect::new(0, 0, 16, 16),
                (7, 29, 53, 255),
            );
            let parent = Binding::begin(&device, &*outer, TargetUpdate::Rebuild).unwrap();
            fill(&device, sdl2::rect::Rect::new(0, 0, 4, 4), (255, 0, 0, 255));
            let child = Binding::begin(&device, &*inner, TargetUpdate::Rebuild).unwrap();
            fill(&device, sdl2::rect::Rect::new(0, 0, 4, 4), (0, 255, 0, 255));
            child.finish(&device, true).unwrap();
            parent.finish(&device, true).unwrap();
            {
                let canvas = device.canvas().unwrap();
                assert!(target_state::is_current(&canvas, frame));
                let mut pixel = [0; 4];
                super::super::readback::rgba8(
                    &canvas,
                    crate::types::PhysicalRect::new(0, 0, 1, 1).unwrap(),
                    &mut pixel,
                    4,
                )
                .unwrap();
                assert_eq!(pixel, [7, 29, 53, 255]);
            }
            assert!(device.finish_persistent_frame().unwrap());
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn selected_target_identity_survives_format_conversion_and_nested_restore() {
    let device = device();
    let outer = device
        .creator
        .create_texture_target(sdl2::pixels::PixelFormatEnum::RGBA4444, 4, 4)
        .unwrap();
    let inner = device
        .creator
        .create_texture_target(sdl2::pixels::PixelFormatEnum::RGBA32, 4, 4)
        .unwrap();
    let canvas = device.canvas().unwrap();
    let root = DriverState::capture(&canvas).unwrap();
    let selected_outer = target_state::select(&canvas, &outer).unwrap();
    assert!(target_state::is_current(&canvas, selected_outer));
    let parent = DriverState::capture(&canvas).unwrap();
    let selected_inner = target_state::select(&canvas, &inner).unwrap();
    assert!(target_state::is_current(&canvas, selected_inner));
    assert!(!target_state::is_current(&canvas, selected_outer));
    parent.restore(&canvas).unwrap();
    assert!(target_state::is_current(&canvas, selected_outer));
    assert!(!target_state::is_current(&canvas, selected_inner));
    root.restore(&canvas).unwrap();
    assert!(!target_state::is_current(&canvas, selected_outer));
}

#[test]
fn reuse_checks_storage_without_binding_or_accepting_invalid_contents() {
    let first = device();
    let second = device();
    let image = target(&first);
    let requested = plan(&first, image.descriptor().layout).unwrap().unwrap();
    let allocations = tracked_allocations(|| {
        for _ in 0..16 {
            assert!(can_reuse(&first, &*image, &requested).unwrap());
            assert!(!can_reuse(&second, &*image, &requested).unwrap());
        }
    });
    assert_eq!(allocations, 0);
    assert!(Binding::begin(&first, &*image, TargetUpdate::Incremental).is_err());
    let mut changed = requested;
    changed.workspace_bytes += 1;
    assert!(!can_reuse(&first, &*image, &changed).unwrap());
    let binding = Binding::begin(&first, &*image, TargetUpdate::Rebuild).unwrap();
    assert_eq!(
        can_reuse(&first, &*image, &changed),
        Err(RenderError::InvalidScope)
    );
    assert_eq!(
        can_reuse(&first, &*image, &requested),
        Err(RenderError::InvalidScope)
    );
    binding.finish(&first, false).unwrap();
    assert!(can_reuse(&first, &*image, &requested).unwrap());
    assert!(Binding::begin(&first, &*image, TargetUpdate::Incremental).is_err());
    {
        let _lease = first.canvas().unwrap();
        assert_eq!(
            can_reuse(&first, &*image, &requested),
            Err(RenderError::InvalidScope)
        );
    }
    first.invalidate();
    assert!(!can_reuse(&first, &*image, &requested).unwrap());
    first.poison();
    assert!(can_reuse(&first, &*image, &requested).is_err());
}

#[test]
fn target_rebuild_incremental_failure_and_owner_lifetime() {
    let device = device();
    let target = target(&device);
    assert!(matches!(
        Binding::begin(&device, &*target, TargetUpdate::Incremental),
        Err(RenderError::InvalidTexture)
    ));
    let binding = Binding::begin(&device, &*target, TargetUpdate::Rebuild).unwrap();
    binding.finish(&device, true).unwrap();
    assert!(
        pixels(&*target)
            .read_pixels()
            .unwrap()
            .bytes()
            .iter()
            .all(|p| *p == 0)
    );
    let binding = Binding::begin(&device, &*target, TargetUpdate::Incremental).unwrap();
    fill(&device, sdl2::rect::Rect::new(0, 0, 2, 4), (255, 0, 0, 128));
    binding.finish(&device, true).unwrap();
    let first = pixels(&*target);
    assert_eq!(
        first.read_pixels().unwrap().get_pixel(0, 0),
        crate::types::Color::rgba(255, 0, 0, 128)
    );
    let binding = Binding::begin(&device, &*target, TargetUpdate::Incremental).unwrap();
    fill(&device, sdl2::rect::Rect::new(2, 0, 2, 4), (0, 255, 0, 255));
    binding.finish(&device, true).unwrap();
    let image = pixels(&*target);
    assert_eq!(
        image.read_pixels().unwrap().get_pixel(0, 0),
        first.read_pixels().unwrap().get_pixel(0, 0)
    );
    assert_eq!(
        image.read_pixels().unwrap().get_pixel(3, 0),
        crate::types::Color::rgb(0, 255, 0)
    );
    let binding = Binding::begin(&device, &*target, TargetUpdate::Incremental).unwrap();
    binding.finish(&device, false).unwrap();
    assert!(
        target
            .read_pixels_into(
                target.descriptor().region(),
                &mut Texture::owned(4, 4, ColorFormat::RGBA8888)
                    .unwrap()
                    .pixels_mut()
                    .unwrap()
            )
            .is_err()
    );
    assert!(Binding::begin(&device, &*target, TargetUpdate::Incremental).is_err());
    Binding::begin(&device, &*target, TargetUpdate::Rebuild)
        .unwrap()
        .finish(&device, true)
        .unwrap();
    drop(device);
    assert!(
        pixels(&*target)
            .read_pixels()
            .unwrap()
            .bytes()
            .iter()
            .all(|p| *p == 0)
    );
}

#[test]
fn nested_targets_restore_parent_state_and_reject_feedback() {
    let device = device();
    let outer = target(&device);
    let inner = target(&device);
    {
        let mut canvas = device.canvas().unwrap();
        canvas.set_scale(2.0, 3.0).unwrap();
        canvas.set_viewport(sdl2::rect::Rect::new(1, 1, 6, 5));
        canvas.set_clip_rect(sdl2::rect::Rect::new(2, 1, 2, 2));
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(1, 2, 3, 4));
    }
    let saved = {
        let canvas = device.canvas().unwrap();
        (
            canvas.viewport(),
            canvas.clip_rect(),
            canvas.scale(),
            canvas.draw_color(),
        )
    };
    let parent = Binding::begin(&device, &*outer, TargetUpdate::Rebuild).unwrap();
    fill(&device, sdl2::rect::Rect::new(0, 0, 4, 4), (255, 0, 0, 255));
    let child = Binding::begin(&device, &*inner, TargetUpdate::Rebuild).unwrap();
    assert!(Binding::begin(&device, &*outer, TargetUpdate::Rebuild).is_err());
    let mut image = Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap();
    assert!(
        outer
            .read_pixels_into(
                outer.descriptor().region(),
                &mut image.pixels_mut().unwrap()
            )
            .is_err()
    );
    fill(&device, sdl2::rect::Rect::new(0, 0, 4, 4), (0, 255, 0, 255));
    child.finish(&device, true).unwrap();
    inner
        .read_pixels_into(
            inner.descriptor().region(),
            &mut image.pixels_mut().unwrap(),
        )
        .unwrap();
    fill(&device, sdl2::rect::Rect::new(0, 0, 1, 1), (0, 0, 255, 255));
    parent.finish(&device, true).unwrap();
    let actual = {
        let canvas = device.canvas().unwrap();
        (
            canvas.viewport(),
            canvas.clip_rect(),
            canvas.scale(),
            canvas.draw_color(),
        )
    };
    assert_eq!(actual, saved);
    let image = pixels(&*outer);
    assert_eq!(
        image.read_pixels().unwrap().get_pixel(0, 0),
        crate::types::Color::rgb(0, 0, 255)
    );
    assert_eq!(
        image.read_pixels().unwrap().get_pixel(2, 2),
        crate::types::Color::rgb(255, 0, 0)
    );
    assert_eq!(
        pixels(&*inner).read_pixels().unwrap().get_pixel(2, 2),
        crate::types::Color::rgb(0, 255, 0)
    );
}

#[test]
fn invalid_device_and_foreign_targets_do_not_submit() {
    let first = device();
    let second = device();
    let target = target(&first);
    assert!(Binding::begin(&second, &*target, TargetUpdate::Rebuild).is_err());
    let binding = Binding::begin(&first, &*target, TargetUpdate::Rebuild).unwrap();
    first.poison();
    assert!(binding.finish(&first, true).is_err());
    assert!(first.canvas().is_err());
    first.invalidate();
    assert_eq!(target.validate(), Err(RenderError::InvalidTexture));
    assert!(Binding::begin(&first, &*target, TargetUpdate::Rebuild).is_err());
}

#[test]
fn prepared_nested_scopes_and_source_readback_do_not_allocate_rust_storage() {
    let device = device();
    let outer = target(&device);
    let inner = target(&device);
    Binding::begin(&device, &*outer, TargetUpdate::Rebuild)
        .unwrap()
        .finish(&device, true)
        .unwrap();
    Binding::begin(&device, &*inner, TargetUpdate::Rebuild)
        .unwrap()
        .finish(&device, true)
        .unwrap();
    let mut image = Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..8 {
                let a = Binding::begin(&device, &*outer, TargetUpdate::Incremental).unwrap();
                let b = Binding::begin(&device, &*inner, TargetUpdate::Incremental).unwrap();
                b.finish(&device, true).unwrap();
                inner
                    .read_pixels_into(
                        inner.descriptor().region(),
                        &mut image.pixels_mut().unwrap(),
                    )
                    .unwrap();
                a.finish(&device, true).unwrap();
            }
        }),
        0
    );
}

#[test]
fn abandoned_or_out_of_order_bindings_cannot_restore_dropped_targets() {
    for abandon in [false, true] {
        let device = device();
        let outer = target(&device);
        let inner = target(&device);
        let parent = Binding::begin(&device, &*outer, TargetUpdate::Rebuild).unwrap();
        let child = Binding::begin(&device, &*inner, TargetUpdate::Rebuild).unwrap();
        if abandon {
            drop(parent);
        } else {
            assert_eq!(parent.finish(&device, true), Err(RenderError::InvalidScope));
        }
        drop(outer);
        assert!(child.finish(&device, true).is_err());
        assert_eq!(device.check(), Err(RenderError::BackendFailure));
    }
}

#[test]
fn reset_between_bind_and_restore_rejects_old_driver_state() {
    let device = device();
    let target = target(&device);
    let binding = Binding::begin(&device, &*target, TargetUpdate::Rebuild).unwrap();
    device.invalidate();
    assert_eq!(
        binding.finish(&device, true),
        Err(RenderError::InvalidScope)
    );
    assert_eq!(device.check(), Err(RenderError::BackendFailure));
}
