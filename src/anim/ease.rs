use crate::types::Fixed;

pub type EaseFn = fn(Fixed) -> Fixed;
pub type EaseDerivFn = fn(Fixed) -> Fixed;

/// Bundles an ease function with its derivative and precomputed arc length.
#[derive(Clone, Copy)]
pub struct EaseCurve {
    pub eval: EaseFn,
    pub derivative: EaseDerivFn,
    pub arc_length: Fixed,
}

impl EaseCurve {
    pub const fn new(eval: EaseFn, derivative: EaseDerivFn, arc_length: Fixed) -> Self {
        Self {
            eval,
            derivative,
            arc_length,
        }
    }
}

// ─── Ease functions ────────────────────────────────────────────────────

pub fn linear(t: Fixed) -> Fixed {
    t
}

pub fn linear_d(_t: Fixed) -> Fixed {
    Fixed::ONE
}

pub fn ease_in_quad(t: Fixed) -> Fixed {
    t * t
}

pub fn ease_in_quad_d(t: Fixed) -> Fixed {
    t * Fixed::from_int(2)
}

pub fn ease_out_quad(t: Fixed) -> Fixed {
    let inv = Fixed::ONE - t;
    Fixed::ONE - inv * inv
}

pub fn ease_out_quad_d(t: Fixed) -> Fixed {
    (Fixed::ONE - t) * Fixed::from_int(2)
}

pub fn ease_in_out_quad(t: Fixed) -> Fixed {
    if t < Fixed::HALF {
        t * t * Fixed::from_int(2)
    } else {
        let inv = Fixed::ONE - t;
        Fixed::ONE - inv * inv * Fixed::from_int(2)
    }
}

pub fn ease_in_out_quad_d(t: Fixed) -> Fixed {
    if t < Fixed::HALF {
        t * Fixed::from_int(4)
    } else {
        (Fixed::ONE - t) * Fixed::from_int(4)
    }
}

pub fn ease_in_cubic(t: Fixed) -> Fixed {
    t * t * t
}

pub fn ease_in_cubic_d(t: Fixed) -> Fixed {
    t * t * Fixed::from_int(3)
}

pub fn ease_out_cubic(t: Fixed) -> Fixed {
    let inv = Fixed::ONE - t;
    Fixed::ONE - inv * inv * inv
}

pub fn ease_out_cubic_d(t: Fixed) -> Fixed {
    let inv = Fixed::ONE - t;
    inv * inv * Fixed::from_int(3)
}

pub fn ease_in_out_cubic(t: Fixed) -> Fixed {
    if t < Fixed::HALF {
        Fixed::from_int(4) * t * t * t
    } else {
        let p = Fixed::from_int(2) * t - Fixed::from_int(2);
        Fixed::ONE + p * p * p / Fixed::from_int(2)
    }
}

pub fn ease_in_out_cubic_d(t: Fixed) -> Fixed {
    if t < Fixed::HALF {
        Fixed::from_int(12) * t * t
    } else {
        let p = Fixed::from_int(2) * t - Fixed::from_int(2);
        p * p * Fixed::from_int(6)
    }
}

// ─── Precomputed EaseCurve constants ───────────────────────────────────
// Arc lengths computed by numerical integration:
//   L = ∫₀¹ sqrt(1 + f'(t)²) dt  (32-step Simpson)
// These are Fixed Q24.8 raw values.

pub const LINEAR: EaseCurve = EaseCurve::new(linear, linear_d, Fixed::from_raw(362));
pub const IN_QUAD: EaseCurve = EaseCurve::new(ease_in_quad, ease_in_quad_d, Fixed::from_raw(379));
pub const OUT_QUAD: EaseCurve =
    EaseCurve::new(ease_out_quad, ease_out_quad_d, Fixed::from_raw(379));
pub const IN_OUT_QUAD: EaseCurve =
    EaseCurve::new(ease_in_out_quad, ease_in_out_quad_d, Fixed::from_raw(379));
pub const IN_CUBIC: EaseCurve =
    EaseCurve::new(ease_in_cubic, ease_in_cubic_d, Fixed::from_raw(396));
pub const OUT_CUBIC: EaseCurve =
    EaseCurve::new(ease_out_cubic, ease_out_cubic_d, Fixed::from_raw(396));
pub const IN_OUT_CUBIC: EaseCurve =
    EaseCurve::new(ease_in_out_cubic, ease_in_out_cubic_d, Fixed::from_raw(512));

#[cfg(test)]
mod tests {
    use super::*;

    fn arc_length_simpson(f_deriv: EaseDerivFn, n: usize) -> f32 {
        let h = 1.0 / n as f32;
        let integrand = |t: f32| -> f32 {
            let d = f_deriv(Fixed::from_f32(t)).to_f32();
            (1.0 + d * d).sqrt()
        };
        let mut sum = integrand(0.0) + integrand(1.0);
        for i in 1..n {
            let t = i as f32 * h;
            let coeff = if i % 2 == 0 { 2.0 } else { 4.0 };
            sum += coeff * integrand(t);
        }
        sum * h / 3.0
    }

    #[test]
    fn derivative_spot_checks() {
        let eps = 0.05;
        assert!((linear_d(Fixed::HALF).to_f32() - 1.0).abs() < eps);
        assert!((ease_in_quad_d(Fixed::HALF).to_f32() - 1.0).abs() < eps);
        assert!((ease_in_quad_d(Fixed::ONE).to_f32() - 2.0).abs() < eps);
        assert!((ease_out_quad_d(Fixed::ZERO).to_f32() - 2.0).abs() < eps);
        assert!((ease_out_quad_d(Fixed::ONE).to_f32() - 0.0).abs() < eps);
        assert!((ease_in_cubic_d(Fixed::ONE).to_f32() - 3.0).abs() < eps);
        assert!((ease_out_cubic_d(Fixed::ZERO).to_f32() - 3.0).abs() < eps);
        let d_quarter = ease_in_out_cubic_d(Fixed::from_raw(64)).to_f32();
        assert!((d_quarter - 0.75).abs() < 0.1, "d(0.25)={d_quarter}");
        let d_three_quarter = ease_in_out_cubic_d(Fixed::from_raw(192)).to_f32();
        assert!(
            (d_three_quarter - 1.5).abs() < 0.1,
            "d(0.75)={d_three_quarter}"
        );
    }

    #[test]
    fn arc_length_constants_reasonable() {
        let cases: &[(EaseDerivFn, Fixed, &str)] = &[
            (linear_d, LINEAR.arc_length, "linear"),
            (ease_in_quad_d, IN_QUAD.arc_length, "in_quad"),
            (ease_out_cubic_d, OUT_CUBIC.arc_length, "out_cubic"),
            (ease_in_out_cubic_d, IN_OUT_CUBIC.arc_length, "in_out_cubic"),
        ];
        for &(df, stored, name) in cases {
            let computed = arc_length_simpson(df, 64);
            let stored_f = stored.to_f32();
            let err = (computed - stored_f).abs() / computed;
            assert!(
                err < 0.05,
                "{name} arc_length: stored={stored_f}, computed={computed}, err={err:.3}"
            );
        }
    }
}
