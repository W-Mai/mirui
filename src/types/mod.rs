pub mod dimension;
pub mod fixed;

pub use dimension::Dimension;
pub use fixed::Fixed;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: Fixed,
    pub y: Fixed,
}

impl Point {
    pub const ZERO: Self = Self {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    };

    pub fn floor(&self) -> (i32, i32) {
        (self.x.to_int(), self.y.to_int())
    }

    pub fn ceil(&self) -> (i32, i32) {
        (self.x.ceil().to_int(), self.y.ceil().to_int())
    }

    pub fn round(&self) -> (i32, i32) {
        (self.x.round().to_int(), self.y.round().to_int())
    }
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

    pub fn is_aligned(&self) -> bool {
        self.x.is_integer() && self.y.is_integer() && self.w.is_integer() && self.h.is_integer()
    }

    /// Pixel bounds: (x0, y0, x1, y1) — floor for top-left, ceil for bottom-right
    pub fn pixel_bounds(&self) -> (i32, i32, i32, i32) {
        (
            self.x.to_int(),
            self.y.to_int(),
            (self.x + self.w).ceil().to_int(),
            (self.y + self.h).ceil().to_int(),
        )
    }

    /// Convert to integer pixel rect that fully contains this rect
    pub fn to_px(&self) -> (i32, i32, u16, u16) {
        let (x0, y0, x1, y1) = self.pixel_bounds();
        (x0, y0, (x1 - x0) as u16, (y1 - y0) as u16)
    }

    /// Construct from integer values
    pub fn new(
        x: impl Into<Fixed>,
        y: impl Into<Fixed>,
        w: impl Into<Fixed>,
        h: impl Into<Fixed>,
    ) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            w: w.into(),
            h: h.into(),
        }
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

#[derive(Clone, Copy, Debug)]
pub struct NormColor {
    pub r: Fixed,
    pub g: Fixed,
    pub b: Fixed,
    pub a: Fixed,
}

const F255: Fixed = Fixed::from_int(255);

impl From<Color> for NormColor {
    fn from(c: Color) -> Self {
        Self {
            r: Fixed::from_int(c.r as i32) / F255,
            g: Fixed::from_int(c.g as i32) / F255,
            b: Fixed::from_int(c.b as i32) / F255,
            a: Fixed::from_int(c.a as i32) / F255,
        }
    }
}

impl From<NormColor> for Color {
    fn from(nc: NormColor) -> Self {
        Self {
            r: nc.r.map01(F255).to_int() as u8,
            g: nc.g.map01(F255).to_int() as u8,
            b: nc.b.map01(F255).to_int() as u8,
            a: nc.a.map01(F255).to_int() as u8,
        }
    }
}

impl Color {
    pub fn normalized(&self) -> NormColor {
        NormColor::from(*self)
    }
}
