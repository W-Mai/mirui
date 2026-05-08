pub mod dimension;
pub mod fixed;

pub use dimension::Dimension;
pub use fixed::Fixed;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);
        if x1 < x2 && y1 < y2 {
            Some(Rect {
                x: x1,
                y: y1,
                w: x2 - x1,
                h: y2 - y1,
            })
        } else {
            None
        }
    }

    /// Convert to integer pixel rect (for rendering)
    pub fn to_px(&self) -> (i32, i32, u16, u16) {
        (
            self.x.to_int(),
            self.y.to_int(),
            self.w.to_int() as u16,
            self.h.to_int() as u16,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

pub type Opa = u8;
