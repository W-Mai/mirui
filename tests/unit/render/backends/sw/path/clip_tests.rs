use super::*;
use crate::render::raster::{LineCap, LineJoin};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Point, Viewport};

#[test]
fn clipped_path_writes_match_checked_pixels_for_every_target_layout() {
    use crate::render::scene::{GradientStop, GradientUnits, RadialGradient, SpreadMode};
    use crate::render::texture::AlphaMode;
    use alloc::borrow::Cow;

    let width = 11_u16;
    let height = 8_u16;
    let path = Path::rect((-4).into(), (-3).into(), 21.into(), 18.into());
    for format in [
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
        ColorFormat::RGB888,
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
    ] {
        let stride = usize::from(width) * format.bytes_per_pixel() + 5;
        let len = stride * usize::from(height);
        for mode in [AlphaMode::Opaque, AlphaMode::Blend] {
            for alpha in [0, 1, 127, 255] {
                let color = Color::rgba(177, 45, 99, alpha);
                let paint = Paint::RadialGradient(RadialGradient {
                    center: Point::new(5, 4).into(),
                    focal: Point::new(5, 4).into(),
                    radius: Fixed::from_int(4).into(),
                    focal_radius: Fixed::ZERO.into(),
                    stops: Cow::Owned(alloc::vec![
                        GradientStop {
                            offset: Fixed::ZERO.into(),
                            color: color.into()
                        },
                        GradientStop {
                            offset: Fixed::ONE.into(),
                            color: color.into()
                        },
                    ]),
                    spread: SpreadMode::Pad,
                    units: GradientUnits::UserSpaceOnUse,
                    transform: Transform::IDENTITY.into(),
                });
                for clip in [
                    Rect::new(0, 0, width, height),
                    Rect::new(2, 1, 7, 5),
                    Rect::new(-4, -4, 7, 6),
                    Rect::new(20, 20, 3, 3),
                ] {
                    for route in 0..3 {
                        let mut actual = alloc::vec![0xA7_u8; 9 + len + 8];
                        let mut expected = actual.clone();
                        {
                            let mut target =
                                Texture::new(&mut actual[9..9 + len], width, height, format)
                                    .unwrap();
                            target.stride = stride;
                            target.alpha_mode = mode;
                            let mut reference =
                                Texture::new(&mut expected[9..9 + len], width, height, format)
                                    .unwrap();
                            reference.stride = stride;
                            reference.alpha_mode = mode;
                            let mut renderer = SwRenderer::new(target);
                            let mut draw = || match route {
                                0 => renderer.fill_path_transformed(
                                    &path,
                                    clip,
                                    &Transform::IDENTITY,
                                    &paint,
                                    255,
                                    FillRule::NonZero,
                                ),
                                1 => renderer.fill_physical_path(
                                    &path,
                                    &clip,
                                    &paint,
                                    255,
                                    &Transform::IDENTITY,
                                    path.bbox().unwrap(),
                                ),
                                _ => renderer
                                    .fill_path_projective(
                                        &path,
                                        &Transform3D::IDENTITY,
                                        clip,
                                        &paint,
                                        255,
                                        FillRule::NonZero,
                                    )
                                    .unwrap(),
                            };
                            draw();
                            assert_eq!(crate::test_support::tracked_allocations(draw), 0);
                            let mut pixels = reference.pixels_mut().unwrap();
                            let (x0, y0, x1, y1) = clip.pixel_bounds();
                            for _ in 0..2 {
                                for y in 0..i32::from(height) {
                                    for x in 0..i32::from(width) {
                                        if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                                            pixels.blend_pixel_int(x, y, &color, alpha);
                                        }
                                    }
                                }
                            }
                        }
                        assert_eq!(
                            actual, expected,
                            "{format:?} {mode:?} {alpha} {clip:?} {route}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn transformed_stroke_uses_physical_clip_once_at_hidpi_scale() {
    let mut renderer = SwRenderer::new(
        Texture::owned(64, 64, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    renderer.viewport = Viewport::new(64, 64, Fixed::from_int(2));
    let mut path = Path::new();
    path.move_to(Point::new(0, 20)).line_to(Point::new(40, 20));
    let paint = Paint::Color(Color::rgb(255, 0, 0).into());
    renderer.stroke_path_transformed(
        &path,
        Rect::new(0, 0, 16, 64),
        &Transform::IDENTITY,
        Fixed::from_int(2),
        &paint,
        255,
        LineCap::Butt,
        LineJoin::Miter,
        Fixed::from_int(4),
        &[],
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(10, 20).r,
        255
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(24, 20).r,
        0
    );
}
