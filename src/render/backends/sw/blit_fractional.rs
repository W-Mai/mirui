use super::rect_fill::{pixel_axis_coverage, rounded_rect_coverage};
use crate::render::command::CompositeMode;
use crate::render::sampling::SampleAxis;
use crate::render::texture::{TexturePixels, TexturePixelsMut};
use crate::types::{Fixed, Rect};

#[allow(clippy::too_many_arguments)]
pub(super) fn blit_fractional(
    target: &mut TexturePixelsMut<'_>,
    source: &TexturePixels<'_>,
    source_rect: (i32, i32, i32, i32),
    area: Rect,
    clip: Rect,
    opacity: u8,
    radius: Fixed,
    composite: CompositeMode,
    mask: Option<&[u8]>,
) {
    let Some(draw) = area
        .intersect(&clip)
        .and_then(|draw| draw.intersect(&Rect::new(0, 0, target.width(), target.height())))
    else {
        return;
    };
    let (x0, y0, x1, y1) = draw.pixel_bounds();
    let (source_x, source_y, width, height) = source_rect;
    let x_axis = SampleAxis::new(x0, area.x, area.w, width);
    let mut y_axis = SampleAxis::new(y0, area.y, area.h, height);
    let radius = radius.max(Fixed::ZERO).min(area.w / 2).min(area.h / 2);
    let interior = if area.w == Fixed::from_int(width)
        && area.h == Fixed::from_int(height)
        && radius == Fixed::ZERO
        && opacity == 255
        && composite == CompositeMode::SourceOver
        && mask.is_none()
    {
        let left = area.x.ceil().to_int().clamp(x0, x1);
        let top = area.y.ceil().to_int().clamp(y0, y1);
        let right = (area.x + area.w).floor().to_int().clamp(x0, x1);
        let bottom = (area.y + area.h).floor().to_int().clamp(y0, y1);
        if left < right && top < bottom {
            super::blit_fast::blit_1to1_fast(
                target,
                source,
                source_x,
                source_y,
                width as u16,
                height as u16,
                (area.x - Fixed::HALF).ceil().to_int(),
                (area.y - Fixed::HALF).ceil().to_int(),
                left,
                top,
                right,
                bottom,
            );
            Some((
                left,
                top,
                right,
                bottom,
                SampleAxis::new(right, area.x, area.w, width),
            ))
        } else {
            None
        }
    } else {
        None
    };
    let right = area.x + area.w;
    let bottom = area.y + area.h;
    for y in y0..y1 {
        let sy = source_y + y_axis.next(height);
        let coverage_y = pixel_axis_coverage(area.y, bottom, y);
        let (left_end, right_start, right_axis) = match interior {
            Some((left, top, right, bottom, right_axis)) if y >= top && y < bottom => {
                (left, right, right_axis)
            }
            _ => (x1, x1, x_axis),
        };
        let mut axis = x_axis;
        for x in (x0..left_end).chain(right_start..x1) {
            if x == right_start {
                axis = right_axis;
            }
            let sx = source_x + axis.next(width);
            let mut coverage = pixel_axis_coverage(area.x, right, x) * coverage_y;
            if radius != Fixed::ZERO {
                coverage = coverage
                    * rounded_rect_coverage(
                        Fixed::from_int(x) - area.x,
                        Fixed::from_int(y) - area.y,
                        area.w,
                        area.h,
                        radius,
                    );
            }
            let color = source.get_pixel(sx, sy);
            let alpha = ((u16::from(color.a) * u16::from(opacity) + 127) / 255) as u8;
            let mut alpha = (Fixed::from_int(i32::from(alpha)) * coverage).to_int() as u8;
            if let Some(mask) = mask {
                alpha = ((u16::from(alpha)
                    * u16::from(mask[y as usize * target.width() as usize + x as usize])
                    + 127)
                    / 255) as u8;
            }
            if alpha != 0 {
                target.composite_pixel_int(x, y, &color, alpha, composite);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::backends::sw::SwRenderer;
    use crate::render::texture::{AlphaMode, ColorFormat, Texture};
    use crate::types::{Color, Point};

    #[test]
    fn one_to_one_interiors_match_masked_sampling_across_pixel_formats() {
        let mut bytes = [0u8; 4 * 3 * 4];
        for (i, pixel) in bytes.chunks_exact_mut(4).enumerate() {
            pixel.copy_from_slice(&[213, (i * 17) as u8, 31, [0, 137, 255][i % 3]]);
        }
        let source = Texture::from_ref(&bytes, 4, 3, ColorFormat::RGBA8888).unwrap();
        let source = source.read_pixels().unwrap();
        let mask = [255u8; 64];
        for format in [
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
            ColorFormat::RGB888,
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
        ] {
            for alpha_mode in [AlphaMode::Opaque, AlphaMode::Blend] {
                for origin in [-193, 64, 128, 193, 1279] {
                    for clip in [Rect::new(0, 0, 8, 8), Rect::new(1, 2, 2, 3)] {
                        let area = Rect::new(
                            Fixed::from_ratio(origin, 256),
                            Fixed::from_ratio(origin + 64, 256),
                            4,
                            3,
                        );
                        let mut actual = alloc::vec![61; 64 * format.bytes_per_pixel()];
                        let mut expected = actual.clone();
                        let mut target = Texture::new(&mut actual, 8, 8, format).unwrap();
                        target.alpha_mode = alpha_mode;
                        let mut reference = Texture::new(&mut expected, 8, 8, format).unwrap();
                        reference.alpha_mode = alpha_mode;
                        let allocations = crate::test_support::tracked_allocations(|| {
                            blit_fractional(
                                &mut target.pixels_mut().unwrap(),
                                &source,
                                (0, 0, 4, 3),
                                area,
                                clip,
                                255,
                                Fixed::ZERO,
                                CompositeMode::SourceOver,
                                None,
                            );
                        });
                        assert_eq!(allocations, 0);
                        blit_fractional(
                            &mut reference.pixels_mut().unwrap(),
                            &source,
                            (0, 0, 4, 3),
                            area,
                            clip,
                            255,
                            Fixed::ZERO,
                            CompositeMode::SourceOver,
                            Some(&mask),
                        );
                        assert_eq!(
                            actual, expected,
                            "{format:?}, {alpha_mode:?}, {origin}, {clip:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn fractional_edges_match_quad_blits_with_opacity_masks_and_compositing() {
        let source_bytes = [201, 103, 47, 171].repeat(16);
        let source = Texture::from_ref(&source_bytes, 4, 4, ColorFormat::RGBA8888).unwrap();
        let mask: [u8; 64] = core::array::from_fn(|i| (i * 17) as u8);
        for opacity in [255, 113] {
            for radius in [Fixed::ZERO, Fixed::ONE] {
                for composite in [CompositeMode::SourceOver, CompositeMode::Difference] {
                    for mask in [None, Some(mask.as_slice())] {
                        let area = Rect::new(Fixed::HALF, Fixed::from_ratio(5, 4), 4, 4);
                        let mut actual = [23, 41, 61, 137].repeat(64);
                        let mut expected = actual.clone();
                        let mut target =
                            Texture::new(&mut actual, 8, 8, ColorFormat::RGBA8888).unwrap();
                        target.alpha_mode = AlphaMode::Blend;
                        let mut reference =
                            Texture::new(&mut expected, 8, 8, ColorFormat::RGBA8888).unwrap();
                        reference.alpha_mode = AlphaMode::Blend;
                        blit_fractional(
                            &mut target.pixels_mut().unwrap(),
                            &source.read_pixels().unwrap(),
                            (0, 0, 4, 4),
                            area,
                            Rect::new(0, 0, 8, 8),
                            opacity,
                            radius,
                            composite,
                            mask,
                        );
                        let quad = [
                            Point::new(area.x, area.y),
                            Point::new(area.x + area.w, area.y),
                            Point::new(area.x + area.w, area.y + area.h),
                            Point::new(area.x, area.y + area.h),
                        ];
                        super::super::quad::blit_quad(
                            &mut reference.pixels_mut().unwrap(),
                            &source.read_pixels().unwrap(),
                            &quad,
                            Rect::new(0, 0, 8, 8),
                            Point::new(4, 4),
                            radius,
                            opacity,
                            composite,
                            mask,
                        );
                        assert_eq!(actual, expected);
                    }
                }
            }
        }
    }

    #[test]
    fn sampling_is_exact_and_independent_of_the_clipped_start() {
        let mut large =
            SampleAxis::new(65500, Fixed::ZERO, Fixed::from_ratio(16776961, 256), 65535);
        assert_eq!(large.next(65535), 65500);
        for origin in [i32::MIN, -193, -128, -1, 0, 1, 127, 193, i32::MAX] {
            for extent in [1, 63, 128, 257, 1023, 4096, 65537, 16776961, i32::MAX] {
                for samples in [1, 3, 16, 257, 65535] {
                    for start in [0, 1, 3, 17, 32760, 65500] {
                        let mut axis = SampleAxis::new(
                            start,
                            Fixed::from_ratio(origin, 256),
                            Fixed::from_ratio(extent, 256),
                            samples,
                        );
                        for pixel in start..start + 33 {
                            let numerator = (i64::from(pixel) * 256 + 128 - i64::from(origin))
                                * i64::from(samples);
                            let expected =
                                (numerator / i64::from(extent)).clamp(0, i64::from(samples - 1));
                            assert_eq!(i64::from(axis.next(samples)), expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fractional_blits_preserve_source_subrect_and_partial_frame_pixels() {
        let mut bytes = [0u8; 4 * 4 * 4];
        for (index, pixel) in bytes.chunks_exact_mut(4).enumerate() {
            pixel.copy_from_slice(&[index as u8 * 13, 193, 67, 255]);
        }
        let source = Texture::from_ref(&bytes, 4, 4, ColorFormat::RGBA8888).unwrap();
        for origin in [-128, 64, 128, 193] {
            for extent in [63, 128, 257, 768, 1023] {
                for format in [ColorFormat::RGBA8888, ColorFormat::RGB565Swapped] {
                    for opacity in [255, 117] {
                        let pos = Point::new(
                            Fixed::from_ratio(origin, 256),
                            Fixed::from_ratio(origin + 128, 256),
                        );
                        let size = Point::new(
                            Fixed::from_ratio(extent, 256),
                            Fixed::from_ratio(extent + 128, 256),
                        );
                        let mut whole = alloc::vec![0u8; 8 * 8 * format.bytes_per_pixel()];
                        let mut partial = whole.clone();
                        let mut reference = whole.clone();
                        let mut target = Texture::new(&mut whole, 8, 8, format).unwrap();
                        target.alpha_mode = AlphaMode::Blend;
                        let mut renderer = SwRenderer::new(target);
                        let allocations = crate::test_support::tracked_allocations(|| {
                            renderer.blit_inner(
                                &source,
                                &Rect::new(1, 1, 2, 2),
                                pos,
                                size,
                                &Rect::new(0, 0, 8, 8),
                                opacity,
                                Fixed::ZERO,
                                CompositeMode::SourceOver,
                            );
                        });
                        assert_eq!(allocations, 0);
                        let mut target = Texture::new(&mut partial, 8, 8, format).unwrap();
                        target.alpha_mode = AlphaMode::Blend;
                        let mut renderer = SwRenderer::new(target);
                        for y in 0..8 {
                            for x in 0..8 {
                                renderer.blit_inner(
                                    &source,
                                    &Rect::new(1, 1, 2, 2),
                                    pos,
                                    size,
                                    &Rect::new(x, y, 1, 1),
                                    opacity,
                                    Fixed::ZERO,
                                    CompositeMode::SourceOver,
                                );
                            }
                        }
                        let mut target = Texture::new(&mut reference, 8, 8, format).unwrap();
                        target.alpha_mode = AlphaMode::Blend;
                        let mut target = target.pixels_mut().unwrap();
                        for y in 0..8 {
                            for x in 0..8 {
                                let overlap = |pixel: i32, start: i32, len: i32| {
                                    ((pixel * 256 + 256).min(start + len)
                                        - (pixel * 256).max(start))
                                    .max(0)
                                };
                                let coverage = overlap(x, origin, extent)
                                    * overlap(y, origin + 128, extent + 128)
                                    / 256;
                                let sx = ((x * 256 + 128 - origin) * 2 / extent).clamp(0, 1) + 1;
                                let sy = ((y * 256 - origin) * 2 / (extent + 128)).clamp(0, 1) + 1;
                                let color = Color::rgba(((sy * 4 + sx) * 13) as u8, 193, 67, 255);
                                let alpha = (i32::from(opacity) * coverage / 256) as u8;
                                if alpha != 0 {
                                    target.composite_pixel_int(
                                        x,
                                        y,
                                        &color,
                                        alpha,
                                        CompositeMode::SourceOver,
                                    );
                                }
                            }
                        }
                        assert_eq!(
                            whole, reference,
                            "origin={origin}, extent={extent}, format={format:?}"
                        );
                        assert_eq!(whole, partial, "clipping must not change source sampling");
                    }
                }
            }
        }
    }
}
