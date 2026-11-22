use super::*;
use crate::render::texture::{AlphaMode, ColorFormat, Texture};
use crate::types::{PhysicalRect, Viewport};

#[test]
fn square_borders_match_outer_minus_inner_coverage_without_overlapping_blends() {
    for format in [
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ] {
        for alpha_mode in [AlphaMode::Opaque, AlphaMode::Blend] {
            for scale in [Fixed::ONE, Fixed::from_ratio(3, 2)] {
                for origin in [Fixed::from_ratio(-1, 4), Fixed::ZERO, Fixed::HALF] {
                    for width in [Fixed::ZERO, Fixed::HALF, Fixed::ONE, Fixed::from_int(8)] {
                        for (alpha, opa) in [(255, 255), (117, 193), (255, 0)] {
                            for clip in [
                                Rect::new(0, 0, 12, 11),
                                Rect::new(2, 1, 3, 8),
                                Rect::new(7, 2, 5, 4),
                            ] {
                                let mut actual =
                                    alloc::vec![17; 12 * 11 * format.bytes_per_pixel()];
                                let mut expected = actual.clone();
                                let mut target = Texture::new(&mut actual, 12, 11, format).unwrap();
                                target.alpha_mode = alpha_mode;
                                let mut reference =
                                    Texture::new(&mut expected, 12, 11, format).unwrap();
                                reference.alpha_mode = alpha_mode;
                                let mut renderer = SwRenderer::new(target);
                                renderer.viewport = Viewport::new(12, 11, scale);
                                renderer.physical_clip = PhysicalRect::new(1, 0, 10, 10);
                                let area = Rect::new(origin, origin, Fixed::from_ratio(23, 4), 7);
                                let color = Color::rgba(231, 123, 45, alpha);
                                let allocations = crate::test_support::tracked_allocations(|| {
                                    renderer.stroke_rect_inner(
                                        &area,
                                        &clip,
                                        width,
                                        &color,
                                        Fixed::ZERO,
                                        opa,
                                    );
                                });
                                assert_eq!(allocations, 0);
                                let area = renderer.viewport.rect_to_physical(area);
                                let clip = renderer.physical_draw_clip(clip);
                                let width = width * scale;
                                let inner_w = (area.w - width * 2).max(Fixed::ZERO);
                                let inner_h = (area.h - width * 2).max(Fixed::ZERO);
                                let opacity = Fixed::from_int(opa as i32)
                                    .map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
                                if let Some(draw) = area
                                    .intersect(&clip)
                                    .and_then(|draw| draw.intersect(&Rect::new(0, 0, 12, 11)))
                                {
                                    let (x0, y0, x1, y1) = draw.pixel_bounds();
                                    let mut pixels = reference.pixels_mut().unwrap();
                                    for y in y0..y1 {
                                        for x in x0..x1 {
                                            let overlap =
                                                |start: Fixed, extent: Fixed, pixel: i32| {
                                                    ((start + extent)
                                                        .min(Fixed::from_int(pixel + 1))
                                                        - start.max(Fixed::from_int(pixel)))
                                                    .max(Fixed::ZERO)
                                                };
                                            let outer = overlap(area.x, area.w, x)
                                                * overlap(area.y, area.h, y);
                                            let inner = overlap(area.x + width, inner_w, x)
                                                * overlap(area.y + width, inner_h, y);
                                            let coverage = (outer - inner).max(Fixed::ZERO);
                                            let alpha =
                                                (coverage * opacity).map01(255).to_int() as u8;
                                            pixels.blend_pixel(
                                                Fixed::from_int(x),
                                                Fixed::from_int(y),
                                                &color,
                                                alpha,
                                            );
                                        }
                                    }
                                }
                                assert_eq!(
                                    renderer.target.read_pixels().unwrap().bytes(),
                                    reference.read_pixels().unwrap().bytes(),
                                    "format={format:?}, alpha={alpha_mode:?}, scale={scale:?}, origin={origin:?}, width={width:?}, clip={clip:?}, opacity={opa}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Rounded borders skip the pixels inside their inner rect; the result
/// must match evaluating the ring formula at every pixel of the area.
#[test]
fn rounded_borders_match_the_per_pixel_ring_formula() {
    let (width, height) = (40_u16, 30_u16);
    for origin in [Fixed::ZERO, Fixed::from_ratio(1, 3), Fixed::HALF] {
        for (w, h) in [
            (Fixed::from_int(34), Fixed::from_int(24)),
            (Fixed::from_ratio(77, 4), Fixed::from_int(9)),
        ] {
            for radius in [
                Fixed::ONE,
                Fixed::from_int(4),
                Fixed::from_ratio(19, 2),
                Fixed::from_int(30),
            ] {
                for border in [
                    Fixed::HALF,
                    Fixed::ONE,
                    Fixed::from_ratio(5, 2),
                    Fixed::from_int(6),
                ] {
                    let mut actual = alloc::vec![9; usize::from(width) * usize::from(height) * 4];
                    let mut expected = actual.clone();
                    let area = Rect::new(
                        origin + Fixed::from_int(2),
                        origin + Fixed::from_int(3),
                        w,
                        h,
                    );
                    let clip = Rect::new(0, 0, i32::from(width), i32::from(height));
                    let color = Color::rgba(200, 120, 40, 230);
                    let target =
                        Texture::new(&mut actual, width, height, ColorFormat::RGBA8888).unwrap();
                    let mut renderer = SwRenderer::new(target);
                    renderer.stroke_rect_inner(&area, &clip, border, &color, radius, 200);

                    let mut reference =
                        Texture::new(&mut expected, width, height, ColorFormat::RGBA8888).unwrap();
                    let r = radius.min(area.w / 2).min(area.h / 2);
                    let inner_r = (r - border).max(Fixed::ZERO);
                    let inner_w = (area.w - border * 2).max(Fixed::ZERO);
                    let inner_h = (area.h - border * 2).max(Fixed::ZERO);
                    let inner = Rect::new(area.x + border, area.y + border, inner_w, inner_h);
                    let opacity =
                        Fixed::from_int(200).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
                    let (x0, y0, x1, y1) = area.intersect(&clip).unwrap().pixel_bounds();
                    let mut pixels = reference.pixels_mut().unwrap();
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let rel_x = Fixed::from_int(x) - area.x;
                            let rel_y = Fixed::from_int(y) - area.y;
                            let outer = rect_pixel_coverage(area, x, y)
                                * rounded_rect_coverage(rel_x, rel_y, area.w, area.h, r);
                            let inside = rect_pixel_coverage(inner, x, y)
                                * rounded_rect_coverage(
                                    rel_x - border,
                                    rel_y - border,
                                    inner_w,
                                    inner_h,
                                    inner_r,
                                );
                            let alpha = ((outer - inside).max(Fixed::ZERO) * opacity)
                                .map01(255)
                                .to_int() as u8;
                            if alpha > 0 {
                                pixels.blend_pixel(
                                    Fixed::from_int(x),
                                    Fixed::from_int(y),
                                    &color,
                                    alpha,
                                );
                            }
                        }
                    }
                    drop(pixels);
                    assert_eq!(
                        renderer.target.read_pixels().unwrap().bytes(),
                        reference.read_pixels().unwrap().bytes(),
                        "origin={origin:?} size={w:?}x{h:?} radius={radius:?} border={border:?}"
                    );
                }
            }
        }
    }
}
