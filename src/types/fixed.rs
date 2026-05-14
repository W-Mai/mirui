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
    pub const HALF: Self = Self(SCALE / 2);
    pub const ONE: Self = Self(SCALE);
    pub const MAX: Self = Self(i32::MAX);
    pub const MIN: Self = Self(i32::MIN);
    pub const PI: Self = Self(804); // round(π * 256) = 804, error ≈ 0.001

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

    #[inline]
    pub const fn is_integer(self) -> bool {
        self.0 & (SCALE - 1) == 0
    }

    #[inline]
    pub const fn fract(self) -> Self {
        Self(self.0 & (SCALE - 1))
    }

    pub fn map_range(
        self,
        from: (impl Into<Self>, impl Into<Self>),
        to: (impl Into<Self>, impl Into<Self>),
    ) -> Self {
        let (from_min, from_max) = (from.0.into(), from.1.into());
        let (to_min, to_max) = (to.0.into(), to.1.into());
        let t = (self - from_min) / (from_max - from_min);
        to_min + t * (to_max - to_min)
    }

    /// Map from [0..ONE] to [0..to]
    pub fn map01(self, to: impl Into<Self>) -> Self {
        self * to.into()
    }

    /// Square root. For Fixed value v = raw/256, result is sqrt(v) in Fixed.
    /// Algebra: sqrt(raw/256) = sqrt(raw)/16 → as raw, we need sqrt(raw << 8).
    /// Use u64 for the shifted value to avoid overflow at raw > i32::MAX >> 8.
    pub fn sqrt(self) -> Self {
        if self.0 <= 0 {
            return Self::ZERO;
        }
        let n = (self.0 as u64) << FRAC_BITS;
        let mut x = n;
        let mut y = x.div_ceil(2);
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        Self(x as i32)
    }

    pub fn sin_deg(angle_deg: Self) -> Self {
        let rad = angle_deg * Self::PI / Self::from_int(180);
        sin_rad(rad)
    }

    pub fn cos_deg(angle_deg: Self) -> Self {
        Self::sin_deg(Self::from_int(90) - angle_deg)
    }
}

fn sin_rad(x: Fixed) -> Fixed {
    let two_pi = Fixed::PI * 2;
    let mut a = x;
    while a < Fixed::ZERO {
        a += two_pi;
    }
    while a >= two_pi {
        a -= two_pi;
    }

    let half_pi = Fixed::PI / 2;
    let (mut a, sign) = if a < half_pi {
        (a, 1)
    } else if a < Fixed::PI {
        (Fixed::PI - a, 1)
    } else if a < half_pi * 3 {
        (a - Fixed::PI, -1)
    } else {
        (two_pi - a, -1)
    };

    let quarter_pi = Fixed::PI / 4;
    let use_cos = a > quarter_pi;
    if use_cos {
        a = half_pi - a;
    }

    // Taylor: sin(x) = x - x³/6 + x⁵/120 - x⁷/5040 + ...
    //         cos(x) = 1 - x²/2 + x⁴/24 - x⁶/720 + ...
    let x2 = a * a;
    let result = if use_cos {
        let x4 = x2 * x2;
        let x6 = x4 * x2;
        Fixed::ONE - x2 / 2 + x4 / 24 - x6 / 720
    } else {
        let x3 = a * x2;
        let x5 = x3 * x2;
        let x7 = x5 * x2;
        a - x3 / 6 + x5 / 120 - x7 / 5040
    };

    if sign < 0 { -result } else { result }
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
        // i64 intermediate: pure-i32 `(self.0 << 8) / rhs.0` overflows at
        // |self.0| > 2^23, which Dimension::Percent hits past ~328 px.
        Self((((self.0 as i64) << FRAC_BITS) / (rhs.0 as i64)) as i32)
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

/// Q48.16 fixed-point (i64 raw, 16 fractional bits). Used for intermediate
/// values that need more precision or range than [`Fixed`] can provide —
/// e.g. 3×3 homography matrix elements, distance-squared in quad rasterization.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct Fixed64(pub i64);

const FRAC_BITS_64: i64 = 16;
const SCALE_64: i64 = 1 << FRAC_BITS_64;

impl Fixed64 {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(SCALE_64);

    #[inline]
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn from_int(v: i64) -> Self {
        Self(v << FRAC_BITS_64)
    }

    #[inline]
    pub const fn to_int(self) -> i64 {
        self.0 >> FRAC_BITS_64
    }

    #[inline]
    pub const fn raw(self) -> i64 {
        self.0
    }

    #[inline]
    pub const fn abs(self) -> Self {
        if self.0 < 0 { Self(-self.0) } else { self }
    }

    /// Lift a Q24.8 Fixed to Q48.16 Fixed64 (shift left by 8).
    #[inline]
    pub const fn from_fixed(f: Fixed) -> Self {
        Self((f.0 as i64) << 8)
    }

    /// Narrow back to Q24.8 Fixed, saturating on overflow.
    #[inline]
    pub fn to_fixed(self) -> Fixed {
        let shifted = self.0 >> 8;
        let clamped = shifted.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        Fixed(clamped)
    }

    #[inline]
    pub fn from_f32(v: f32) -> Self {
        Self((v * SCALE_64 as f32) as i64)
    }

    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / SCALE_64 as f32
    }

    pub fn sqrt(self) -> Self {
        if self.0 <= 0 {
            return Self::ZERO;
        }
        let n = (self.0 as u128) << FRAC_BITS_64;
        let mut x = n;
        let mut y = x.div_ceil(2);
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        Self(x as i64)
    }
}

