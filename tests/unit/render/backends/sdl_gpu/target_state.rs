use super::*;

#[test]
fn clear_region_overwrites_pixels_and_restores_driver_state() {
    check_clear(TargetAlpha::Straight, [200, 100, 50, 128]);
    check_clear(TargetAlpha::Premultiplied, [100, 50, 25, 128]);
}

fn check_clear(alpha: TargetAlpha, encoded: [u8; 4]) {
    let mut canvas = sdl2::surface::Surface::new(4, 3, sdl2::pixels::PixelFormatEnum::RGBA32)
        .unwrap()
        .into_canvas()
        .unwrap();
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(13, 29, 47, 255));
    canvas.clear();
    canvas.set_viewport(sdl2::rect::Rect::new(1, 1, 2, 2));
    canvas.set_clip_rect(sdl2::rect::Rect::new(0, 0, 1, 1));
    canvas.set_scale(1.5, 1.5).unwrap();
    canvas.set_blend_mode(sdl2::render::BlendMode::Add);
    let before = (
        canvas.viewport(),
        canvas.clip_rect(),
        canvas.scale(),
        canvas.draw_color(),
        canvas.blend_mode(),
    );
    for _ in 0..4 {
        clear_region(
            &canvas,
            PhysicalRect::new(1, 1, 3, 2).unwrap(),
            Color::rgba(200, 100, 50, 128),
            alpha,
        )
        .unwrap();
        clear_region(
            &canvas,
            PhysicalRect::new(2, 1, 1, 1).unwrap(),
            Color::rgba(0, 0, 0, 0),
            alpha,
        )
        .unwrap();
    }
    assert_eq!(
        (
            canvas.viewport(),
            canvas.clip_rect(),
            canvas.scale(),
            canvas.draw_color(),
            canvas.blend_mode()
        ),
        before
    );
    canvas.set_scale(1.0, 1.0).unwrap();
    canvas.set_viewport(Option::<sdl2::rect::Rect>::None);
    canvas.set_clip_rect(Option::<sdl2::rect::Rect>::None);
    let mut bytes = [0_u8; 4 * 3 * 4];
    super::super::readback::rgba8(&canvas, PhysicalRect::from_size(4, 3), &mut bytes, 16).unwrap();
    for y in 0..3 {
        for x in 0..4 {
            let expected = if x == 2 && y == 1 {
                [0, 0, 0, 0]
            } else if x >= 1 && y >= 1 {
                encoded
            } else {
                [13, 29, 47, 255]
            };
            let offset = (y * 4 + x) * 4;
            assert_eq!(&bytes[offset..offset + 4], &expected);
        }
    }
}
