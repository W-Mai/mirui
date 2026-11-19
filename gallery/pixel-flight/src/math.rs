//! Fixed-point scalar operations used by the flight scene.

use mirui::types::{Fixed, Fixed64};

pub const PI: Fixed = Fixed::PI;
pub const TAU: Fixed = Fixed::from_ratio(1608, 256);
pub const SQRT_2: Fixed = Fixed::from_ratio(362, 256);

/// Polynomial series use Q48.16 to retain terms below one Q24.8 unit.
/// Scene state and returned screen-scale values remain Q24.8.
pub trait FixedMath {
    fn sin(self) -> Self;
    fn cos(self) -> Self;
    fn sin_cos(self) -> (Self, Self)
    where
        Self: Sized;
    fn tan(self) -> Self;
    fn atan2(self, x: Self) -> Self;
    fn exp(self) -> Self;
    fn ln(self) -> Self;
    fn powi(self, n: i32) -> Self;
    fn pow_fraction(self, n: Self) -> Self;
    fn signum(self) -> Self;
    fn rem_euclid(self, divisor: Self) -> Self;
    fn to_radians(self) -> Self;
    fn to_degrees(self) -> Self;
}

impl FixedMath for Fixed {
    fn sin(self) -> Self {
        Fixed::sin_rad(self)
    }
    fn cos(self) -> Self {
        Fixed::cos_rad(self)
    }
    fn sin_cos(self) -> (Self, Self) {
        (self.sin(), self.cos())
    }
    fn tan(self) -> Self {
        let cosine = self.cos();
        if cosine.is_zero() {
            return Fixed::ZERO;
        }
        self.sin() / cosine
    }
    fn atan2(self, x: Self) -> Self {
        Fixed::atan2(self, x)
    }
    fn exp(self) -> Self {
        if self < Fixed::from_int(-16) {
            return Fixed::ZERO;
        }
        if self <= Fixed::ZERO {
            let x = -self * 16;
            let index = x.to_int() as usize;
            if index == 256 {
                return EXP_NEG[256];
            }
            return EXP_NEG[index] + (EXP_NEG[index + 1] - EXP_NEG[index]) * x.fract();
        }
        if self > Fixed::from_int(15) {
            return Fixed::MAX;
        }
        let x = Fixed64::from_fixed(self);
        let ln2 = Fixed64::from_ratio(45426, 65536);
        let power = (x / ln2).to_int();
        let r = x - ln2 * power;
        let mut term = Fixed64::ONE;
        let mut sum = term;
        for n in 1..=12 {
            term = term * r / n;
            sum += term;
        }
        if power >= 0 {
            (sum * (1i64 << power)).to_fixed()
        } else {
            (sum / (1i64 << -power)).to_fixed()
        }
    }
    fn ln(self) -> Self {
        assert!(self > Fixed::ZERO, "positive logarithm input");
        let mut x = Fixed64::from_fixed(self);
        let mut power = 0i64;
        while x >= Fixed64::from_int(2) {
            x = x / 2;
            power += 1;
        }
        while x < Fixed64::ONE {
            x = x * 2;
            power -= 1;
        }
        let z = (x - Fixed64::ONE) / (x + Fixed64::ONE);
        let z2 = z * z;
        let mut term = z;
        let mut sum = z;
        for n in (3..=25).step_by(2) {
            term = term * z2;
            sum += term / n;
        }
        (sum * 2 + Fixed64::from_ratio(45426, 65536) * power).to_fixed()
    }
    fn powi(self, n: i32) -> Self {
        let mut base = self;
        let mut count = n.unsigned_abs();
        let mut result = Fixed::ONE;
        while count != 0 {
            if count & 1 != 0 {
                result = result * base;
            }
            count >>= 1;
            if count != 0 {
                base = base * base;
            }
        }
        if n < 0 { Fixed::ONE / result } else { result }
    }
    fn pow_fraction(self, n: Self) -> Self {
        if self == Fixed::ZERO {
            return if n == Fixed::ZERO {
                Fixed::ONE
            } else {
                Fixed::ZERO
            };
        }
        (self.ln() * n).exp()
    }
    fn signum(self) -> Self {
        if self < Fixed::ZERO {
            -Fixed::ONE
        } else if self > Fixed::ZERO {
            Fixed::ONE
        } else {
            Fixed::ZERO
        }
    }
    fn rem_euclid(self, divisor: Self) -> Self {
        self - (self / divisor).floor() * divisor
    }
    fn to_radians(self) -> Self {
        self * PI / 180
    }
    fn to_degrees(self) -> Self {
        self * 180 / PI
    }
}

