use super::Fixed;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: Fixed,
    pub y: Fixed,
    pub w: Fixed,
    pub h: Fixed,
}

impl Rect {
    pub const ZERO: Self = Self {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::ZERO,
        h: Fixed::ZERO,
    };

    pub const fn new(x: Fixed, y: Fixed, w: Fixed, h: Fixed) -> Self {
        Self { x, y, w, h }
    }
}
