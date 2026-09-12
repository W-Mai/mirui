//! 48.16 fixed-point number used by projective VECTOR state.

/// Q48.16 scalar used by 3×3 homographies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed64(i64);

impl Fixed64 {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1 << 16);

    pub const fn from_int(value: i64) -> Self {
        Self(value << 16)
    }

    pub const fn from_ratio(numerator: i64, denominator: i64) -> Self {
        assert!(denominator != 0, "fixed-point denominator must not be zero");
        let raw = (numerator as i128 * (1_i128 << 16)) / denominator as i128;
        assert!(
            raw >= i64::MIN as i128 && raw <= i64::MAX as i128,
            "fixed-point ratio is out of range"
        );
        Self(raw as i64)
    }

    pub const fn to_int(self) -> i64 {
        self.0 >> 16
    }

    pub const fn to_parts(self) -> (i64, u16) {
        (self.to_int(), self.0 as u16)
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 65_536.0
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 65_536.0
    }

    pub(crate) const fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self(i64::from_le_bytes(bytes))
    }

    pub(crate) const fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_constructors_preserve_q48_16_values() {
        assert_eq!(Fixed64::from_int(7).to_int(), 7);
        assert!(
            (Fixed64::from_ratio(1, 800).to_f64() - 0.001_235_961_914_062_5).abs() < f64::EPSILON
        );
        assert!((Fixed64::from_ratio(1, 4).to_f64() - 0.25).abs() < f64::EPSILON);
        assert_eq!(Fixed64(i64::MAX).to_parts(), (i64::MAX >> 16, u16::MAX));
        assert_eq!(Fixed64(i64::MIN).to_parts(), (i64::MIN >> 16, 0));
        assert_eq!(Fixed64(-1).to_parts(), (-1, u16::MAX));
    }
}
