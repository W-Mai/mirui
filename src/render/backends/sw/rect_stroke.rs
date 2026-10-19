use super::SwRenderer;
use super::rect_fill::{fill_axis_aligned, rounded_rect_coverage};
use crate::types::{Color, Fixed, Rect};

impl SwRenderer<'_> {
    pub(super) fn stroke_rect_inner(
        &mut self,
        area: &Rect,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        let phys_area = self.viewport.rect_to_physical(*area);
        let phys_clip = self.physical_draw_clip(*clip);
        let width = width * self.viewport.scale();
        let radius = radius * self.viewport.scale();
        let area = &phys_area;
        let clip = &phys_clip;

        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let r = radius.min(area.w / 2).min(area.h / 2);
        let bw = width;
        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));

        let inner_r = (r - bw).max(Fixed::ZERO);
        let inner_w = (area.w - bw * 2).max(Fixed::ZERO);
        let inner_h = (area.h - bw * 2).max(Fixed::ZERO);

        let clip_mask = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let target_w = self.target.width as usize;

        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        if r == Fixed::ZERO && clip_mask.is_none() {
            let base_opa = opa_norm.map01(255).to_int() as u8;
            let effective_opa = ((u16::from(color.a) * u16::from(base_opa)) / 255) as u8;
            let x0 = (area.x + bw).ceil().to_int().clamp(px_x0, px_x1);
            let y0 = (area.y + bw).ceil().to_int().clamp(px_y0, px_y1);
            let x1 = (area.x + bw + inner_w).ceil().to_int().clamp(px_x0, px_x1);
            let y1 = (area.y + bw + inner_h).ceil().to_int().clamp(px_y0, px_y1);
            if x0 >= x1 || y0 >= y1 {
                fill_axis_aligned(
                    &mut target,
                    px_x0,
                    px_y0,
                    px_x1,
                    px_y1,
                    color,
                    effective_opa,
                );
            } else {
                for (left, top, right, bottom) in [
                    (px_x0, px_y0, px_x1, y0),
                    (px_x0, y1, px_x1, px_y1),
                    (px_x0, y0, x0, y1),
                    (x1, y0, px_x1, y1),
                ] {
                    fill_axis_aligned(&mut target, left, top, right, bottom, color, effective_opa);
                }
            }
            return;
        }
        for py in px_y0..px_y1 {
            let row_mask_off = py as usize * target_w;
            for px in px_x0..px_x1 {
                let rel_x = Fixed::from_int(px) - area.x;
                let rel_y = Fixed::from_int(py) - area.y;

                let outer_cov = rounded_rect_coverage(rel_x, rel_y, area.w, area.h, r);
                if outer_cov == Fixed::ZERO {
                    continue;
                }

                let inner_rel_x = rel_x - bw;
                let inner_rel_y = rel_y - bw;
                let inner_cov = if inner_rel_x >= Fixed::ZERO
                    && inner_rel_y >= Fixed::ZERO
                    && inner_rel_x < inner_w
                    && inner_rel_y < inner_h
                {
                    rounded_rect_coverage(inner_rel_x, inner_rel_y, inner_w, inner_h, inner_r)
                } else {
                    Fixed::ZERO
                };

                let border_cov = (outer_cov - inner_cov).max(Fixed::ZERO);
                let base_opa = (border_cov * opa_norm).map01(255).to_int() as u8;
                if base_opa == 0 {
                    continue;
                }
                let final_opa = match clip_mask {
                    Some(m) => {
                        let clip_alpha = m[row_mask_off + px as usize];
                        if clip_alpha == 0 {
                            continue;
                        }
                        ((base_opa as u16 * clip_alpha as u16 + 127) / 255) as u8
                    }
                    None => base_opa,
                };
                if final_opa > 0 {
                    target.blend_pixel(Fixed::from_int(px), Fixed::from_int(py), color, final_opa);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::texture::{AlphaMode, ColorFormat, Texture};
    use crate::types::{PhysicalRect, Viewport};

    #[test]
    fn square_border_bands_match_pixel_sampling_without_overlapping_blends() {
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
                                    let mut target =
                                        Texture::new(&mut actual, 12, 11, format).unwrap();
                                    target.alpha_mode = alpha_mode;
                                    let mut reference =
                                        Texture::new(&mut expected, 12, 11, format).unwrap();
                                    reference.alpha_mode = alpha_mode;
                                    let mut renderer = SwRenderer::new(target);
                                    renderer.viewport = Viewport::new(12, 11, scale);
                                    renderer.physical_clip = PhysicalRect::new(1, 0, 10, 10);
                                    let area =
                                        Rect::new(origin, origin, Fixed::from_ratio(23, 4), 7);
                                    let color = Color::rgba(231, 123, 45, alpha);
                                    let allocations =
                                        crate::test_support::tracked_allocations(|| {
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
                                    let quantized_opa = Fixed::from_int(opa as i32)
                                        .map_range((0, 255), (Fixed::ZERO, Fixed::ONE))
                                        .map01(255)
                                        .to_int()
                                        as u8;
                                    if let Some(draw) = area
                                        .intersect(&clip)
                                        .and_then(|draw| draw.intersect(&Rect::new(0, 0, 12, 11)))
                                    {
                                        let (x0, y0, x1, y1) = draw.pixel_bounds();
                                        let mut pixels = reference.pixels_mut().unwrap();
                                        for y in y0..y1 {
                                            for x in x0..x1 {
                                                let inner_x = Fixed::from_int(x) - area.x - width;
                                                let inner_y = Fixed::from_int(y) - area.y - width;
                                                if inner_x >= Fixed::ZERO
                                                    && inner_x < inner_w
                                                    && inner_y >= Fixed::ZERO
                                                    && inner_y < inner_h
                                                {
                                                    continue;
                                                }
                                                pixels.blend_pixel(
                                                    Fixed::from_int(x),
                                                    Fixed::from_int(y),
                                                    &color,
                                                    quantized_opa,
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
}
