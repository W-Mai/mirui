use super::{Fixed, Point, Rect};

/// 3×3 homography for 2.5D widget warping. Internal storage is Q16.16
/// (i64 raw with 16 fractional bits) because Q8.8 can't represent the
/// small values in the bottom row — e.g. `1/800 ≈ 0.00125` rounds to
/// zero in Q8.8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform3D {
    pub m00: i64,
    pub m01: i64,
    pub m02: i64,
    pub m10: i64,
    pub m11: i64,
    pub m12: i64,
    pub m20: i64,
    pub m21: i64,
    pub m22: i64,
}

const FRAC_BITS: i64 = 16;
const ONE_Q16: i64 = 1 << FRAC_BITS;

#[inline]
fn from_fixed(f: Fixed) -> i64 {
    (f.raw() as i64) << 8 // Q8.8 → Q16.16 is left-shift 8
}

#[inline]
fn to_fixed(q: i64) -> Fixed {
    let shifted = q >> 8;
    let clamped = shifted.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    Fixed::from_raw(clamped)
}

#[inline]
fn q_mul(a: i64, b: i64) -> i64 {
    (a * b) >> FRAC_BITS
}

#[inline]
fn q_div(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        None
    } else {
        Some((a << FRAC_BITS) / b)
    }
}

impl Transform3D {
    pub const IDENTITY: Self = Self {
        m00: ONE_Q16,
        m01: 0,
        m02: 0,
        m10: 0,
        m11: ONE_Q16,
        m12: 0,
        m20: 0,
        m21: 0,
        m22: ONE_Q16,
    };

    pub fn translate(tx: Fixed, ty: Fixed) -> Self {
        Self {
            m00: ONE_Q16,
            m01: 0,
            m02: from_fixed(tx),
            m10: 0,
            m11: ONE_Q16,
            m12: from_fixed(ty),
            m20: 0,
            m21: 0,
            m22: ONE_Q16,
        }
    }

    pub fn scale(sx: Fixed, sy: Fixed) -> Self {
        Self {
            m00: from_fixed(sx),
            m01: 0,
            m02: 0,
            m10: 0,
            m11: from_fixed(sy),
            m12: 0,
            m20: 0,
            m21: 0,
            m22: ONE_Q16,
        }
    }

    pub fn rotate_deg(angle: Fixed) -> Self {
        let c = from_fixed(Fixed::cos_deg(angle));
        let s = from_fixed(Fixed::sin_deg(angle));
        Self {
            m00: c,
            m01: -s,
            m02: 0,
            m10: s,
            m11: c,
            m12: 0,
            m20: 0,
            m21: 0,
            m22: ONE_Q16,
        }
    }

    /// CSS `perspective(d) rotateY(angle)` equivalent, computed as
    /// one homography. Composing an independent rotate_y with
    /// perspective doesn't yield this because the z component from
    /// the 3D rotation is lost in a 2D matrix.
    pub fn rotate_y_perspective(angle: Fixed, distance: Fixed) -> Self {
        Self::rotate_axis_perspective(angle, distance, true)
    }

    /// CSS `perspective(d) rotateX(angle)` equivalent. See
    /// `rotate_y_perspective` for the "why it's combined" rationale.
    pub fn rotate_x_perspective(angle: Fixed, distance: Fixed) -> Self {
        Self::rotate_axis_perspective(angle, distance, false)
    }

    fn rotate_axis_perspective(angle: Fixed, distance: Fixed, y_axis: bool) -> Self {
        let c = from_fixed(Fixed::cos_deg(angle));
        let s = from_fixed(Fixed::sin_deg(angle));
        let d = from_fixed(distance);
        let s_over_d = q_div(s, d).unwrap_or(0);
        if y_axis {
            Self {
                m00: c,
                m01: 0,
                m02: 0,
                m10: 0,
                m11: ONE_Q16,
                m12: 0,
                m20: s_over_d,
                m21: 0,
                m22: ONE_Q16,
            }
        } else {
            Self {
                m00: ONE_Q16,
                m01: 0,
                m02: 0,
                m10: 0,
                m11: c,
                m12: 0,
                m20: 0,
                m21: s_over_d,
                m22: ONE_Q16,
            }
        }
    }

