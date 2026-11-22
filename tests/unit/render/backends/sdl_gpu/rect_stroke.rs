use super::*;
use sdl2::{pixels::PixelFormatEnum, surface::Surface};

#[test]
fn clipped_outline_matches_cropped_complete_outline_and_restores_scissor() {
    let viewport = Rect::new(0, 0, 32, 32);
    let areas = [
        Rect::new(4, 4, 20, 20),
        Rect::new(-4, -3, 20, 18),
        Rect::new(27, 28, 10, 12),
        Rect::new(5, 5, 1, 1),
        Rect::new(3, 3, 0, 8),
        Rect {
            x: Fixed::from_f32(3.25),
            y: Fixed::from_f32(4.75),
            w: Fixed::from_f32(18.5),
            h: Fixed::from_f32(15.25),
        },
    ];
    let clips = [
        viewport,
        Rect::new(8, 8, 8, 8),
        Rect::new(0, 0, 12, 12),
        Rect::new(20, 18, 12, 14),
        Rect::new(24, 4, 5, 20),
        Rect::new(4, 4, 0, 20),
    ];
    let draw = |area: &Rect, clip: &Rect| {
        let mut canvas = Surface::new(32, 32, PixelFormatEnum::RGBA32)
            .unwrap()
            .into_canvas()
            .unwrap();
        canvas.set_draw_color(sdl2::pixels::Color::RGB(0, 0, 0));
        canvas.clear();
        canvas.set_draw_color(sdl2::pixels::Color::RGB(255, 255, 255));
        stroke_pixel_rect(&mut canvas, area, clip, false).unwrap();
        assert_eq!(canvas.clip_rect(), None);
        canvas.read_pixels(None, PixelFormatEnum::RGBA32).unwrap()
    };
    for area in areas {
        let complete = draw(&area, &viewport);
        for clip in clips {
            let clipped = draw(&area, &clip);
            let bounds = clip.pixel_bounds();
            for y in 0..32 {
                for x in 0..32 {
                    let offset = (y * 32 + x) as usize * 4;
                    let inside = x >= bounds.0 && x < bounds.2 && y >= bounds.1 && y < bounds.3;
                    let expected = if inside {
                        &complete[offset..offset + 3]
                    } else {
                        &[0, 0, 0]
                    };
                    assert_eq!(
                        &clipped[offset..offset + 3],
                        expected,
                        "area={area:?}, clip={clip:?}, pixel=({x}, {y})"
                    );
                }
            }
        }
    }
}
