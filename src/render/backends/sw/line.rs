use super::SwRenderer;
use crate::types::{Color, Fixed, Fixed64, Point, Rect};

impl SwRenderer<'_> {
    pub(super) fn try_aligned_line(
        &mut self,
        p1: Point,
        p2: Point,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opacity: u8,
    ) -> bool {
        if width <= Fixed::ZERO || p1 == p2 || !self.scratch.clip_stack.is_empty() {
            return false;
        }
        let half = Fixed64::from_fixed(width / 2);
        let (x, y, w, h) = if p1.x == p2.x {
            (
                Fixed64::from_fixed(p1.x) - half,
                Fixed64::from_fixed(p1.y.min(p2.y)),
                Fixed64::from_fixed(width),
                Fixed64::from_fixed(p1.y.max(p2.y)) - Fixed64::from_fixed(p1.y.min(p2.y)),
            )
        } else if p1.y == p2.y {
            (
                Fixed64::from_fixed(p1.x.min(p2.x)),
                Fixed64::from_fixed(p1.y) - half,
                Fixed64::from_fixed(p1.x.max(p2.x)) - Fixed64::from_fixed(p1.x.min(p2.x)),
                Fixed64::from_fixed(width),
            )
        } else {
            return false;
        };
        let fixed = |value: Fixed64| {
            (value >= Fixed64::from_fixed(Fixed::MIN) && value <= Fixed64::from_fixed(Fixed::MAX))
                .then(|| value.to_fixed())
        };
        let (Some(x), Some(y), Some(w), Some(h)) = (fixed(x), fixed(y), fixed(w), fixed(h)) else {
            return false;
        };
        let area = Rect::new(x, y, w, h);
        if !self.viewport.rect_to_physical(area).is_aligned() {
            return false;
        }
        let alpha = super::path::solid_alpha(opacity, color.a)
            .map01(255)
            .to_int() as u8;
        self.fill_rect_inner(&area, clip, &Color { a: 255, ..*color }, Fixed::ZERO, alpha);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{
        canvas::{Canvas, Paint},
        path::PathCmd,
        raster::{LineCap, LineJoin},
        texture::{AlphaMode, ColorFormat, Texture},
    };
    use crate::types::Viewport;

    #[test]
    fn aligned_lines_match_stroke_rasterization_across_alpha_clips_and_scales() {
        for format in [
            ColorFormat::RGBA8888,
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
        ] {
            for mode in [AlphaMode::Opaque, AlphaMode::Blend] {
                for scale in [Fixed::ONE, Fixed::from_ratio(3, 2), Fixed::from_int(2)] {
                    for width in [
                        Fixed::ZERO,
                        Fixed::ONE,
                        Fixed::from_int(2),
                        Fixed::from_int(3),
                        Fixed::from_ratio(3, 2),
                    ] {
                        for (p1, p2) in [
                            (Point::new(2, 8), Point::new(18, 8)),
                            (Point::new(18, 8), Point::new(2, 8)),
                            (Point::new(8, 2), Point::new(8, 18)),
                            (Point::new(8, 18), Point::new(8, 2)),
                            (Point::new(-2, 8), Point::new(18, 8)),
                            (Point::new(8, 8), Point::new(8, 8)),
                            (
                                Point::new(Fixed::from_ratio(17, 2), 2),
                                Point::new(Fixed::from_ratio(17, 2), 18),
                            ),
                            (Point::new(2, 2), Point::new(18, 18)),
                        ] {
                            for alpha in [0, 1, 127, 255] {
                                for opacity in [0, 1, 127, 180, 255] {
                                    let mut actual =
                                        SwRenderer::new(Texture::owned(24, 24, format).unwrap());
                                    let mut expected =
                                        SwRenderer::new(Texture::owned(24, 24, format).unwrap());
                                    actual.viewport = Viewport::new(24, 24, scale);
                                    expected.viewport = actual.viewport;
                                    actual.target.alpha_mode = mode;
                                    expected.target.alpha_mode = mode;
                                    for renderer in [&mut actual, &mut expected] {
                                        for (i, byte) in renderer
                                            .target
                                            .pixels_mut()
                                            .unwrap()
                                            .bytes_mut()
                                            .iter_mut()
                                            .enumerate()
                                        {
                                            *byte = (i as u8).wrapping_mul(73).wrapping_add(11);
                                        }
                                    }
                                    let clip = Rect::new(1, 3, 16, 14);
                                    let color = Color::rgba(37, 113, 219, alpha);
                                    actual.draw_line(p1, p2, &clip, width, &color, opacity);
                                    expected.stroke_commands_inner(
                                        &[PathCmd::MoveTo(p1), PathCmd::LineTo(p2)],
                                        &clip,
                                        width,
                                        &Paint::Color(color.into()),
                                        opacity,
                                        LineCap::Butt,
                                        LineJoin::Miter,
                                        Fixed::from_int(4),
                                        &[],
                                    );
                                    assert!(
                                        actual.target.read_pixels().unwrap().bytes()
                                            == expected.target.read_pixels().unwrap().bytes(),
                                        "{format:?} {mode:?} {scale:?} {width:?} {p1:?} {p2:?} {alpha} {opacity}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn aligned_lines_do_not_allocate_stroke_workspaces() {
        let mut renderer = SwRenderer::new(Texture::owned(24, 24, ColorFormat::RGBA8888).unwrap());
        let draw = || {
            renderer.draw_line(
                Point::new(2, 8),
                Point::new(18, 8),
                &Rect::new(0, 0, 24, 24),
                2.into(),
                &Color::rgb(35, 77, 219),
                180,
            )
        };
        assert_eq!(crate::test_support::tracked_allocations(draw), 0);
        assert_eq!(renderer.scratch.stroke_outline.command_capacity(), 0);
    }

    #[test]
    fn path_masked_lines_keep_stroke_coverage_and_clip_rounding() {
        let mut actual = SwRenderer::new(Texture::owned(24, 24, ColorFormat::RGBA8888).unwrap());
        let mut expected = SwRenderer::new(Texture::owned(24, 24, ColorFormat::RGBA8888).unwrap());
        for renderer in [&mut actual, &mut expected] {
            renderer.target.pixels_mut().unwrap().bytes_mut().fill(73);
            renderer.scratch.clip_stack.push(super::super::ClipMask {
                alpha: (0..24 * 24).map(|i| (i as u8).wrapping_mul(31)).collect(),
            });
        }
        let p1 = Point::new(2, 8);
        let p2 = Point::new(18, 8);
        let clip = Rect::new(0, 0, 24, 24);
        let color = Color::rgba(37, 113, 219, 127);
        assert!(!actual.try_aligned_line(p1, p2, &clip, 2.into(), &color, 180));
        actual.draw_line(p1, p2, &clip, 2.into(), &color, 180);
        expected.stroke_commands_inner(
            &[PathCmd::MoveTo(p1), PathCmd::LineTo(p2)],
            &clip,
            2.into(),
            &Paint::Color(color.into()),
            180,
            LineCap::Butt,
            LineJoin::Miter,
            Fixed::from_int(4),
            &[],
        );
        assert_eq!(
            actual.target.read_pixels().unwrap().bytes(),
            expected.target.read_pixels().unwrap().bytes()
        );
    }
}
