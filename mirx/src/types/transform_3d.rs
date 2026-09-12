use super::Fixed64;

/// Row-major 3×3 homography using Q48.16 components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

    pub fn is_identity(self) -> bool {
        self.m00 == Fixed64::ONE
            && self.m01.is_zero()
            && self.m02.is_zero()
            && self.m10.is_zero()
            && self.m11 == Fixed64::ONE
            && self.m12.is_zero()
            && self.m20.is_zero()
            && self.m21.is_zero()
            && self.m22 == Fixed64::ONE
    }
}

impl Default for Transform3D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_default() {
        assert_eq!(Transform3D::default(), Transform3D::IDENTITY);
        assert!(Transform3D::IDENTITY.is_identity());
    }

    #[test]
    fn perspective_term_changes_identity() {
        let transform = Transform3D {
            m20: Fixed64::from_ratio(1, 800),
            ..Transform3D::IDENTITY
        };
        assert!(!transform.is_identity());
    }
}
