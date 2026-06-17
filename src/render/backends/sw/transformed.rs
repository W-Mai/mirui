use crate::render::texture::Texture;
use crate::types::{Color, Fixed, Point, Rect, Transform};

pub fn fill_rect_transformed(
    dst: &mut Texture,
    phys_rect: Rect,
    phys_clip: Rect,
    tf: &Transform,
    color: &Color,
    opa: u8,
) {
    let Some(inv) = tf.inverse() else { return };
    let bbox = tf.apply_rect_bbox(phys_rect);
    let Some(draw_area) = bbox.intersect(&phys_clip) else {
        return;
    };
    let screen = Rect::new(0, 0, dst.width, dst.height);
    let Some(draw_area) = draw_area.intersect(&screen) else {
        return;
    };
    let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
    let rx0 = phys_rect.x;
    let ry0 = phys_rect.y;
    let rx1 = phys_rect.x + phys_rect.w;
    let ry1 = phys_rect.y + phys_rect.h;
    for py in px_y0..px_y1 {
        for px in px_x0..px_x1 {
            let sample = inv.apply_point(Point {
                x: Fixed::from_int(px) + Fixed::HALF,
                y: Fixed::from_int(py) + Fixed::HALF,
            });
            if sample.x < rx0 || sample.x >= rx1 || sample.y < ry0 || sample.y >= ry1 {
                continue;
            }
            if opa == 255 {
                dst.set_pixel(px, py, color);
            } else {
                dst.blend_pixel_int(px, py, color, opa);
            }
        }
    }
}

/// Texture blit under an arbitrary transform. Uses nearest-neighbour
/// inverse sampling; matches the existing identity blit sampling
/// semantics so rotating a sprite 0° degenerates to the old output.
#[allow(clippy::too_many_arguments)]
pub fn blit_transformed(
    dst: &mut Texture,
    src: &Texture,
    src_rect: &Rect,
    phys_dst_rect: Rect,
    phys_clip: Rect,
    tf: &Transform,
) {
    let Some(inv) = tf.inverse() else { return };
    let bbox = tf.apply_rect_bbox(phys_dst_rect);
    let Some(draw_area) = bbox.intersect(&phys_clip) else {
        return;
    };
    let screen = Rect::new(0, 0, dst.width, dst.height);
    let Some(draw_area) = draw_area.intersect(&screen) else {
        return;
    };
    let (dx0, dy0, dx1, dy1) = draw_area.pixel_bounds();

    let (sx0, sy0, sw, sh) = src_rect.to_px();
    let dst_x0 = phys_dst_rect.x;
    let dst_y0 = phys_dst_rect.y;
    let dst_w = phys_dst_rect.w;
    let dst_h = phys_dst_rect.h;
    if dst_w <= Fixed::ZERO || dst_h <= Fixed::ZERO || sw == 0 || sh == 0 {
        return;
    }

    for py in dy0..dy1 {
        for px in dx0..dx1 {
            let dp = inv.apply_point(Point {
                x: Fixed::from_int(px) + Fixed::HALF,
                y: Fixed::from_int(py) + Fixed::HALF,
            });
            let u = dp.x - dst_x0;
            let v = dp.y - dst_y0;
            if u < Fixed::ZERO || v < Fixed::ZERO || u >= dst_w || v >= dst_h {
                continue;
            }
            let sx = sx0 + (u * Fixed::from_int(sw as i32) / dst_w).to_int();
            let sy = sy0 + (v * Fixed::from_int(sh as i32) / dst_h).to_int();
            if sx < sx0 || sx >= sx0 + sw as i32 || sy < sy0 || sy >= sy0 + sh as i32 {
                continue;
            }
            let c = src.get_pixel(sx, sy);
            if c.a == 0 {
                continue;
            }
            if c.a == 255 {
                dst.set_pixel(px, py, &c);
            } else {
                dst.blend_pixel_int(px, py, &c, c.a);
            }
        }
    }
}

#[inline]
pub fn offset_rect(r: &Rect, tx: Fixed, ty: Fixed) -> Rect {
    if tx == Fixed::ZERO && ty == Fixed::ZERO {
        return *r;
    }
    Rect {
        x: r.x + tx,
        y: r.y + ty,
        w: r.w,
        h: r.h,
    }
}

#[inline]
pub fn offset_point(p: &Point, tx: Fixed, ty: Fixed) -> Point {
    Point {
        x: p.x + tx,
        y: p.y + ty,
    }
}
