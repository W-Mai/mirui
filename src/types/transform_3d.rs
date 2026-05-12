use super::{Fixed, Fixed64, Point, Rect};

/// 3×3 homography for 2.5D widget warping. Uses [`Fixed64`] (Q48.16)
/// instead of [`Fixed`] (Q24.8) because Q24.8 can't represent the small
/// values in the bottom row — e.g. `1/800 ≈ 0.00125` rounds to zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform3D {
    pub m00: Fixed64,
    pub m01: Fixed64,
    pub m02: Fixed64,
    pub m10: Fixed64,
    pub m11: Fixed64,
    pub m12: Fixed64,
    pub m20: Fixed64,
    pub m21: Fixed64,
    pub m22: Fixed64,
}

#[inline]
fn try_div(a: Fixed64, b: Fixed64) -> Option<Fixed64> {
    if b.raw() == 0 { None } else { Some(a / b) }
}

impl Transform3D {
    pub const IDENTITY: Self = Self {
        m00: Fixed64::ONE,
        m01: Fixed64::ZERO,
        m02: Fixed64::ZERO,
        m10: Fixed64::ZERO,
        m11: Fixed64::ONE,
        m12: Fixed64::ZERO,
        m20: Fixed64::ZERO,
        m21: Fixed64::ZERO,
        m22: Fixed64::ONE,
    };

    pub fn translate(tx: Fixed, ty: Fixed) -> Self {
        Self {
            m00: Fixed64::ONE,
            m01: Fixed64::ZERO,
            m02: Fixed64::from_fixed(tx),
            m10: Fixed64::ZERO,
            m11: Fixed64::ONE,
            m12: Fixed64::from_fixed(ty),
            m20: Fixed64::ZERO,
            m21: Fixed64::ZERO,
            m22: Fixed64::ONE,
        }
    }

    pub fn scale(sx: Fixed, sy: Fixed) -> Self {
        Self {
            m00: Fixed64::from_fixed(sx),
            m01: Fixed64::ZERO,
            m02: Fixed64::ZERO,
            m10: Fixed64::ZERO,
            m11: Fixed64::from_fixed(sy),
            m12: Fixed64::ZERO,
            m20: Fixed64::ZERO,
            m21: Fixed64::ZERO,
            m22: Fixed64::ONE,
        }
    }