impl Add for Fixed64 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Fixed64 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Fixed64 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Fixed64 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Mul for Fixed64 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        // Plain i64 multiply, matching the pre-Fixed64 `q_mul`. Two Q48.16
        // raws squared fit i64 as long as each side stays under ±2^31,
        // which covers screen-scale matrices; callers that need the wider
        // margin use `mul_wide`.
        Self((self.0 * rhs.0) >> FRAC_BITS_64)
    }
}

impl Fixed64 {
    /// Q48.16 multiply with i128 intermediate. Pays the software-i128 cost
    /// on 32-bit targets but survives values past ±2^31.
    #[inline]
    pub fn mul_wide(self, rhs: Self) -> Self {
        Self((((self.0 as i128) * (rhs.0 as i128)) >> FRAC_BITS_64) as i64)
    }

    /// Q48.16 divide with i128 intermediate, matching `mul_wide`.
    #[inline]
    pub fn div_wide(self, rhs: Self) -> Self {
        Self((((self.0 as i128) << FRAC_BITS_64) / (rhs.0 as i128)) as i64)
    }
}

impl Div for Fixed64 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        // i64 intermediate same as Fixed::Div: `(self.0 << 16) / rhs.0`
        // overflows when |self.0| > 2^47, well past typical screen values.
        // Use `div_wide` if you hit the ceiling.
        Self((self.0 << FRAC_BITS_64) / rhs.0)
    }
}

impl Neg for Fixed64 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<i64> for Fixed64 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: i64) -> Self {
        Self(self.0 * rhs)
    }
}

impl Div<i64> for Fixed64 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: i64) -> Self {
        Self(self.0 / rhs)
    }
}

impl From<Fixed> for Fixed64 {
    #[inline]
    fn from(f: Fixed) -> Self {
        Self::from_fixed(f)
    }
}

impl From<Fixed64> for Fixed {
    #[inline]
    fn from(f: Fixed64) -> Self {
        f.to_fixed()
    }
}