// Immutable exponential samples; interpolation stays in Q24.8.
const EXP_NEG: [Fixed; 257] = [
    Fixed::from_ratio(256, 256),
    Fixed::from_ratio(240, 256),
    Fixed::from_ratio(226, 256),
    Fixed::from_ratio(212, 256),
    Fixed::from_ratio(199, 256),
    Fixed::from_ratio(187, 256),
    Fixed::from_ratio(176, 256),
    Fixed::from_ratio(165, 256),
    Fixed::from_ratio(155, 256),
    Fixed::from_ratio(146, 256),
    Fixed::from_ratio(137, 256),
    Fixed::from_ratio(129, 256),
    Fixed::from_ratio(121, 256),
    Fixed::from_ratio(114, 256),
    Fixed::from_ratio(107, 256),
    Fixed::from_ratio(100, 256),
    Fixed::from_ratio(94, 256),
    Fixed::from_ratio(88, 256),
    Fixed::from_ratio(83, 256),
    Fixed::from_ratio(78, 256),
    Fixed::from_ratio(73, 256),
    Fixed::from_ratio(69, 256),
    Fixed::from_ratio(65, 256),
    Fixed::from_ratio(61, 256),
    Fixed::from_ratio(57, 256),
    Fixed::from_ratio(54, 256),
    Fixed::from_ratio(50, 256),
    Fixed::from_ratio(47, 256),
    Fixed::from_ratio(44, 256),
    Fixed::from_ratio(42, 256),
    Fixed::from_ratio(39, 256),
    Fixed::from_ratio(37, 256),
    Fixed::from_ratio(35, 256),
    Fixed::from_ratio(33, 256),
    Fixed::from_ratio(31, 256),
    Fixed::from_ratio(29, 256),
    Fixed::from_ratio(27, 256),
    Fixed::from_ratio(25, 256),
    Fixed::from_ratio(24, 256),
    Fixed::from_ratio(22, 256),
    Fixed::from_ratio(21, 256),
    Fixed::from_ratio(20, 256),
    Fixed::from_ratio(19, 256),
    Fixed::from_ratio(17, 256),
    Fixed::from_ratio(16, 256),
    Fixed::from_ratio(15, 256),
    Fixed::from_ratio(14, 256),
    Fixed::from_ratio(14, 256),
    Fixed::from_ratio(13, 256),
    Fixed::from_ratio(12, 256),
    Fixed::from_ratio(11, 256),
    Fixed::from_ratio(11, 256),
    Fixed::from_ratio(10, 256),
    Fixed::from_ratio(9, 256),
    Fixed::from_ratio(9, 256),
    Fixed::from_ratio(8, 256),
    Fixed::from_ratio(8, 256),
    Fixed::from_ratio(7, 256),
    Fixed::from_ratio(7, 256),
    Fixed::from_ratio(6, 256),
    Fixed::from_ratio(6, 256),
    Fixed::from_ratio(6, 256),
    Fixed::from_ratio(5, 256),
    Fixed::from_ratio(5, 256),
    Fixed::from_ratio(5, 256),
    Fixed::from_ratio(4, 256),
    Fixed::from_ratio(4, 256),
    Fixed::from_ratio(4, 256),
    Fixed::from_ratio(4, 256),
    Fixed::from_ratio(3, 256),
    Fixed::from_ratio(3, 256),
    Fixed::from_ratio(3, 256),
    Fixed::from_ratio(3, 256),
    Fixed::from_ratio(3, 256),
    Fixed::from_ratio(3, 256),
    Fixed::from_ratio(2, 256),
    Fixed::from_ratio(2, 256),
    Fixed::from_ratio(2, 256),
    Fixed::from_ratio(2, 256),
    Fixed::from_ratio(2, 256),
    Fixed::from_ratio(2, 256),
    Fixed::from_ratio(2, 256),
    Fixed::from_ratio(2, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(1, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
    Fixed::from_ratio(0, 256),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_operations_preserve_domain_and_precision() {
        for n in -8..=8 {
            let x = Fixed::from_ratio(n, 4);
            assert!((x.exp().ln() - x).abs() <= Fixed::from_ratio(1, 16));
        }
        assert!(
            (Fixed::from_int(4).pow_fraction(Fixed::HALF) - Fixed::from_int(2)).abs()
                <= Fixed::from_ratio(1, 64)
        );
        assert_eq!((-Fixed::HALF).rem_euclid(Fixed::ONE), Fixed::HALF);
    }
}
