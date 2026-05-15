pub mod dimension;
pub mod fixed;
pub mod transform;
pub mod transform_3d;
pub mod viewport;

pub use dimension::Dimension;
pub use fixed::{Fixed, Fixed64};
pub use transform::{Transform, TransformClass};
pub use transform_3d::Transform3D;
pub use viewport::Viewport;

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

    pub fn new(x: impl Into<Fixed>, y: impl Into<Fixed>) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
        }
    }

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

    pub fn bounding_quad(q: &[Point; 4]) -> Rect {
        let mut min_x = q[0].x;
        let mut max_x = q[0].x;
        let mut min_y = q[0].y;
        let mut max_y = q[0].y;
        for p in &q[1..] {
            if p.x < min_x {
                min_x = p.x;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.y > max_y {
                max_y = p.y;
            }
        }
        Self {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        }
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let ax1 = self.x + self.w;
        let ay1 = self.y + self.h;
        let bx1 = other.x + other.w;
        let by1 = other.y + other.h;
        let x0 = if self.x < other.x { self.x } else { other.x };
        let y0 = if self.y < other.y { self.y } else { other.y };
        let x1 = if ax1 > bx1 { ax1 } else { bx1 };
        let y1 = if ay1 > by1 { ay1 } else { by1 };
        Rect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
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

    /// Linear interpolation in 8-bit channel space. `t` is clamped to
    /// [0, 1]; t=0 returns `a`, t=1 returns `b`.
    pub fn lerp(a: Color, b: Color, t: Fixed) -> Color {
        let t = t.clamp(Fixed::ZERO, Fixed::ONE);
        let one_minus_t = Fixed::ONE - t;
        let mix = |ca: u8, cb: u8| -> u8 {
            let v = Fixed::from_int(ca as i32) * one_minus_t + Fixed::from_int(cb as i32) * t;
            v.to_int().clamp(0, 255) as u8
        };
        Color {
            r: mix(a.r, b.r),
            g: mix(a.g, b.g),
            b: mix(a.b, b.b),
            a: mix(a.a, b.a),
        }
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

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn lerp_endpoints() {
        let a = Color::rgb(0, 100, 200);
        let b = Color::rgb(200, 50, 100);
        assert_eq!(Color::lerp(a, b, Fixed::ZERO), a);
        assert_eq!(Color::lerp(a, b, Fixed::ONE), b);
    }

    #[test]
    fn lerp_midpoint() {
        let a = Color::rgb(0, 0, 0);
        let b = Color::rgb(200, 100, 60);
        let mid = Color::lerp(a, b, Fixed::ONE / 2);
        assert!((mid.r as i32 - 100).abs() <= 1);
        assert!((mid.g as i32 - 50).abs() <= 1);
        assert!((mid.b as i32 - 30).abs() <= 1);
    }

    #[test]
    fn lerp_clamps_oob_t() {
        let a = Color::rgb(50, 50, 50);
        let b = Color::rgb(200, 200, 200);
        assert_eq!(Color::lerp(a, b, Fixed::from_int(-5)), a);
        assert_eq!(Color::lerp(a, b, Fixed::from_int(5)), b);
    }
}
