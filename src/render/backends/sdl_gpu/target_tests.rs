use super::*;
use crate::render::texture::Texture;
use crate::test_support::tracked_allocations;

type TestDevice = Device<sdl2::surface::Surface<'static>>;

fn device() -> Rc<TestDevice> {
    let canvas = sdl2::surface::Surface::new(16, 16, sdl2::pixels::PixelFormatEnum::RGBA32)
        .unwrap()
        .into_canvas()
        .unwrap();
    let creator = canvas.texture_creator();
    Rc::new(Device::new(canvas, creator))
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
        .read_pixels_into(&mut image.pixels_mut().unwrap())
        .unwrap();
    image
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
            .read_pixels_into(&mut image.pixels_mut().unwrap())
            .is_err()
    );
    fill(&device, sdl2::rect::Rect::new(0, 0, 4, 4), (0, 255, 0, 255));
    child.finish(&device, true).unwrap();
    inner
        .read_pixels_into(&mut image.pixels_mut().unwrap())
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
                    .read_pixels_into(&mut image.pixels_mut().unwrap())
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