    /// Parallel-projection rotate around Y (just squash x by cos).
    /// No perspective effect; combine with `perspective_xy` via
    /// `rotate_y_perspective` instead if you want CSS-style cover flow.
    pub fn rotate_y_deg(angle: Fixed) -> Self {
        let c = from_fixed(Fixed::cos_deg(angle));
        Self {
            m00: c,
            m01: 0,
            m02: 0,
            m10: 0,
            m11: ONE_Q16,
            m12: 0,
            m20: 0,
            m21: 0,
            m22: ONE_Q16,
        }
    }

    pub fn rotate_x_deg(angle: Fixed) -> Self {
        let c = from_fixed(Fixed::cos_deg(angle));
        Self {
            m00: ONE_Q16,
            m01: 0,
            m02: 0,
            m10: 0,
            m11: c,
            m12: 0,
            m20: 0,
            m21: 0,
            m22: ONE_Q16,
        }
    }

    /// Identity matrix with just the perspective divide row set —
    /// alone it's a no-op for purely 2D points; needs to be part of
    /// a combined homography to produce perspective. Prefer
    /// `rotate_y_perspective` for actual 3D-looking effects.
    pub fn perspective(distance: Fixed) -> Self {
        Self::perspective_xy(distance, distance)
    }

    pub fn perspective_xy(dx: Fixed, dy: Fixed) -> Self {
        let mx = q_div(-ONE_Q16, from_fixed(dx)).unwrap_or(0);
        let my = q_div(-ONE_Q16, from_fixed(dy)).unwrap_or(0);
        Self {
            m00: ONE_Q16,
            m01: 0,
            m02: 0,
            m10: 0,
            m11: ONE_Q16,
            m12: 0,
            m20: mx,
            m21: my,
            m22: ONE_Q16,
        }
    }

