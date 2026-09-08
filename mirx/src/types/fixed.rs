//! 24.8 fixed-point number. High 24 bits = integer, low 8 = fraction.
//!
//! Wire layout: four little-endian bytes containing the signed Q24.8 value.
//! The integer representation stays private; wire implementations use the
//! explicit byte conversion methods.
//!
//! mirx intentionally does **not** implement arithmetic on `Fixed`.
//! Consumers convert to their own `Fixed` (which has the `Add` / `Mul`
//! / `Sub` / etc. impls) via `From` and do the math there. Keeping
//! mirx arithmetic-free keeps the crate thin and lets each consumer
//! pick its own overflow / saturating policy.

/// Q24.8 scalar used by VECTOR and FONT wire records.
///
/// ```compile_fail
/// use mirx::types::Fixed;
/// let value = Fixed(128);
/// ```
///
/// ```compile_fail
/// use mirx::types::Fixed;
/// let value = Fixed::from_raw(128);
/// ```
///
/// ```compile_fail
/// use mirx::types::Fixed;
/// let value = Fixed::ONE.raw();
/// ```
///
/// ```compile_fail
/// use mirx::types::Fixed;
/// let value = Fixed::ONE.0;
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed(i32);

impl Fixed {
    pub const ZERO: Self = Fixed(0);
    pub const ONE: Self = Fixed(256);

    pub const fn from_int(v: i32) -> Self {
        Fixed(v << 8)
    }

    /// Constructs `numerator / denominator` with Q24.8 truncation toward zero.
    ///
    /// # Panics
    ///
    /// Panics when `denominator` is zero or the scaled result exceeds Q24.8.
    pub const fn from_ratio(numerator: i32, denominator: i32) -> Self {
        assert!(denominator != 0, "fixed-point denominator must not be zero");
        let raw = (numerator as i64 * 256) / denominator as i64;
        assert!(
            raw >= i32::MIN as i64 && raw <= i32::MAX as i64,
            "fixed-point ratio is out of range"
        );
        Self(raw as i32)
    }

    pub const fn to_int(self) -> i32 {
        self.0 >> 8
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub const fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self(i32::from_le_bytes(bytes))
    }

    pub const fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }

    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 256.0
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 256.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_int_roundtrips() {
        assert_eq!(Fixed::from_int(0).to_le_bytes(), 0_i32.to_le_bytes());
        assert_eq!(Fixed::from_int(1).to_le_bytes(), 256_i32.to_le_bytes());
        assert_eq!(Fixed::from_int(-1).to_le_bytes(), (-256_i32).to_le_bytes());
        assert_eq!(Fixed::from_int(42).to_int(), 42);
    }

    #[test]
    fn wire_bytes_roundtrip() {
        assert_eq!(
            Fixed::from_le_bytes(1234_i32.to_le_bytes()).to_le_bytes(),
            1234_i32.to_le_bytes()
        );
        assert_eq!(
            Fixed::from_le_bytes((-7_i32).to_le_bytes()).to_le_bytes(),
            (-7_i32).to_le_bytes()
        );
    }

    #[test]
    fn to_f32_is_subpixel_aware() {
        let half = Fixed::from_ratio(1, 2);
        assert!((half.to_f32() - 0.5).abs() < 1e-6);
    }
}
