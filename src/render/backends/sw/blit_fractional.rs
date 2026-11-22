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
    if radius == Fixed::ZERO
        && composite == CompositeMode::SourceOver
        && mask.is_none()
        && !(opacity == 255
            && area.w == Fixed::from_int(source_rect.2)
            && area.h == Fixed::from_int(source_rect.3))
        && super::blit_rgba::try_fractional(target, source, source_rect, area, clip, opacity)
    {
        return;
    }
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
        let (left_end, right_start, right_axis) = match &interior {
            Some((left, top, right, bottom, right_axis)) if y >= *top && y < *bottom => {
                (*left, *right, right_axis)
            }
            _ => (x1, x1, &x_axis),
        };
        let mut axis = x_axis;
        for x in (x0..left_end).chain(right_start..x1) {
            if x == right_start {
                axis = *right_axis;
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
#[path = "../../../../tests/unit/render/backends/sw/blit_fractional.rs"]
mod tests;
