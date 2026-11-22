use super::*;
use crate::test_support::tracked_allocations;
use crate::types::PhysicalRect;
use alloc::rc::Rc;

fn device() -> Rc<Device<sdl2::surface::Surface<'static>>> {
    let canvas = sdl2::surface::Surface::new(4, 3, sdl2::pixels::PixelFormatEnum::RGBA32)
        .unwrap()
        .into_canvas()
        .unwrap();
    let creator = canvas.texture_creator();
    Rc::new(Device::new(canvas, creator).unwrap())
}

fn read_pixel(device: &Device<sdl2::surface::Surface<'static>>, x: u16, y: u16) -> [u8; 4] {
    let mut pixel = [0; 4];
    super::super::readback::rgba8(
        &*device.canvas().unwrap(),
        PhysicalRect::new(x, y, 1, 1).unwrap(),
        &mut pixel,
        4,
    )
    .unwrap();
    pixel
}

#[test]
fn transient_frames_retain_clip_without_claiming_native_target_storage() {
    let device = device();
    let initial_clip = sdl2::rect::Rect::new(0, 0, 1, 1);
    let changed = sdl2::rect::Rect::new(1, 1, 2, 2);
    device.canvas().unwrap().set_clip_rect(initial_clip);
    for abort in [false, true] {
        assert_eq!(
            tracked_allocations(|| assert!(!device.begin_frame().unwrap())),
            0
        );
        assert!(device.frame_active());
        assert!(!device.persistent_frame_active());
        assert!(!device.has_persistent_target());
        assert_eq!(device.begin_frame(), Err(RenderError::InvalidScope));
        {
            let mut canvas = device.canvas().unwrap();
            canvas.set_clip_rect(changed);
            device.finish_draw(&mut canvas);
            assert_eq!(canvas.clip_rect(), Some(changed));
        }
        assert_eq!(
            tracked_allocations(|| {
                if abort {
                    device.abort_frame();
                } else {
                    assert!(!device.finish_frame().unwrap());
                }
            }),
            0
        );
        assert!(!device.frame_active());
        assert_eq!(device.canvas().unwrap().clip_rect(), Some(initial_clip));
    }
    assert!(!device.begin_frame().unwrap());
    device.canvas().unwrap().set_clip_rect(changed);
    device.invalidate();
    assert!(!device.frame_active());
    assert_eq!(device.canvas().unwrap().clip_rect(), Some(initial_clip));
    let mut canvas = device.canvas().unwrap();
    device.finish_draw(&mut canvas);
    assert_eq!(canvas.clip_rect(), None);
}

#[test]
fn transient_frames_restore_nested_target_and_original_driver_properties() {
    let device = device();
    let original_pixel = read_pixel(&device, 0, 0);
    let expected = {
        let canvas = device.canvas().unwrap();
        (
            canvas.viewport(),
            canvas.clip_rect(),
            canvas.scale(),
            canvas.blend_mode(),
        )
    };
    assert!(!device.begin_frame().unwrap());
    let target = device
        .creator
        .create_texture_target(PixelFormatEnum::RGBA32, 2, 2)
        .unwrap();
    {
        let mut canvas = device.canvas().unwrap();
        let clip = sdl2::rect::Rect::new(1, 1, 2, 2);
        canvas.set_clip_rect(clip);
        let parent = DriverState::capture(&canvas).unwrap();
        target_state::select(&canvas, &target).unwrap();
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(201, 5, 9, 255));
        canvas.clear();
        canvas.set_clip_rect(sdl2::rect::Rect::new(0, 0, 1, 1));
        device.finish_draw(&mut canvas);
        parent.restore(&canvas).unwrap();
        assert_eq!(canvas.clip_rect(), Some(clip));
        canvas.set_viewport(sdl2::rect::Rect::new(1, 1, 2, 2));
        canvas.set_scale(1.5, 1.5).unwrap();
        canvas.set_blend_mode(BlendMode::Add);
    }
    assert!(!device.finish_frame().unwrap());
    assert_eq!(read_pixel(&device, 0, 0), original_pixel);
    let canvas = device.canvas().unwrap();
    assert_eq!(
        (
            canvas.viewport(),
            canvas.clip_rect(),
            canvas.scale(),
            canvas.blend_mode()
        ),
        expected
    );
}

#[test]
fn command_clip_reuse_is_confined_to_owned_frames_and_restores_on_abort() {
    let device = device();
    let clip = sdl2::rect::Rect::new(0, 0, 1, 1);
    {
        let mut canvas = device.canvas().unwrap();
        canvas.set_clip_rect(clip);
        device.finish_draw(&mut canvas);
        assert_eq!(canvas.clip_rect(), None);
        canvas.set_clip_rect(clip);
    }
    device.prepare_persistent_target((4, 3)).unwrap();
    device.begin_frame().unwrap();
    let changed = sdl2::rect::Rect::new(1, 1, 2, 2);
    {
        let mut canvas = device.canvas().unwrap();
        canvas.set_clip_rect(changed);
        device.finish_draw(&mut canvas);
        assert_eq!(canvas.clip_rect(), Some(changed));
        let outer = DriverState::capture(&canvas).unwrap();
        let inner = device
            .creator
            .create_texture_target(PixelFormatEnum::RGBA32, 2, 2)
            .unwrap();
        target_state::select(&canvas, &inner).unwrap();
        assert_eq!(canvas.clip_rect(), None);
        canvas.set_clip_rect(clip);
        device.finish_draw(&mut canvas);
        assert_eq!(canvas.clip_rect(), Some(clip));
        outer.restore(&canvas).unwrap();
        assert_eq!(canvas.clip_rect(), Some(changed));
    }
    device.abort_frame();
    assert_eq!(device.canvas().unwrap().clip_rect(), Some(clip));
    device.begin_frame().unwrap();
    {
        let mut canvas = device.canvas().unwrap();
        canvas.set_clip_rect(changed);
        device.finish_draw(&mut canvas);
    }
    device.finish_frame().unwrap();
    assert_eq!(device.canvas().unwrap().clip_rect(), Some(clip));
}