    pub fn from_affine(t: super::Transform) -> Self {
        Self {
            m00: from_fixed(t.m00),
            m01: from_fixed(t.m01),
            m02: from_fixed(t.tx),
            m10: from_fixed(t.m10),
            m11: from_fixed(t.m11),
            m12: from_fixed(t.ty),
            m20: 0,
            m21: 0,
            m22: ONE_Q16,
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    pub fn compose(&self, other: &Self) -> Self {
        Self {
            m00: q_mul(self.m00, other.m00)
                + q_mul(self.m01, other.m10)
                + q_mul(self.m02, other.m20),
            m01: q_mul(self.m00, other.m01)
                + q_mul(self.m01, other.m11)
                + q_mul(self.m02, other.m21),
            m02: q_mul(self.m00, other.m02)
                + q_mul(self.m01, other.m12)
                + q_mul(self.m02, other.m22),
            m10: q_mul(self.m10, other.m00)
                + q_mul(self.m11, other.m10)
                + q_mul(self.m12, other.m20),
            m11: q_mul(self.m10, other.m01)
                + q_mul(self.m11, other.m11)
                + q_mul(self.m12, other.m21),
            m12: q_mul(self.m10, other.m02)
                + q_mul(self.m11, other.m12)
                + q_mul(self.m12, other.m22),
            m20: q_mul(self.m20, other.m00)
                + q_mul(self.m21, other.m10)
                + q_mul(self.m22, other.m20),
            m21: q_mul(self.m20, other.m01)
                + q_mul(self.m21, other.m11)
                + q_mul(self.m22, other.m21),
            m22: q_mul(self.m20, other.m02)
                + q_mul(self.m21, other.m12)
                + q_mul(self.m22, other.m22),
        }
    }

    /// Returns `None` when the projected `w' <= 0` (point behind the
    /// camera after a strong perspective).
    pub fn apply_point(&self, p: Point) -> Option<Point> {
        let x = from_fixed(p.x);
        let y = from_fixed(p.y);
        let xp = q_mul(self.m00, x) + q_mul(self.m01, y) + self.m02;
        let yp = q_mul(self.m10, x) + q_mul(self.m11, y) + self.m12;
        let w = q_mul(self.m20, x) + q_mul(self.m21, y) + self.m22;
        if w <= 0 {
            return None;
        }
        let sx = q_div(xp, w)?;
        let sy = q_div(yp, w)?;
        Some(Point {
            x: to_fixed(sx),
            y: to_fixed(sy),
        })
    }

    pub fn apply_rect(&self, r: Rect) -> Option<[Point; 4]> {
        let x0 = r.x;
        let y0 = r.y;
        let x1 = r.x + r.w;
        let y1 = r.y + r.h;
        let p0 = self.apply_point(Point { x: x0, y: y0 })?;
        let p1 = self.apply_point(Point { x: x1, y: y0 })?;
        let p2 = self.apply_point(Point { x: x1, y: y1 })?;
        let p3 = self.apply_point(Point { x: x0, y: y1 })?;
        Some([p0, p1, p2, p3])
    }

    pub fn inverse(&self) -> Option<Self> {
        // Adjugate cells: each is two Q16.16 multiplies then a
        // subtract, so the raw result is Q32.32. Shift back to Q16.16
        // before using them as a normal matrix.
        let a = (self.m11 * self.m22 - self.m12 * self.m21) >> FRAC_BITS;
        let b = (self.m02 * self.m21 - self.m01 * self.m22) >> FRAC_BITS;
        let c = (self.m01 * self.m12 - self.m02 * self.m11) >> FRAC_BITS;
        let d = (self.m12 * self.m20 - self.m10 * self.m22) >> FRAC_BITS;
        let e = (self.m00 * self.m22 - self.m02 * self.m20) >> FRAC_BITS;
        let f = (self.m02 * self.m10 - self.m00 * self.m12) >> FRAC_BITS;
        let g = (self.m10 * self.m21 - self.m11 * self.m20) >> FRAC_BITS;
        let h = (self.m01 * self.m20 - self.m00 * self.m21) >> FRAC_BITS;
        let i = (self.m00 * self.m11 - self.m01 * self.m10) >> FRAC_BITS;

        // det via first-row expansion; again shift back to Q16.16.
        let det = q_mul(self.m00, a) + q_mul(self.m01, d) + q_mul(self.m02, g);
        if det == 0 {
            return None;
        }

        let inv = |v: i64| q_div(v, det);
        Some(Self {
            m00: inv(a)?,
            m01: inv(b)?,
            m02: inv(c)?,
            m10: inv(d)?,
            m11: inv(e)?,
            m12: inv(f)?,
            m20: inv(g)?,
            m21: inv(h)?,
            m22: inv(i)?,
        })
    }
}

impl Default for Transform3D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Test whether `p` lies inside the quadrilateral `q` (given in
/// either winding order). Uses signed-edge cross products; on the
/// boundary counts as inside.
pub fn point_in_quad(q: &[Point; 4], p: Point) -> bool {
    let edge = |a: Point, b: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let s0 = edge(q[0], q[1]);
    let s1 = edge(q[1], q[2]);
    let s2 = edge(q[2], q[3]);
    let s3 = edge(q[3], q[0]);
    let all_pos = s0 >= Fixed::ZERO && s1 >= Fixed::ZERO && s2 >= Fixed::ZERO && s3 >= Fixed::ZERO;
    let all_neg = s0 <= Fixed::ZERO && s1 <= Fixed::ZERO && s2 <= Fixed::ZERO && s3 <= Fixed::ZERO;
    all_pos || all_neg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passes_point() {
        let p = Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(20),
        };
        let out = Transform3D::IDENTITY.apply_point(p).unwrap();
        assert_eq!(out.x.to_int(), 10);
        assert_eq!(out.y.to_int(), 20);
    }

    #[test]
    fn translate_applied() {
        let t = Transform3D::translate(Fixed::from_int(5), Fixed::from_int(-3));
        let p = t
            .apply_point(Point {
                x: Fixed::from_int(1),
                y: Fixed::from_int(1),
            })
            .unwrap();
        assert_eq!(p.x.to_int(), 6);
        assert_eq!(p.y.to_int(), -2);
    }

    #[test]
    fn scale_applied() {
        let t = Transform3D::scale(Fixed::from_int(2), Fixed::from_int(3));
        let p = t
            .apply_point(Point {
                x: Fixed::from_int(4),
                y: Fixed::from_int(5),
            })
            .unwrap();
        assert_eq!(p.x.to_int(), 8);
        assert_eq!(p.y.to_int(), 15);
    }

    #[test]
    fn rotate_90_degrees() {
        let t = Transform3D::rotate_deg(Fixed::from_int(90));
        let p = t
            .apply_point(Point {
                x: Fixed::ONE,
                y: Fixed::ZERO,
            })
            .unwrap();
        assert!(p.x.abs().raw() < 4, "x ≈ 0, got {}", p.x.to_f32());
        assert!(
            (p.y - Fixed::ONE).abs().raw() < 4,
            "y ≈ 1, got {}",
            p.y.to_f32()
        );
    }

    #[test]
    fn compose_is_associative() {
        let a = Transform3D::rotate_deg(Fixed::from_int(20));
        let b = Transform3D::translate(Fixed::from_int(5), Fixed::from_int(-3));
        let c = Transform3D::scale(Fixed::from_int(2), Fixed::ONE);
        let left = a.compose(&b).compose(&c);
        let right = a.compose(&b.compose(&c));
        let p = Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
        };
        let pl = left.apply_point(p).unwrap();
        let pr = right.apply_point(p).unwrap();
        assert!((pl.x - pr.x).abs().raw() < 20);
        assert!((pl.y - pr.y).abs().raw() < 20);
    }

    #[test]
    fn inverse_round_trip() {
        let t = Transform3D::rotate_deg(Fixed::from_int(30)).compose(&Transform3D::translate(
            Fixed::from_int(7),
            Fixed::from_int(-4),
        ));
        let inv = t.inverse().expect("affine is invertible");
        let p = Point {
            x: Fixed::from_int(12),
            y: Fixed::from_int(9),
        };
        let back = inv.apply_point(t.apply_point(p).unwrap()).unwrap();
        assert!((back.x - p.x).abs().raw() < 20);
        assert!((back.y - p.y).abs().raw() < 20);
    }

    #[test]
    fn rotate_y_perspective_compresses_far_side() {
        // Plain parallel rotateY just scales x by cos. The combined
        // rotate_y_perspective additionally divides by (1 + x*sin/d),
        // so the "far" edge compresses further.
        let plain = Transform3D::rotate_y_deg(Fixed::from_int(45));
        let with_persp =
            Transform3D::rotate_y_perspective(Fixed::from_int(45), Fixed::from_int(400));
        let right_edge = Point {
            x: Fixed::from_int(50),
            y: Fixed::ZERO,
        };
        let p1 = plain.apply_point(right_edge).unwrap();
        let p2 = with_persp.apply_point(right_edge).unwrap();
        assert!(p2.x.to_f32().abs() < p1.x.to_f32().abs());
    }

    #[test]
    fn perspective_xy_asymmetric() {
        let t = Transform3D::perspective_xy(Fixed::from_int(400), Fixed::from_int(200));
        // m20 != m21
        assert_ne!(t.m20, t.m21);
    }

    #[test]
    fn apply_rect_returns_four_corners() {
        let t = Transform3D::IDENTITY;
        let r = Rect::new(10, 20, 30, 40);
        let q = t.apply_rect(r).unwrap();
        assert_eq!(q[0].x.to_int(), 10);
        assert_eq!(q[0].y.to_int(), 20);
        assert_eq!(q[2].x.to_int(), 40);
        assert_eq!(q[2].y.to_int(), 60);
    }

    #[test]
    fn from_affine_matches_2d_application() {
        let t2d = super::super::Transform::rotate_deg(Fixed::from_int(30)).compose(
            &super::super::Transform::translate(Fixed::from_int(5), Fixed::ZERO),
        );
        let t3d = Transform3D::from_affine(t2d);
        let p = Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
        };
        let p2 = t2d.apply_point(p);
        let p3 = t3d.apply_point(p).unwrap();
        assert!((p2.x - p3.x).abs().raw() < 20);
        assert!((p2.y - p3.y).abs().raw() < 20);
    }
}
