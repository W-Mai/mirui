use crate::prelude::{Fixed, Point};
use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

/// A point or a velocity on the plane, in fixed-point units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Vec2 {
    pub(crate) x: Fixed,
    pub(crate) y: Fixed,
}

impl Vec2 {
    pub(crate) fn new(x: Fixed, y: Fixed) -> Self {
        Self { x, y }
    }

    /// The unit vector pointing `degrees` clockwise from straight right.
    pub(crate) fn heading(degrees: Fixed) -> Self {
        Self::new(Fixed::cos_deg(degrees), Fixed::sin_deg(degrees))
    }

    pub(crate) fn length(self) -> Fixed {
        self.dot(self).sqrt()
    }

    pub(crate) fn dot(self, other: Self) -> Fixed {
        self.x * other.x + self.y * other.y
    }
}

impl From<Vec2> for Point {
    fn from(vector: Vec2) -> Self {
        Point::new(vector.x, vector.y)
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl Mul<Fixed> for Vec2 {
    type Output = Self;

    fn mul(self, factor: Fixed) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }
}

/// Dividing a vector by its own length keeps the unit vector precise.
impl Div<Fixed> for Vec2 {
    type Output = Self;

    fn div(self, divisor: Fixed) -> Self {
        Self::new(self.x / divisor, self.y / divisor)
    }
}
