use crate::types::{Color, Fixed, Fixed64, Point, Rect};

use crate::draw::texture::Texture;

#[cfg(feature = "perf")]
use super::perf::quad_perf;

pub fn quad_bbox(q: &[Point; 4]) -> Rect {
    let mut min_x = q[0].x;
    let mut max_x = q[0].x;
    let mut min_y = q[0].y;
    let mut max_y = q[0].y;
    for p in &q[1..] {
        if p.x < min_x {
            min_x = p.x;
        }
        if p.x > max_x {
            max_x = p.x;
        }
        if p.y < min_y {
            min_y = p.y;
        }
        if p.y > max_y {
            max_y = p.y;
        }
    }
    Rect {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    }
}

pub fn blit_quad(dst: &mut Texture, src: &Texture, q: &[Point; 4], phys_clip: Rect) {
    use crate::types::Transform3D;
    let src_rect = Rect::new(0, 0, src.width, src.height);
    let Some(forward) = Transform3D::from_quad(src_rect, q) else {
        return;
    };
    let Some(inverse) = forward.inverse() else {
        return;
    };
    let bbox = quad_bbox(q);
    let Some(area) = bbox.intersect(&phys_clip) else {
        return;
    };
    let screen = Rect::new(0, 0, dst.width, dst.height);
    let Some(area) = area.intersect(&screen) else {
        return;
    };
    let (px_x0, px_y0, px_x1, px_y1) = area.pixel_bounds();
    let sw = src.width as i32;
    let sh = src.height as i32;
    let half = Fixed64::from_raw(Fixed64::ONE.raw() >> 1);
    for py in px_y0..px_y1 {
        let py_f = Fixed::from_int(py) + Fixed::from_raw(128);
        let Some((x_l, x_r)) = quad_row_span(q, py_f) else {
            continue;
        };
        let x_l_px = x_l.to_int().max(px_x0);
        let x_r_px = x_r.ceil().to_int().min(px_x1);
        if x_r_px <= x_l_px {
            continue;
        }
        let x0_f = Fixed64::from_fixed(Fixed::from_int(x_l_px)) + half;
        let y0_f = Fixed64::from_fixed(py_f);
        let mut big_x = inverse.m00 * x0_f + inverse.m01 * y0_f + inverse.m02;
        let mut big_y = inverse.m10 * x0_f + inverse.m11 * y0_f + inverse.m12;
        let mut w = inverse.m20 * x0_f + inverse.m21 * y0_f + inverse.m22;
        #[cfg(feature = "perf")]
        unsafe {
            quad_perf::BLIT_PIXELS_SCANNED += (x_r_px - x_l_px) as u64;
        }
        for px in x_l_px..x_r_px {
            if w.raw() > 0 {
                let inv_w = Fixed64::ONE / w;
                let sx = (big_x * inv_w).to_fixed().to_int();
                let sy = (big_y * inv_w).to_fixed().to_int();
                if sx >= 0 && sx < sw && sy >= 0 && sy < sh {
                    let c = src.get_pixel(sx, sy);
                    if c.a != 0 {
                        #[cfg(feature = "perf")]
                        unsafe {
                            quad_perf::BLIT_PIXELS_DRAWN += 1;
                        }
                        if c.a == 255 {
                            dst.set_pixel(px, py, &c);
                        } else {
                            dst.blend_pixel_int(px, py, &c, c.a);
                        }
                    }
                }
            }
            big_x += inverse.m00;
            big_y += inverse.m10;
            w += inverse.m20;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn fill_rect_quad(
    dst: &mut Texture,
    q: &[Point; 4],
    phys_clip: Rect,
    color: &Color,
    radius: Fixed,
    local_w: Fixed,
    local_h: Fixed,
    opa: u8,
) {
    let bbox = quad_bbox(q);
    let Some(area) = bbox.intersect(&phys_clip) else {
        return;
    };
    let screen = Rect::new(0, 0, dst.width, dst.height);
    let Some(area) = area.intersect(&screen) else {
        return;
    };
    let (px_x0, px_y0, px_x1, px_y1) = area.pixel_bounds();

    if radius == Fixed::ZERO {
        fill_rect_quad_no_corner(dst, q, px_x0, px_y0, px_x1, px_y1, color, opa);
        return;
    }

    let _ = (local_w, local_h);
    use super::quad_aa::{corner_pixel_coverage, quad_pixel_coverage};
    let corners = build_corner_info(q, radius);
    for py in px_y0..px_y1 {
        let py_f = Fixed::from_int(py) + Fixed::from_raw(128);
        let Some((x_l, x_r)) = quad_row_span(q, py_f) else {
            continue;
        };
        let x_l_px = x_l.to_int().max(px_x0);
        let x_r_px = x_r.ceil().to_int().min(px_x1);
        if x_r_px <= x_l_px {
            continue;
        }
        #[cfg(feature = "perf")]
        unsafe {
            quad_perf::FILL_PIXELS_SCANNED += (x_r_px - x_l_px) as u64;
        }
        for px in x_l_px..x_r_px {
            let edge_cov = quad_pixel_coverage(q, px, py);
            if edge_cov == Fixed::ZERO {
                continue;
            }
            // Corner disk only bites pixels sitting in one of the four
            // outward wedges; the wedge test is cheap and lets the
            // interior of the quad bypass the sqrt.
            let cov = {
                let p = Point {
                    x: Fixed::from_int(px) + Fixed::from_raw(128),
                    y: py_f,
                };
                match pixel_in_corner_wedge(&corners, p) {
                    Some(c) => edge_cov * corner_pixel_coverage(px, py, c.center, radius),
                    None => edge_cov,
                }
            };
            if cov == Fixed::ZERO {
                continue;
            }
            let final_opa = if cov == Fixed::ONE {
                opa
            } else {
                let c = (cov * Fixed::from_int(opa as i32)).to_int() as u8;
                if c == 0 {
                    continue;
                }
                c
            };
            #[cfg(feature = "perf")]
            unsafe {
                quad_perf::FILL_PIXELS_DRAWN += 1;
            }
            if final_opa == 255 {
                dst.set_pixel(px, py, color);
            } else {
                dst.blend_pixel_int(px, py, color, final_opa);
            }
        }
    }
}

pub fn stroke_rect_quad(
    dst: &mut Texture,
    q: &[Point; 4],
    phys_clip: Rect,
    color: &Color,
    width: Fixed,
    radius: Fixed,
    opa: u8,
) {
    if width <= Fixed::ZERO {
        return;
    }
    let bbox = quad_bbox(q);
    let Some(area) = bbox.intersect(&phys_clip) else {
        return;
    };
    let screen = Rect::new(0, 0, dst.width, dst.height);
    let Some(area) = area.intersect(&screen) else {
        return;
    };
    let (px_x0, px_y0, px_x1, px_y1) = area.pixel_bounds();

    // Inner quad edges are parallel to outer edges (inset is a uniform
    // shift along incident edges), so both corner sets share the same
    // unit vectors — compute once, inset three times in one loop.
    let shapes = build_corner_shapes(q);
    let inner_radius = (radius - width).max(Fixed::ZERO);
    let mut outer_corners = [CornerInfo::ZERO; 4];
    let mut inner_corners = [CornerInfo::ZERO; 4];
    let mut inner = [Point::ZERO; 4];
    for i in 0..4 {
        outer_corners[i] = shapes[i].inset(radius);
        inner_corners[i] = shapes[i].inset(inner_radius);
        inner[i] = shapes[i].inset(width).center;
    }
    let outer_r_sq = radius * radius;
    let inner_r_sq = inner_radius * inner_radius;
    let degenerate_inner = inner_quad_is_degenerate(&inner);

    for py in px_y0..px_y1 {
        let py_f = Fixed::from_int(py) + Fixed::from_raw(128);
        let Some((x_lo, x_ro)) = quad_row_span(q, py_f) else {
            continue;
        };
        let xlo_px = x_lo.to_int().max(px_x0);
        let xro_px = x_ro.ceil().to_int().min(px_x1);
        if xro_px <= xlo_px {
            continue;
        }
        let inner_span = if degenerate_inner {
            None
        } else {
            quad_row_span(&inner, py_f)
        };
        for px in xlo_px..xro_px {
            let p = Point {
                x: Fixed::from_int(px) + Fixed::from_raw(128),
                y: py_f,
            };
            if pixel_clipped_by_corner(&outer_corners, p, outer_r_sq) {
                continue;
            }
            let in_inner = if let Some((x_li, x_ri)) = inner_span {
                p.x >= x_li && p.x < x_ri && !pixel_clipped_by_corner(&inner_corners, p, inner_r_sq)
            } else {
                false
            };
            if in_inner {
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

fn inner_quad_is_degenerate(inner: &[Point; 4]) -> bool {
    // Detect width >= half-extent: opposite inner vertices cross over,
    // collapsing both diagonals.
    let d1x = inner[2].x - inner[0].x;
    let d1y = inner[2].y - inner[0].y;
    let d2x = inner[3].x - inner[1].x;
    let d2y = inner[3].y - inner[1].y;
    let d1_sq = d1x * d1x + d1y * d1y;
    let d2_sq = d2x * d2x + d2y * d2y;
    d1_sq < Fixed::from_raw(256) || d2_sq < Fixed::from_raw(256)
}

/// Shape of the quad's four corners: vertex + two inward unit vectors.
/// Depends only on the quad geometry, not the inset radius, so can be
/// reused to derive multiple CornerInfo sets (outer, inner, offset).
struct CornerShape {
    vertex: Point,
    ua: Point,
    ub: Point,
}

#[derive(Clone, Copy)]
struct CornerInfo {
    center: Point,
    ua: Point,
    ub: Point,
}

impl CornerInfo {
    const ZERO: Self = Self {
        center: Point::ZERO,
        ua: Point::ZERO,
        ub: Point::ZERO,
    };
}

impl CornerShape {
    fn inset(&self, r: Fixed) -> CornerInfo {
        CornerInfo {
            center: Point {
                x: self.vertex.x + self.ua.x * r + self.ub.x * r,
                y: self.vertex.y + self.ua.y * r + self.ub.y * r,
            },
            ua: self.ua,
            ub: self.ub,
        }
    }
}

fn build_corner_shapes(q: &[Point; 4]) -> [CornerShape; 4] {
    core::array::from_fn(|i| {
        let vertex = q[i];
        let next = q[(i + 1) % 4];
        let prev = q[(i + 3) % 4];
        let ua = unit_vec(next.x - vertex.x, next.y - vertex.y);
        let ub = unit_vec(prev.x - vertex.x, prev.y - vertex.y);
        CornerShape { vertex, ua, ub }
    })
}

fn unit_vec(dx: Fixed, dy: Fixed) -> Point {
    let len = (dx * dx + dy * dy).sqrt();
    if len > Fixed::ZERO {
        Point {
            x: dx / len,
            y: dy / len,
        }
    } else {
        Point::ZERO
    }
}

fn build_corner_info(q: &[Point; 4], r: Fixed) -> [CornerInfo; 4] {
    build_corner_shapes(q).map(|s| s.inset(r))
}

/// Pixel is clipped by a corner iff it is in that corner's outward wedge
/// (both edge projections negative, i.e. past the corner vertex in both
/// directions) AND farther than r from the corner center.
fn pixel_clipped_by_corner(corners: &[CornerInfo; 4], p: Point, r_sq: Fixed) -> bool {
    for c in corners {
        let dx = p.x - c.center.x;
        let dy = p.y - c.center.y;
        let proj_a = dx * c.ua.x + dy * c.ua.y;
        let proj_b = dx * c.ub.x + dy * c.ub.y;
        if proj_a < Fixed::ZERO && proj_b < Fixed::ZERO {
            let dist_sq = dx * dx + dy * dy;
            if dist_sq > r_sq {
                return true;
            }
        }
    }
    false
}

/// If pixel center `p` lies in some corner's outward wedge, return a
/// reference to that corner (so the caller can do analytic disk cov).
/// A pixel is in at most one wedge (wedges are disjoint by construction).
fn pixel_in_corner_wedge(corners: &[CornerInfo; 4], p: Point) -> Option<&CornerInfo> {
    for c in corners {
        let dx = p.x - c.center.x;
        let dy = p.y - c.center.y;
        let proj_a = dx * c.ua.x + dy * c.ua.y;
        let proj_b = dx * c.ub.x + dy * c.ub.y;
        if proj_a < Fixed::ZERO && proj_b < Fixed::ZERO {
            return Some(c);
        }
    }
    None
}

/// Fill a quad with no rounded corners. Each pixel covered by the quad
/// gets analytic coverage from the 4 edges (see `quad_aa`) so sub-pixel
/// translation produces smoothly varying alpha, not step aliasing.
///
/// Hot path: rows entirely outside the quad return None from
/// `quad_row_span` and skip; inside-a-row cost is one `quad_pixel_coverage`
/// call per pixel (4 × `edge_clip_area`).
#[allow(clippy::too_many_arguments)]
fn fill_rect_quad_no_corner(
    dst: &mut Texture,
    q: &[Point; 4],
    px_x0: i32,
    px_y0: i32,
    px_x1: i32,
    px_y1: i32,
    color: &Color,
    opa: u8,
) {
    use super::quad_aa::quad_pixel_coverage;
    for py in px_y0..px_y1 {
        let py_f = Fixed::from_int(py) + Fixed::from_raw(128);
        let Some((x_l, x_r)) = quad_row_span(q, py_f) else {
            continue;
        };
        let xlo_px = x_l.to_int().max(px_x0);
        let xhi_px = x_r.ceil().to_int().min(px_x1);
        if xhi_px <= xlo_px {
            continue;
        }
        #[cfg(feature = "perf")]
        unsafe {
            quad_perf::FILL_PIXELS_SCANNED += (xhi_px - xlo_px) as u64;
        }
        for px in xlo_px..xhi_px {
            let cov = quad_pixel_coverage(q, px, py);
            if cov == Fixed::ZERO {
                continue;
            }
            let final_opa = if cov == Fixed::ONE {
                opa
            } else {
                // cov is in [0, 1] Q24.8; map to 0..=255 and combine with opa.
                let c = (cov * Fixed::from_int(opa as i32)).to_int() as u8;
                if c == 0 {
                    continue;
                }
                c
            };
            if final_opa == 255 {
                dst.set_pixel(px, py, color);
            } else {
                dst.blend_pixel_int(px, py, color, final_opa);
            }
        }
    }
}

/// Intersect horizontal line y=py with convex quad q; return leftmost and
/// rightmost x of the two intersections. None if row is fully outside.
fn quad_row_span(q: &[Point; 4], py: Fixed) -> Option<(Fixed, Fixed)> {
    let mut x_l = Fixed::MAX;
    let mut x_r = Fixed::MIN;
    let mut hit = false;
    for i in 0..4 {
        let a = q[i];
        let b = q[(i + 1) % 4];
        let dy = b.y - a.y;
        if dy.raw() == 0 {
            continue;
        }
        let (y0, y1) = if dy.raw() > 0 { (a.y, b.y) } else { (b.y, a.y) };
        if py < y0 || py >= y1 {
            continue;
        }
        // Linear interp: x = a.x + (py - a.y) / (b.y - a.y) * (b.x - a.x).
        // Fixed64 intermediates avoid Q24.8 mul overflow on large widgets.
        let dx = Fixed64::from_fixed(b.x - a.x);
        let t_num = Fixed64::from_fixed(py - a.y);
        let t_den = Fixed64::from_fixed(dy);
        let x = Fixed64::from_fixed(a.x) + t_num * dx / t_den;
        let x = x.to_fixed();
        if x < x_l {
            x_l = x;
        }
        if x > x_r {
            x_r = x;
        }
        hit = true;
    }
    if hit { Some((x_l, x_r)) } else { None }
}
