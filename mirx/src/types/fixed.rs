//! 24.8 fixed-point number. High 24 bits = integer, low 8 = fraction.
//!
//! Wire layout: 4 bytes little-endian i32 (`raw`). This is the same
//! layout `mirui::types::Fixed` uses, so `From` conversions between
//! the two are a zero-cost `Fixed(v.0)` / `Fixed(v.0)` swap.
//!
//! mirx intentionally does **not** implement arithmetic on `Fixed`.
//! Consumers convert to their own `Fixed` (which has the `Add` / `Mul`
//! / `Sub` / etc. impls) via `From` and do the math there. Keeping
//! mirx arithmetic-free keeps the crate thin and lets each consumer
//! pick its own overflow / saturating policy.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed(pub i32);

impl Fixed {
    pub const ZERO: Self = Fixed(0);
    pub const ONE: Self = Fixed(256);

    pub const fn from_raw(raw: i32) -> Self {
        Fixed(raw)
    }

    pub const fn from_int(v: i32) -> Self {
        Fixed(v << 8)
    }

    pub const fn to_int(self) -> i32 {
        self.0 >> 8
    }

    pub const fn raw(self) -> i32 {
        self.0
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
        assert_eq!(Fixed::from_int(0).raw(), 0);
        assert_eq!(Fixed::from_int(1).raw(), 256);
        assert_eq!(Fixed::from_int(-1).raw(), -256);
        assert_eq!(Fixed::from_int(42).to_int(), 42);
    }

    #[test]
    fn raw_is_passthrough() {
        assert_eq!(Fixed::from_raw(1234).raw(), 1234);
        assert_eq!(Fixed::from_raw(-7).0, -7);
    }

    #[test]
    fn to_f32_is_subpixel_aware() {
        let half = Fixed::from_raw(128);
        assert!((half.to_f32() - 0.5).abs() < 1e-6);
    }
}
