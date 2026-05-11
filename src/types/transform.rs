use super::Fixed;

/// 2D affine transform attached per-widget. Currently only used to
/// carry `IDENTITY` through the DrawCommand pipeline so backends reserve
/// the handling path; real translate/rotate/scale/skew live in a future
/// widget-transform spec.
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
    fn identity_passes_point_through() {
        let t = Transform::IDENTITY;
        assert!(t.is_identity());
        assert_eq!(Transform::default(), Transform::IDENTITY);
    }

    #[test]
    fn non_identity_fails_is_identity() {
        let mut t = Transform::IDENTITY;
        t.tx = Fixed::from_int(5);
        assert!(!t.is_identity());
    }
}
