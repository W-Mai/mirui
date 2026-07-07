use super::Fixed;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: Fixed,
    pub y: Fixed,
}

impl Point {
    pub const ZERO: Self = Self {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    };

    pub const fn new(x: Fixed, y: Fixed) -> Self {
        Self { x, y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_constructible() {
        assert_eq!(Point::ZERO, Point::new(Fixed::ZERO, Fixed::ZERO));
    }
}