    pub fn rotate_deg(angle: Fixed) -> Self {
        let c = Fixed64::from_fixed(Fixed::cos_deg(angle));
        let s = Fixed64::from_fixed(Fixed::sin_deg(angle));
        Self {
            m00: c,
            m01: -s,
            m02: Fixed64::ZERO,
            m10: s,
            m11: c,
            m12: Fixed64::ZERO,
            m20: Fixed64::ZERO,
            m21: Fixed64::ZERO,
            m22: Fixed64::ONE,
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
        let c = Fixed64::from_fixed(Fixed::cos_deg(angle));
        let s = Fixed64::from_fixed(Fixed::sin_deg(angle));
        let d = Fixed64::from_fixed(distance);
        let s_over_d = try_div(s, d).unwrap_or(Fixed64::ZERO);
        if y_axis {
            Self {
                m00: c,
                m01: Fixed64::ZERO,
                m02: Fixed64::ZERO,
                m10: Fixed64::ZERO,
                m11: Fixed64::ONE,
                m12: Fixed64::ZERO,
                m20: s_over_d,
                m21: Fixed64::ZERO,
                m22: Fixed64::ONE,
            }
        } else {
            Self {
                m00: Fixed64::ONE,
                m01: Fixed64::ZERO,
                m02: Fixed64::ZERO,
                m10: Fixed64::ZERO,
                m11: c,
                m12: Fixed64::ZERO,
                m20: Fixed64::ZERO,
                m21: s_over_d,
                m22: Fixed64::ONE,
            }
        }
    }

    /// Parallel-projection rotate around Y (just squash x by cos).
    /// No perspective effect; combine with `perspective_xy` via
    /// `rotate_y_perspective` instead if you want CSS-style cover flow.
    pub fn rotate_y_deg(angle: Fixed) -> Self {
        let c = Fixed64::from_fixed(Fixed::cos_deg(angle));
        Self {
            m00: c,
            m01: Fixed64::ZERO,
            m02: Fixed64::ZERO,
            m10: Fixed64::ZERO,
            m11: Fixed64::ONE,
            m12: Fixed64::ZERO,
            m20: Fixed64::ZERO,
            m21: Fixed64::ZERO,
            m22: Fixed64::ONE,
        }
    }

    pub fn rotate_x_deg(angle: Fixed) -> Self {
        let c = Fixed64::from_fixed(Fixed::cos_deg(angle));
        Self {
            m00: Fixed64::ONE,
            m01: Fixed64::ZERO,
            m02: Fixed64::ZERO,
            m10: Fixed64::ZERO,
            m11: c,
            m12: Fixed64::ZERO,
            m20: Fixed64::ZERO,
            m21: Fixed64::ZERO,
            m22: Fixed64::ONE,
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
        let mx = try_div(-Fixed64::ONE, Fixed64::from_fixed(dx)).unwrap_or(Fixed64::ZERO);
        let my = try_div(-Fixed64::ONE, Fixed64::from_fixed(dy)).unwrap_or(Fixed64::ZERO);
        Self {
            m00: Fixed64::ONE,
            m01: Fixed64::ZERO,
            m02: Fixed64::ZERO,
            m10: Fixed64::ZERO,
            m11: Fixed64::ONE,
            m12: Fixed64::ZERO,
            m20: mx,
            m21: my,
            m22: Fixed64::ONE,
        }
    }

    pub fn from_affine(t: super::Transform) -> Self {
        Self {
            m00: Fixed64::from_fixed(t.m00),
            m01: Fixed64::from_fixed(t.m01),
            m02: Fixed64::from_fixed(t.tx),
            m10: Fixed64::from_fixed(t.m10),
            m11: Fixed64::from_fixed(t.m11),
            m12: Fixed64::from_fixed(t.ty),
            m20: Fixed64::ZERO,
            m21: Fixed64::ZERO,
            m22: Fixed64::ONE,
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    pub fn compose(&self, other: &Self) -> Self {
        Self {
            m00: self.m00 * other.m00 + self.m01 * other.m10 + self.m02 * other.m20,
            m01: self.m00 * other.m01 + self.m01 * other.m11 + self.m02 * other.m21,
            m02: self.m00 * other.m02 + self.m01 * other.m12 + self.m02 * other.m22,
            m10: self.m10 * other.m00 + self.m11 * other.m10 + self.m12 * other.m20,
            m11: self.m10 * other.m01 + self.m11 * other.m11 + self.m12 * other.m21,
            m12: self.m10 * other.m02 + self.m11 * other.m12 + self.m12 * other.m22,
            m20: self.m20 * other.m00 + self.m21 * other.m10 + self.m22 * other.m20,
            m21: self.m20 * other.m01 + self.m21 * other.m11 + self.m22 * other.m21,
            m22: self.m20 * other.m02 + self.m21 * other.m12 + self.m22 * other.m22,
        }
    }

    /// Returns `None` when the projected `w' <= 0` (point behind the
    /// camera after a strong perspective).
    pub fn apply_point(&self, p: Point) -> Option<Point> {
        let x = Fixed64::from_fixed(p.x);
        let y = Fixed64::from_fixed(p.y);
        let xp = self.m00 * x + self.m01 * y + self.m02;
        let yp = self.m10 * x + self.m11 * y + self.m12;
        let w = self.m20 * x + self.m21 * y + self.m22;
        if w.raw() <= 0 {
            return None;
        }
        let sx = try_div(xp, w)?;
        let sy = try_div(yp, w)?;
        Some(Point {
            x: sx.to_fixed(),
            y: sy.to_fixed(),
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

    /// Homography from an axis-aligned source rect to a destination
    /// quadrilateral (in `[top-left, top-right, bottom-right, bottom-left]`
    /// order, matching `apply_rect`'s output). Returns `None` when the
    /// quad is degenerate (three collinear points).
    ///
    /// Closed-form per Paul Heckbert, "Fundamentals of Texture Mapping
    /// and Image Warping" (1989), §A.
    pub fn from_quad(src_rect: Rect, dst_quad: &[Point; 4]) -> Option<Self> {
        let ux = Fixed64::from_fixed(src_rect.w);
        let uy = Fixed64::from_fixed(src_rect.h);
        if ux.raw() == 0 || uy.raw() == 0 {
            return None;
        }

        let q0x = Fixed64::from_fixed(dst_quad[0].x);
        let q0y = Fixed64::from_fixed(dst_quad[0].y);
        let q1x = Fixed64::from_fixed(dst_quad[1].x);
        let q1y = Fixed64::from_fixed(dst_quad[1].y);
        let q2x = Fixed64::from_fixed(dst_quad[2].x);
        let q2y = Fixed64::from_fixed(dst_quad[2].y);
        let q3x = Fixed64::from_fixed(dst_quad[3].x);
        let q3y = Fixed64::from_fixed(dst_quad[3].y);

        let dx1 = q1x - q2x;
        let dy1 = q1y - q2y;
        let dx2 = q3x - q2x;
        let dy2 = q3y - q2y;
        let sx = q0x - q1x + q2x - q3x;
        let sy = q0y - q1y + q2y - q3y;

        let denom = dx1 * dy2 - dy1 * dx2;
        if denom.raw() == 0 {
            return None;
        }

        let g_num = sx * dy2 - sy * dx2;
        let h_num = dx1 * sy - dy1 * sx;
        let g = try_div(try_div(g_num, denom)?, ux)?;
        let h = try_div(try_div(h_num, denom)?, uy)?;

        let a = try_div(q1x - q0x, ux)? + g * q1x;
        let b = try_div(q3x - q0x, uy)? + h * q3x;
        let c = q0x;

        let d = try_div(q1y - q0y, ux)? + g * q1y;
        let e = try_div(q3y - q0y, uy)? + h * q3y;
        let f = q0y;

        Some(Self {
            m00: a,
            m01: b,
            m02: c,
            m10: d,
            m11: e,
            m12: f,
            m20: g,
            m21: h,
            m22: Fixed64::ONE,
        })
    }

    pub fn inverse(&self) -> Option<Self> {
        let a = self.m11 * self.m22 - self.m12 * self.m21;
        let b = self.m02 * self.m21 - self.m01 * self.m22;
        let c = self.m01 * self.m12 - self.m02 * self.m11;
        let d = self.m12 * self.m20 - self.m10 * self.m22;
        let e = self.m00 * self.m22 - self.m02 * self.m20;
        let f = self.m02 * self.m10 - self.m00 * self.m12;
        let g = self.m10 * self.m21 - self.m11 * self.m20;
        let h = self.m01 * self.m20 - self.m00 * self.m21;
        let i = self.m00 * self.m11 - self.m01 * self.m10;

        let det = self.m00 * a + self.m01 * d + self.m02 * g;
        if det.raw() == 0 {
            return None;
        }

        let inv = |v: Fixed64| try_div(v, det);
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
    // i64 cross: Q8.8 raw squared overflows i32 past ~180 px wide.
    let edge = |a: Point, b: Point| -> i64 {
        let dx = (b.x.raw() as i64) - (a.x.raw() as i64);
        let dy = (b.y.raw() as i64) - (a.y.raw() as i64);
        let px = (p.x.raw() as i64) - (a.x.raw() as i64);
        let py = (p.y.raw() as i64) - (a.y.raw() as i64);
        dx * py - dy * px
    };
    let s0 = edge(q[0], q[1]);
    let s1 = edge(q[1], q[2]);
    let s2 = edge(q[2], q[3]);
    let s3 = edge(q[3], q[0]);
    let all_pos = s0 >= 0 && s1 >= 0 && s2 >= 0 && s3 >= 0;
    let all_neg = s0 <= 0 && s1 <= 0 && s2 <= 0 && s3 <= 0;
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
    fn from_quad_maps_src_corners_to_dst() {
        let src = Rect::new(0, 0, 100, 80);
        let dst = [
            Point {
                x: Fixed::from_int(10),
                y: Fixed::from_int(20),
            },
            Point {
                x: Fixed::from_int(110),
                y: Fixed::from_int(15),
            },
            Point {
                x: Fixed::from_int(120),
                y: Fixed::from_int(100),
            },
            Point {
                x: Fixed::from_int(5),
                y: Fixed::from_int(95),
            },
        ];
        let h = Transform3D::from_quad(src, &dst).expect("non-degenerate");
        let corners = [
            Point {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            Point {
                x: Fixed::from_int(100),
                y: Fixed::ZERO,
            },
            Point {
                x: Fixed::from_int(100),
                y: Fixed::from_int(80),
            },
            Point {
                x: Fixed::ZERO,
                y: Fixed::from_int(80),
            },
        ];
        // Q8.8 storage of Q16.16 matrix accumulates up to ~0.3 of
        // rounding across the solve; tolerance ≈ 1 pixel.
        for (i, c) in corners.iter().enumerate() {
            let out = h.apply_point(*c).expect("projects");
            assert!(
                (out.x - dst[i].x).abs().raw() < 256,
                "corner {} x: got {} want {}",
                i,
                out.x.to_f32(),
                dst[i].x.to_f32()
            );
            assert!(
                (out.y - dst[i].y).abs().raw() < 256,
                "corner {} y: got {} want {}",
                i,
                out.y.to_f32(),
                dst[i].y.to_f32()
            );
        }
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

    #[test]
    fn point_in_quad_handles_large_widget() {
        let q = [
            Point {
                x: Fixed::from_int(0),
                y: Fixed::from_int(0),
            },
            Point {
                x: Fixed::from_int(800),
                y: Fixed::from_int(0),
            },
            Point {
                x: Fixed::from_int(800),
                y: Fixed::from_int(800),
            },
            Point {
                x: Fixed::from_int(0),
                y: Fixed::from_int(800),
            },
        ];
        assert!(point_in_quad(
            &q,
            Point {
                x: Fixed::from_int(400),
                y: Fixed::from_int(400),
            }
        ));
        assert!(!point_in_quad(
            &q,
            Point {
                x: Fixed::from_int(900),
                y: Fixed::from_int(400),
            }
        ));
    }
}
