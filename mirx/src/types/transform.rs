use super::Fixed;

/// 2D affine transform in row-major 3×3 form with an implicit bottom
/// row `[0 0 1]`. Wire layout: six `Fixed` values in the order
/// `m00, m01, tx, m10, m11, ty` — the same field order
/// `mirui::types::Transform` uses.
///
/// `Transform` carries no projective-quad hook; vector-scene ops that
/// need a quad (Blit / FillRect / Border) keep the quad as a separate
/// `Option<[Point; 4]>` field on the op itself. This mirrors the
/// existing wire format and the runtime type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform {
    pub m00: Fixed,
    pub m01: Fixed,
    pub tx: Fixed,
    pub m10: Fixed,
    pub m11: Fixed,
    pub ty: Fixed,
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
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
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
    fn identity_is_default() {
        assert_eq!(Transform::default(), Transform::IDENTITY);
    }

    #[test]
    fn translate_preserves_identity_classification() {
        let t = Transform::translate(Fixed::ZERO, Fixed::ZERO);
        assert!(t.is_identity());
    }

    #[test]
    fn non_zero_translate_breaks_identity() {
        let t = Transform::translate(Fixed::from_int(1), Fixed::ZERO);
        assert!(!t.is_identity());
    }
}