#[test]
fn persistent_frames_retain_undamaged_pixels_and_replace_window_alpha() {
    let device = device();
    assert!(device.prepare_persistent_target((4, 3)).unwrap());
    for (x, color) in [(0, [120, 30, 60, 128]), (2, [7, 19, 83, 255])] {
        assert!(device.begin_frame().unwrap());
        {
            let mut canvas = device.canvas().unwrap();
            canvas.set_blend_mode(BlendMode::None);
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                color[0], color[1], color[2], color[3],
            ));
            canvas.fill_rect(sdl2::rect::Rect::new(x, 0, 1, 1)).unwrap();
        }
        assert!(device.finish_frame().unwrap());
        assert_eq!(read_pixel(&device, 0, 0), [120, 30, 60, 128]);
        assert_eq!(read_pixel(&device, 3, 2), [0; 4]);
        // A discarded window backbuffer cannot erase retained pixels.
        if x == 0 {
            let mut canvas = device.canvas().unwrap();
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 255));
            canvas.clear();
        }
    }
    assert_eq!(read_pixel(&device, 2, 0), [7, 19, 83, 255]);
    assert!(!device.finish_frame().unwrap());
}

#[test]
fn persistent_abort_restores_driver_state_and_reset_recreates_storage() {
    let device = device();
    assert!(device.prepare_persistent_target((4, 3)).unwrap());
    let expected = {
        let mut canvas = device.canvas().unwrap();
        canvas.set_viewport(sdl2::rect::Rect::new(1, 1, 2, 2));
        canvas.set_clip_rect(sdl2::rect::Rect::new(0, 0, 1, 1));
        canvas.set_scale(1.5, 1.5).unwrap();
        canvas.set_blend_mode(BlendMode::Add);
        (
            canvas.viewport(),
            canvas.clip_rect(),
            canvas.scale(),
            canvas.blend_mode(),
        )
    };
    assert!(device.begin_frame().unwrap());
    assert_eq!(device.begin_frame(), Err(RenderError::InvalidScope));
    assert_eq!(
        device.prepare_persistent_target((8, 6)),
        Err(RenderError::InvalidScope)
    );
    device.abort_frame();
    {
        let canvas = device.canvas().unwrap();
        assert_eq!(
            (
                canvas.viewport(),
                canvas.clip_rect(),
                canvas.scale(),
                canvas.blend_mode()
            ),
            expected
        );
    }
    device.invalidate();
    assert!(!device.has_persistent_target());
    assert!(device.prepare_persistent_target((8, 6)).unwrap());
    assert!(device.begin_frame().unwrap());
    assert_eq!(read_pixel(&device, 7, 5), [0; 4]);
    assert!(device.finish_frame().unwrap());
    let canvas = device.canvas().unwrap();
    assert_eq!(
        (
            canvas.viewport(),
            canvas.clip_rect(),
            canvas.scale(),
            canvas.blend_mode()
        ),
        expected
    );
}

#[test]
fn short_leases_reject_reentry_and_release_after_unwind() {
    let device = device();
    let lease = device.canvas().unwrap();
    assert!(matches!(device.canvas(), Err(RenderError::InvalidScope)));
    drop(lease);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _lease = device.canvas().unwrap();
        panic!("abort driver operation");
    }));
    assert!(result.is_err());
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..16 {
                let canvas = device.canvas().unwrap();
                assert_eq!(canvas.output_size().unwrap(), (4, 3));
            }
        }),
        0
    );
}

#[test]
fn retained_owner_keeps_canvas_and_shared_generation_alive() {
    let surface_owner = device();
    let resource_owner = Rc::clone(&surface_owner);
    let weak = Rc::downgrade(&surface_owner);
    surface_owner.invalidate();
    assert_eq!(resource_owner.generation(), 1);
    drop(surface_owner);
    {
        let mut canvas = resource_owner.canvas().unwrap();
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(17, 39, 83, 127));
        canvas.clear();
    }
    let mut pixels = [0; 4];
    super::super::readback::rgba8(
        &*resource_owner.canvas().unwrap(),
        PhysicalRect::new(0, 0, 1, 1).unwrap(),
        &mut pixels,
        4,
    )
    .unwrap();
    assert_eq!(pixels, [17, 39, 83, 127]);
    resource_owner.invalidate();
    assert_eq!(resource_owner.generation(), 2);
    drop(resource_owner);
    assert!(weak.upgrade().is_none());
}