impl fmt::Debug for Fixed64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixed64({})", self.to_f32())
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

    #[test]
    fn sqrt_returns_value_in_fixed_space() {
        // sqrt(25) must equal 5 in Fixed semantics, not 5/256 ≈ 0.02.
        assert_eq!(Fixed::from_int(25).sqrt().to_int(), 5);
        assert_eq!(Fixed::from_int(100).sqrt().to_int(), 10);
        // Non-square: sqrt(2) ≈ 1.414, expect within 1/256.
        let s = Fixed::from_int(2).sqrt().to_f32();
        assert!((s - core::f32::consts::SQRT_2).abs() < 0.01);
    }

    #[test]
    fn consts() {
        assert_eq!(Fixed::MAX.to_f32(), i32::MAX as f32 / 256.0);
        assert_eq!(Fixed::MIN.to_f32(), i32::MIN as f32 / 256.0);
        // PI: 3.14159 → Fixed raw 804 → 3.140625, error ~0.001
        assert!((Fixed::PI.to_f32() - core::f32::consts::PI).abs() < 0.002);
    }

    #[test]
    fn sin_deg_known_values() {
        let cases = [
            (0, 0.0f32),
            (30, 0.5),
            (45, 0.7071),
            (60, 0.8660),
            (90, 1.0),
            (180, 0.0),
            (270, -1.0),
            (360, 0.0),
        ];
        for (deg, expected) in cases {
            let got = Fixed::sin_deg(Fixed::from_int(deg)).to_f32();
            assert!(
                (got - expected).abs() < 0.01,
                "sin({deg}°) = {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn sin_deg_negative_and_large() {
        let a = Fixed::sin_deg(Fixed::from_int(-30)).to_f32();
        assert!((a - -0.5).abs() < 0.01);
        let b = Fixed::sin_deg(Fixed::from_int(390)).to_f32();
        assert!((b - 0.5).abs() < 0.01);
    }

    #[test]
    fn cos_deg_known_values() {
        let cases = [(0, 1.0f32), (60, 0.5), (90, 0.0), (180, -1.0), (270, 0.0)];
        for (deg, expected) in cases {
            let got = Fixed::cos_deg(Fixed::from_int(deg)).to_f32();
            assert!(
                (got - expected).abs() < 0.01,
                "cos({deg}°) = {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn sin_cos_pythagorean() {
        for deg in (0..360).step_by(15) {
            let s = Fixed::sin_deg(Fixed::from_int(deg));
            let c = Fixed::cos_deg(Fixed::from_int(deg));
            let sum = (s * s + c * c).to_f32();
            assert!((sum - 1.0).abs() < 0.03, "sin²+cos² at {deg}° = {sum}");
        }
    }

    #[test]
    fn div_does_not_overflow_for_large_dividend() {
        assert_eq!(
            (Fixed::from_int(64000) / Fixed::from_int(100)).to_int(),
            640
        );
    }

    #[test]
    fn div_precision_past_old_ceiling() {
        let q = (Fixed::from_int(1_000_000) / Fixed::from_int(7)).to_f32();
        assert!((q - 142857.142857).abs() < 1.0);
    }

    #[test]
    fn fixed64_from_to_int() {
        assert_eq!(Fixed64::from_int(42).to_int(), 42);
        assert_eq!(Fixed64::from_int(-7).to_int(), -7);
    }

    #[test]
    fn fixed64_arithmetic() {
        let a = Fixed64::from_int(10);
        let b = Fixed64::from_int(3);
        assert_eq!((a + b).to_int(), 13);
        assert_eq!((a - b).to_int(), 7);
        assert_eq!((a * b).to_int(), 30);
        let div = (a / b).to_f32();
        assert!((div - 10.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn fixed64_mul_preserves_small_values() {
        // 0.00125 · 800 = 1. Q24.8 (Fixed) rounds 0.00125 to 0, so the
        // product collapses; Q48.16 (Fixed64) keeps 82 raw (≈0.00125).
        let small = Fixed64::from_f32(0.00125);
        let big = Fixed64::from_int(800);
        assert!((small * big).to_f32() > 0.9);
    }

    #[test]
    fn fixed64_fixed_roundtrip() {
        let f = Fixed::from_f32(123.45);
        let wide = Fixed64::from_fixed(f);
        assert_eq!(wide.to_fixed(), f);
    }

    #[test]
    fn fixed64_mul_wide_avoids_i64_overflow() {
        // Plain Mul uses i64 intermediates and would wrap here; mul_wide
        // upcasts to i128 so 1_000_000² survives.
        let a = Fixed64::from_f32(1_000_000.0);
        let b = Fixed64::from_f32(1_000_000.0);
        let c = a.mul_wide(b);
        assert!((c.to_f32() - 1e12).abs() / 1e12 < 1e-3);
    }

    #[test]
    fn fixed64_sqrt_matches_fixed_within_tolerance() {
        let v = Fixed::from_int(169);
        let r_narrow = v.sqrt();
        let r_wide = Fixed64::from_fixed(v).sqrt().to_fixed();
        assert!((r_narrow - r_wide).abs().raw() < 4);
    }
}
