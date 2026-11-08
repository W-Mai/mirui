use crate::types::{Fixed, Point, Rect};

#[derive(Clone, Copy)]
pub(super) struct Geometry {
    pub center: Point,
    pub scale: Fixed,
}
impl Geometry {
    pub fn new(rect: Rect) -> Self {
        let scale = Fixed::ONE
            .min(
                ((rect.w - Fixed::from_int(36)) / Fixed::from_int(456))
                    .max(Fixed::from_ratio(1, 10)),
            )
            .min(
                ((rect.h - Fixed::from_int(120)) / Fixed::from_int(384))
                    .max(Fixed::from_ratio(1, 10)),
            );
        Self {
            center: Point::new(
                rect.x + rect.w / 2,
                rect.y + rect.h / 2 - Fixed::from_int(10),
            ),
            scale,
        }
    }
    pub fn local(self, x: Fixed, y: Fixed) -> Point {
        Point::new(
            (x - self.center.x) / self.scale,
            (y - self.center.y) / self.scale,
        )
    }
    pub fn contains(self, x: Fixed, y: Fixed) -> bool {
        let p = self.local(x, y);
        let dx = (p.x.abs() - Fixed::from_int(144)).max(Fixed::ZERO);
        let dy = (p.y.abs() - Fixed::from_int(40)).max(Fixed::ZERO);
        p.x.abs() <= Fixed::from_int(216)
            && p.y.abs() <= Fixed::from_int(112)
            && dx * dx + dy * dy <= Fixed::from_int(72 * 72)
    }
}

pub(super) fn light_point(progress: Fixed) -> Point {
    let u = progress.clamp(Fixed::ZERO, Fixed::ONE) * Fixed::from_ratio(346212, 1000);
    let corner = Fixed::from_ratio(91106, 1000);
    if u < Fixed::from_int(41) {
        return Point::new(u, 99);
    }
    if u < Fixed::from_int(41) + corner {
        let angle = Fixed::from_int(90) * (Fixed::ONE - (u - Fixed::from_int(41)) / corner);
        return Point::new(
            Fixed::from_int(41) + Fixed::cos_deg(angle) * Fixed::from_int(58),
            Fixed::from_int(41) + Fixed::sin_deg(angle) * Fixed::from_int(58),
        );
    }
    if u < Fixed::from_int(123) + corner {
        return Point::new(99, Fixed::from_int(41) - (u - Fixed::from_int(41) - corner));
    }
    if u < Fixed::from_int(123) + corner * 2 {
        let angle = -(u - Fixed::from_int(123) - corner) / corner * Fixed::from_int(90);
        return Point::new(
            Fixed::from_int(41) + Fixed::cos_deg(angle) * Fixed::from_int(58),
            Fixed::from_int(-41) + Fixed::sin_deg(angle) * Fixed::from_int(58),
        );
    }
    Point::new(
        Fixed::from_int(41) - (u - Fixed::from_int(123) - corner * 2),
        -99,
    )
}

// Millipixel inputs keep fractional strokes constant-evaluated without heap storage.
const fn point(x: i32, y: i32) -> Point {
    Point {
        x: Fixed::from_ratio(x, 1000),
        y: Fixed::from_ratio(y, 1000),
    }
}
pub(super) const fn rounded_rect(hw: i32, hh: i32, r: i32) -> [crate::render::path::PathCmd; 10] {
    use crate::render::path::PathCmd::*;
    assert!(r > 0 && r <= hw && r <= hh);
    let x = hw - r;
    let y = hh - r;
    let k = (r as i64 * 552285 / 1_000_000) as i32;
    [
        MoveTo(point(-x, -hh)),
        LineTo(point(x, -hh)),
        CubicTo {
            ctrl1: point(x + k, -hh),
            ctrl2: point(hw, -y - k),
            end: point(hw, -y),
        },
        LineTo(point(hw, y)),
        CubicTo {
            ctrl1: point(hw, y + k),
            ctrl2: point(x + k, hh),
            end: point(x, hh),
        },
        LineTo(point(-x, hh)),
        CubicTo {
            ctrl1: point(-x - k, hh),
            ctrl2: point(-hw, y + k),
            end: point(-hw, y),
        },
        LineTo(point(-hw, -y)),
        CubicTo {
            ctrl1: point(-hw, -y - k),
            ctrl2: point(-x - k, -hh),
            end: point(-x, -hh),
        },
        Close,
    ]
}
pub(super) const fn ring(
    hw: i32,
    hh: i32,
    r: i32,
    width: i32,
) -> [crate::render::path::PathCmd; 20] {
    let d = width / 2;
    let [a, b, c, d0, e, f, g, h, i, j] = rounded_rect(hw + d, hh + d, r + d);
    let [k, l, m, n, o, p, q, s, t, u] = rounded_rect(hw - d, hh - d, r - d);
    [a, b, c, d0, e, f, g, h, i, j, k, l, m, n, o, p, q, s, t, u]
}
