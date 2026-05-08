use core::fmt;
use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// 24.8 fixed-point number.
/// High 24 bits = integer part, low 8 bits = fractional part.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed(pub i32);

const FRAC_BITS: i32 = 8;
const SCALE: i32 = 1 << FRAC_BITS; // 256

impl Fixed {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(SCALE);

    #[inline]
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn from_int(v: i32) -> Self {
        Self(v << FRAC_BITS)
    }

    #[inline]
    pub const fn to_int(self) -> i32 {
        self.0 >> FRAC_BITS
    }

    /// Round to nearest integer, returning Fixed
    #[inline]
    pub const fn round(self) -> Self {
        Self((self.0 + (SCALE >> 1)) & !(SCALE - 1))
    }

    /// Round up (ceiling), returning Fixed
    #[inline]
    pub const fn ceil(self) -> Self {
        Self((self.0 + SCALE - 1) & !(SCALE - 1))
    }

    /// Round down (floor), returning Fixed
    #[inline]
    pub const fn floor(self) -> Self {
        Self(self.0 & !(SCALE - 1))
    }

    #[inline]
    pub fn from_f32(v: f32) -> Self {
        Self((v * SCALE as f32) as i32)
    }

    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / SCALE as f32
    }

    #[inline]
    pub const fn raw(self) -> i32 {
        self.0
    }

    #[inline]
    pub const fn abs(self) -> Self {
        if self.0 < 0 { Self(-self.0) } else { self }
    }
}

impl Add for Fixed {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Fixed {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Fixed {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Fixed {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Mul for Fixed {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        // Split to avoid i64: a * b = a * b_int + (a * b_frac) >> 8
        let b_int = rhs.0 >> FRAC_BITS;
        let b_frac = rhs.0 & (SCALE - 1);
        Self(self.0 * b_int + ((self.0 * b_frac) >> FRAC_BITS))
    }
}

impl Div for Fixed {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        // (a << 8) / b — safe for UI range (≤8192px)
        Self((self.0 << FRAC_BITS) / rhs.0)
    }
}

impl Neg for Fixed {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<i32> for Fixed {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: i32) -> Self {
        Self(self.0 * rhs)
    }
}

impl Div<i32> for Fixed {
    type Output = Self;
    #[inline]
    fn div(self, rhs: i32) -> Self {
        Self(self.0 / rhs)
    }
}

impl From<i32> for Fixed {
    #[inline]
    fn from(v: i32) -> Self {
        Self::from_int(v)
    }
}

impl From<u16> for Fixed {
    #[inline]
    fn from(v: u16) -> Self {
        Self::from_int(v as i32)
    }
}

impl From<u32> for Fixed {
    #[inline]
    fn from(v: u32) -> Self {
        Self::from_int(v as i32)
    }
}

impl From<f32> for Fixed {
    #[inline]
    fn from(v: f32) -> Self {
        Self::from_f32(v)
    }
}

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixed({})", self.to_f32())
    }
}

impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show as decimal: integer.fraction
        let int = self.0 >> FRAC_BITS;
        let frac = (self.0 & (SCALE - 1)).unsigned_abs();
        if frac == 0 {
            write!(f, "{int}")
        } else {
            // Convert fraction to decimal (2 digits)
            let decimal = (frac * 100) / SCALE as u32;
            write!(f, "{int}.{decimal:02}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_int_roundtrip() {
        assert_eq!(Fixed::from_int(42).to_int(), 42);
        assert_eq!(Fixed::from_int(-7).to_int(), -7);
        assert_eq!(Fixed::from_int(0).to_int(), 0);
    }

    #[test]
    fn from_f32_roundtrip() {
        let f = Fixed::from_f32(1.5);
        assert_eq!(f.0, 384); // 1.5 * 256
        assert_eq!(f.to_int(), 1);
        assert_eq!(f.round().to_int(), 2);
    }

    #[test]
    fn add_sub() {
        let a = Fixed::from_int(10);
        let b = Fixed::from_int(3);
        assert_eq!((a + b).to_int(), 13);
        assert_eq!((a - b).to_int(), 7);
    }

    #[test]
    fn mul_fixed() {
        let a = Fixed::from_f32(2.5);
        let b = Fixed::from_f32(4.0);
        let c = a * b;
        assert_eq!(c.to_int(), 10);
    }

    #[test]
    fn div_fixed() {
        let a = Fixed::from_int(10);
        let b = Fixed::from_int(4);
        let c = a / b;
        assert_eq!(c.0, 640); // 2.5 * 256
    }

    #[test]
    fn mul_int() {
        let a = Fixed::from_f32(1.5);
        assert_eq!((a * 3).to_int(), 4); // 1.5 * 3 = 4.5 → 4
        assert_eq!((a * 3).round().to_int(), 5); // rounds to 5
    }

    #[test]
    fn neg() {
        let a = Fixed::from_int(5);
        assert_eq!((-a).to_int(), -5);
    }

    #[test]
    fn ord() {
        let a = Fixed::from_f32(1.5);
        let b = Fixed::from_int(2);
        assert!(a < b);
        assert!(b > a);
    }

    #[test]
    fn display() {
        let a = Fixed::from_int(42);
        assert_eq!(alloc::format!("{a}"), "42");
        let b = Fixed::from_f32(1.5);
        assert_eq!(alloc::format!("{b}"), "1.50");
    }
}
