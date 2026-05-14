use crate::types::Fixed;

pub type EaseFn = fn(Fixed) -> Fixed;

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
