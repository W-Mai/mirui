use crate::types::Fixed;

pub type EaseFn = fn(Fixed) -> Fixed;

/// An immutable easing curve. Reuse a `static` descriptor to retain its identity across configuration updates.
pub struct Easing {
    sample: EaseFn,
}

impl Easing {
    /// Declare a curve with normalized endpoints: `sample(0) == 0` and `sample(1) == 1`.
    pub const fn new(sample: EaseFn) -> Self {
        Self { sample }
    }

    pub fn sample(&self, progress: Fixed) -> Fixed {
        (self.sample)(progress)
    }
}

impl PartialEq for Easing {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self, other)
    }
}

impl Eq for Easing {}

pub static LINEAR: Easing = Easing::new(linear);
pub static IN_QUAD: Easing = Easing::new(ease_in_quad);
pub static OUT_QUAD: Easing = Easing::new(ease_out_quad);
pub static IN_OUT_QUAD: Easing = Easing::new(ease_in_out_quad);
pub static IN_CUBIC: Easing = Easing::new(ease_in_cubic);
pub static OUT_CUBIC: Easing = Easing::new(ease_out_cubic);
pub static IN_OUT_CUBIC: Easing = Easing::new(ease_in_out_cubic);

pub fn linear(t: Fixed) -> Fixed {
    t
}

pub fn ease_in_quad(t: Fixed) -> Fixed {
    t * t
}

pub fn ease_out_quad(t: Fixed) -> Fixed {
    let inv = Fixed::ONE - t;
    Fixed::ONE - inv * inv
}

pub fn ease_in_out_quad(t: Fixed) -> Fixed {
    if t < Fixed::HALF {
        t * t * Fixed::from_int(2)
    } else {
        let inv = Fixed::ONE - t;
        Fixed::ONE - inv * inv * Fixed::from_int(2)
    }
}

pub fn ease_in_cubic(t: Fixed) -> Fixed {
    t * t * t
}

pub fn ease_out_cubic(t: Fixed) -> Fixed {
    let inv = Fixed::ONE - t;
    Fixed::ONE - inv * inv * inv
}

pub fn ease_in_out_cubic(t: Fixed) -> Fixed {
    if t < Fixed::HALF {
        Fixed::from_int(4) * t * t * t
    } else {
        let p = Fixed::from_int(2) * t - Fixed::from_int(2);
        Fixed::ONE + p * p * p / Fixed::from_int(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_preserve_samples_and_static_identity() {
        static CUSTOM: Easing = Easing::new(ease_out_cubic);
        for (curve, sample) in [
            (&LINEAR, linear as EaseFn),
            (&IN_QUAD, ease_in_quad),
            (&OUT_QUAD, ease_out_quad),
            (&IN_OUT_QUAD, ease_in_out_quad),
            (&IN_CUBIC, ease_in_cubic),
            (&OUT_CUBIC, ease_out_cubic),
            (&IN_OUT_CUBIC, ease_in_out_cubic),
        ] {
            assert!(curve == core::hint::black_box(curve));
            assert!(curve != &CUSTOM);
            assert_eq!(curve.sample(Fixed::ZERO), Fixed::ZERO);
            assert_eq!(curve.sample(Fixed::ONE), Fixed::ONE);
            for step in 0..=256 {
                let t = Fixed::from_ratio(step, 256);
                assert_eq!(curve.sample(t), sample(t));
            }
        }
    }
}
