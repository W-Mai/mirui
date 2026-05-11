use super::{Fixed, Point, Rect};

/// 2D affine. Paint-only; layout ignores it.
///
/// ```text
/// [m00  m01  tx ]
/// [m10  m11  ty ]
/// [  0    0   1 ]
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform {
    pub m00: Fixed,
    pub m01: Fixed,
    pub tx: Fixed,
    pub m10: Fixed,
    pub m11: Fixed,
    pub ty: Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformClass {
    Identity,
    Translate,
    AxisAlignedScale,
    Rotate90,
    General,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        m00: Fixed::ONE,
        m01: Fixed::ZERO,
        tx: Fixed::ZERO,
        m10: Fixed::ZERO,
        m11: Fixed::ONE,
        ty: Fixed::ZERO,
    };

    #[inline]
    pub const fn translate(tx: Fixed, ty: Fixed) -> Self {
        Self {
            m00: Fixed::ONE,
            m01: Fixed::ZERO,
            tx,
            m10: Fixed::ZERO,
            m11: Fixed::ONE,
            ty,
        }
    }

    #[inline]
    pub const fn scale(sx: Fixed, sy: Fixed) -> Self {
        Self {
            m00: sx,
            m01: Fixed::ZERO,
            tx: Fixed::ZERO,
            m10: Fixed::ZERO,
            m11: sy,
            ty: Fixed::ZERO,
        }
    }

    #[inline]
    pub fn rotate_deg(angle: Fixed) -> Self {
        let c = Fixed::cos_deg(angle);
        let s = Fixed::sin_deg(angle);
        Self {
            m00: c,
            m01: Fixed::ZERO - s,
            tx: Fixed::ZERO,
            m10: s,
            m11: c,
            ty: Fixed::ZERO,
        }
    }

    /// Blows up at ±90° — caller's problem.
    #[inline]
    pub fn skew_deg(sx_deg: Fixed, sy_deg: Fixed) -> Self {
        let tan_x = Fixed::sin_deg(sx_deg) / Fixed::cos_deg(sx_deg);
        let tan_y = Fixed::sin_deg(sy_deg) / Fixed::cos_deg(sy_deg);
        Self {
            m00: Fixed::ONE,
            m01: tan_x,
            tx: Fixed::ZERO,
            m10: tan_y,
            m11: Fixed::ONE,
            ty: Fixed::ZERO,
        }
    }

    #[inline]
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// `self × other` — applying the result equals applying `other`
    /// first then `self`. Parent × child convention.
    #[inline]
    pub fn compose(&self, other: &Self) -> Self {
        Self {
            m00: self.m00 * other.m00 + self.m01 * other.m10,
            m01: self.m00 * other.m01 + self.m01 * other.m11,
            tx: self.m00 * other.tx + self.m01 * other.ty + self.tx,
            m10: self.m10 * other.m00 + self.m11 * other.m10,
            m11: self.m10 * other.m01 + self.m11 * other.m11,
            ty: self.m10 * other.tx + self.m11 * other.ty + self.ty,
        }
    }

    #[inline]
    pub fn apply_point(&self, p: Point) -> Point {
        Point {
            x: self.m00 * p.x + self.m01 * p.y + self.tx,
            y: self.m10 * p.x + self.m11 * p.y + self.ty,
        }
    }

    pub fn apply_rect_bbox(&self, r: Rect) -> Rect {
        let x0 = r.x;
        let y0 = r.y;
        let x1 = r.x + r.w;
        let y1 = r.y + r.h;
        let p = [
            self.apply_point(Point { x: x0, y: y0 }),
            self.apply_point(Point { x: x1, y: y0 }),
            self.apply_point(Point { x: x0, y: y1 }),
            self.apply_point(Point { x: x1, y: y1 }),
        ];
        let mut min_x = p[0].x;
        let mut max_x = p[0].x;
        let mut min_y = p[0].y;
        let mut max_y = p[0].y;
        for pt in &p[1..] {
            if pt.x < min_x {
                min_x = pt.x;
            }
            if pt.x > max_x {
                max_x = pt.x;
            }
            if pt.y < min_y {
                min_y = pt.y;
            }
            if pt.y > max_y {
                max_y = pt.y;
            }
        }
        Rect {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        }
    }

    #[inline]
    pub fn determinant(&self) -> Fixed {
        self.m00 * self.m11 - self.m01 * self.m10
    }

    /// `None` on singular (determinant == 0).
    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant();
        if det == Fixed::ZERO {
            return None;
        }
        let inv_det = Fixed::ONE / det;
        Some(Self {
            m00: self.m11 * inv_det,
            m01: (Fixed::ZERO - self.m01) * inv_det,
            tx: (self.m01 * self.ty - self.m11 * self.tx) * inv_det,
            m10: (Fixed::ZERO - self.m10) * inv_det,
            m11: self.m00 * inv_det,
            ty: (self.m10 * self.tx - self.m00 * self.ty) * inv_det,
        })
    }

    /// Epsilon-tolerant so `rotate_deg(90)` lands in `Rotate90`
    /// instead of `General` (Q8.8 cos/sin rounding drifts by ~1 LSB).
    pub fn classify(&self) -> TransformClass {
        if self.is_identity() {
            return TransformClass::Identity;
        }

        let eps = Fixed::from_raw(4);
        let near_zero = |v: Fixed| v.abs() < eps;
        let near_one = |v: Fixed| (v - Fixed::ONE).abs() < eps;
        let near_neg_one = |v: Fixed| (v + Fixed::ONE).abs() < eps;

        let scale_like = near_zero(self.m01) && near_zero(self.m10);
        if scale_like && near_one(self.m00) && near_one(self.m11) {
            return TransformClass::Translate;
        }
        if scale_like {
            return TransformClass::AxisAlignedScale;
        }

        let diag_zero = near_zero(self.m00) && near_zero(self.m11);
        if diag_zero
            && ((near_one(self.m01) && near_neg_one(self.m10))
                || (near_neg_one(self.m01) && near_one(self.m10)))
        {
            return TransformClass::Rotate90;
        }

        TransformClass::General
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passes_point_through() {
        let t = Transform::IDENTITY;
        assert!(t.is_identity());
        assert_eq!(Transform::default(), Transform::IDENTITY);
    }

    #[test]
    fn non_identity_fails_is_identity() {
        let t = Transform::translate(Fixed::from_int(5), Fixed::ZERO);
        assert!(!t.is_identity());
    }

    #[test]
    fn translate_applied_to_point() {
        let t = Transform::translate(Fixed::from_int(3), Fixed::from_int(-2));
        let p = t.apply_point(Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
        });
        assert_eq!(p.x.to_int(), 13);
        assert_eq!(p.y.to_int(), 8);
    }

    #[test]
    fn scale_applied_to_point() {
        let t = Transform::scale(Fixed::from_int(2), Fixed::from_int(3));
        let p = t.apply_point(Point {
            x: Fixed::from_int(5),
            y: Fixed::from_int(5),
        });
        assert_eq!(p.x.to_int(), 10);
        assert_eq!(p.y.to_int(), 15);
    }

    #[test]
    fn compose_order_is_parent_times_child() {
        let child = Transform::translate(Fixed::from_int(10), Fixed::ZERO);
        let parent = Transform::scale(Fixed::from_int(2), Fixed::from_int(2));
        let combined = parent.compose(&child);
        let p = combined.apply_point(Point {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        });
        assert_eq!(p.x.to_int(), 20);
        assert_eq!(p.y.to_int(), 0);
    }

    #[test]
    fn rotate_90_maps_x_axis_to_y_axis() {
        let t = Transform::rotate_deg(Fixed::from_int(90));
        let p = t.apply_point(Point {
            x: Fixed::ONE,
            y: Fixed::ZERO,
        });
        // Fixed sin/cos has rounding; allow 1 LSB tolerance.
        assert!(p.x.abs().raw() < 4, "x should be ~0, got {}", p.x.to_f32());
        assert!(
            (p.y - Fixed::ONE).abs().raw() < 4,
            "y should be ~1, got {}",
            p.y.to_f32()
        );
    }

    #[test]
    fn rotate_composed_with_inverse_is_identity() {
        let a = Fixed::from_int(30);
        let t = Transform::rotate_deg(a);
        let inv = t.inverse().expect("rotation is always invertible");
        let round = t.compose(&inv);
        let eps = 10;
        assert!((round.m00 - Fixed::ONE).abs().raw() < eps);
        assert!(round.m01.abs().raw() < eps);
        assert!(round.tx.abs().raw() < eps);
        assert!(round.m10.abs().raw() < eps);
        assert!((round.m11 - Fixed::ONE).abs().raw() < eps);
        assert!(round.ty.abs().raw() < eps);
    }

    #[test]
    fn singular_matrix_inverse_is_none() {
        let t = Transform::scale(Fixed::ZERO, Fixed::ONE);
        assert!(t.inverse().is_none());
    }

    #[test]
    fn apply_rect_bbox_for_45_rotation() {
        let r = Rect::new(-1, -1, 2, 2);
        let t = Transform::rotate_deg(Fixed::from_int(45));
        let bb = t.apply_rect_bbox(r);
        let diag = Fixed::from_f32(2.0_f32.sqrt() * 2.0);
        assert!(
            (bb.w - diag).abs().raw() < 10,
            "bbox w should be ~{}, got {}",
            diag.to_f32(),
            bb.w.to_f32()
        );
        assert!((bb.h - diag).abs().raw() < 10);
    }

    #[test]
    fn classify_identity() {
        assert_eq!(Transform::IDENTITY.classify(), TransformClass::Identity);
    }

    #[test]
    fn classify_translate() {
        let t = Transform::translate(Fixed::from_int(5), Fixed::from_int(-3));
        assert_eq!(t.classify(), TransformClass::Translate);
    }

    #[test]
    fn classify_axis_aligned_scale() {
        let t = Transform::scale(Fixed::from_int(2), Fixed::from_int(3));
        assert_eq!(t.classify(), TransformClass::AxisAlignedScale);
    }

    #[test]
    fn classify_rotate_90_family() {
        // 180° has off-diag zero → lands in AxisAlignedScale, not Rotate90.
        for deg in &[90, 180, 270, -90] {
            let t = Transform::rotate_deg(Fixed::from_int(*deg));
            let expected = if deg.rem_euclid(180) == 0 {
                TransformClass::AxisAlignedScale
            } else {
                TransformClass::Rotate90
            };
            assert_eq!(t.classify(), expected, "deg = {}", deg);
        }
    }

    #[test]
    fn classify_general() {
        let t = Transform::rotate_deg(Fixed::from_int(30));
        assert_eq!(t.classify(), TransformClass::General);
    }

    #[test]
    fn compose_is_associative() {
        let a = Transform::rotate_deg(Fixed::from_int(20));
        let b = Transform::translate(Fixed::from_int(5), Fixed::from_int(-3));
        let c = Transform::scale(Fixed::from_int(2), Fixed::ONE);
        let ab_c = a.compose(&b).compose(&c);
        let a_bc = a.compose(&b.compose(&c));
        // Allow small Fixed rounding drift.
        let eps = 10;
        assert!((ab_c.m00 - a_bc.m00).abs().raw() < eps);
        assert!((ab_c.tx - a_bc.tx).abs().raw() < eps);
        assert!((ab_c.ty - a_bc.ty).abs().raw() < eps);
    }
}
