use crate::types::{Color, Fixed, NormColor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorFormat {
    RGB565,
    RGB565Swapped,
    RGB888,
    ARGB8888,
}

impl ColorFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::RGB565 | Self::RGB565Swapped => 2,
            Self::RGB888 => 3,
            Self::ARGB8888 => 4,
        }
    }
}

pub struct Texture<'a> {
    pub buf: &'a mut [u8],
    pub width: u16,
    pub height: u16,
    pub format: ColorFormat,
    pub stride: usize,
}

impl<'a> Texture<'a> {
    pub fn new(buf: &'a mut [u8], width: u16, height: u16, format: ColorFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        Self {
            buf,
            width,
            height,
            format,
            stride,
        }
    }

    fn offset(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(y as usize * self.stride + x as usize * self.format.bytes_per_pixel())
    }

    pub fn get_pixel(&self, x: i32, y: i32) -> Color {
        let Some(i) = self.offset(x, y) else {
            return Color::rgb(0, 0, 0);
        };
        match self.format {
            ColorFormat::ARGB8888 => Color::rgba(
                self.buf[i],
                self.buf[i + 1],
                self.buf[i + 2],
                self.buf[i + 3],
            ),
            ColorFormat::RGB888 => Color::rgb(self.buf[i], self.buf[i + 1], self.buf[i + 2]),
            ColorFormat::RGB565 => {
                let lo = self.buf[i] as u16;
                let hi = self.buf[i + 1] as u16;
                let px = lo | (hi << 8);
                Color::rgb(
                    ((px >> 11) as u8) << 3,
                    (((px >> 5) & 0x3F) as u8) << 2,
                    ((px & 0x1F) as u8) << 3,
                )
            }
            ColorFormat::RGB565Swapped => {
                let hi = self.buf[i] as u16;
                let lo = self.buf[i + 1] as u16;
                let px = lo | (hi << 8);
                Color::rgb(
                    ((px >> 11) as u8) << 3,
                    (((px >> 5) & 0x3F) as u8) << 2,
                    ((px & 0x1F) as u8) << 3,
                )
            }
        }
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, color: &Color) {
        let Some(i) = self.offset(x, y) else { return };
        match self.format {
            ColorFormat::ARGB8888 => {
                self.buf[i] = color.r;
                self.buf[i + 1] = color.g;
                self.buf[i + 2] = color.b;
                self.buf[i + 3] = color.a;
            }
            ColorFormat::RGB888 => {
                self.buf[i] = color.r;
                self.buf[i + 1] = color.g;
                self.buf[i + 2] = color.b;
            }
            ColorFormat::RGB565 | ColorFormat::RGB565Swapped => {
                let px = ((color.r as u16 >> 3) << 11)
                    | ((color.g as u16 >> 2) << 5)
                    | (color.b as u16 >> 3);
                let (b0, b1) = if self.format == ColorFormat::RGB565 {
                    (px as u8, (px >> 8) as u8)
                } else {
                    ((px >> 8) as u8, px as u8)
                };
                self.buf[i] = b0;
                self.buf[i + 1] = b1;
            }
        }
    }

    pub fn blend_pixel(&mut self, x: Fixed, y: Fixed, color: &Color, opa: u8) {
        if opa == 0 {
            return;
        }

        if x.is_integer() && y.is_integer() {
            let a = ((color.a as u16) * (opa as u16) / 255) as u8;
            self.blend_pixel_int(x.to_int(), y.to_int(), color, a);
            return;
        }

        let ix = x.to_int();
        let iy = y.to_int();
        let fx = x.fract();
        let fy = y.fract();
        let lx = Fixed::ONE - fx;
        let ty = Fixed::ONE - fy;

        let nc = color.normalized();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let base_a = nc.a * opa_norm;
        let to_alpha = |cov: Fixed| -> u8 { (base_a * cov).map01(255).to_int() as u8 };

        self.blend_pixel_int(ix, iy, color, to_alpha(lx * ty));
        self.blend_pixel_int(ix + 1, iy, color, to_alpha(fx * ty));
        self.blend_pixel_int(ix, iy + 1, color, to_alpha(lx * fy));
        self.blend_pixel_int(ix + 1, iy + 1, color, to_alpha(fx * fy));
    }

    fn blend_pixel_int(&mut self, x: i32, y: i32, color: &Color, a: u8) {
        if a == 0 {
            return;
        }
        if a == 255 {
            self.set_pixel(x, y, color);
            return;
        }
        let src = color.normalized();
        let dst = self.get_pixel(x, y).normalized();
        let t = Fixed::from_int(a as i32).map_range(
            (Fixed::ZERO, Fixed::from_int(255)),
            (Fixed::ZERO, Fixed::ONE),
        );
        let inv = Fixed::ONE - t;
        let blended = NormColor {
            r: src.r * t + dst.r * inv,
            g: src.g * t + dst.g * inv,
            b: src.b * t + dst.b * inv,
            a: Fixed::ONE,
        };
        self.set_pixel(x, y, &Color::from(blended));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb8888_roundtrip() {
        let mut buf = [0u8; 4];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::ARGB8888);
        let c = Color::rgba(100, 200, 50, 255);
        tex.set_pixel(0, 0, &c);
        assert_eq!(tex.get_pixel(0, 0), c);
    }

    #[test]
    fn rgb565_roundtrip() {
        let mut buf = [0u8; 2];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGB565);
        let c = Color::rgb(248, 252, 248); // values that survive 565 truncation
        tex.set_pixel(0, 0, &c);
        let got = tex.get_pixel(0, 0);
        assert_eq!(got.r, c.r);
        assert_eq!(got.g, c.g);
        assert_eq!(got.b, c.b);
    }

    #[test]
    fn blend_50_percent() {
        let mut buf = [0u8; 4];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::ARGB8888);
        tex.set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.blend_pixel(Fixed::ZERO, Fixed::ZERO, &Color::rgb(200, 100, 50), 128);
        let got = tex.get_pixel(0, 0);
        assert!((got.r as i32 - 100).abs() <= 1);
        assert!((got.g as i32 - 50).abs() <= 1);
        assert!((got.b as i32 - 25).abs() <= 1);
    }

    #[test]
    fn blend_rgb565() {
        let mut buf = [0u8; 2];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGB565);
        tex.set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.blend_pixel(Fixed::ZERO, Fixed::ZERO, &Color::rgb(255, 255, 255), 255);
        let got = tex.get_pixel(0, 0);
        assert_eq!(got.r, 248);
        assert_eq!(got.g, 252);
        assert_eq!(got.b, 248);
    }

    #[test]
    fn blend_subpixel_spreads_to_neighbors() {
        // A point at (0.5, 0.5) should spread to all 4 pixels
        let mut buf = [0u8; 4 * 4]; // 2x2 ARGB8888
        let mut tex = Texture::new(&mut buf, 2, 2, ColorFormat::ARGB8888);
        tex.set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.set_pixel(1, 0, &Color::rgb(0, 0, 0));
        tex.set_pixel(0, 1, &Color::rgb(0, 0, 0));
        tex.set_pixel(1, 1, &Color::rgb(0, 0, 0));

        tex.blend_pixel(
            Fixed::from_raw(128),
            Fixed::from_raw(128),
            &Color::rgb(255, 255, 255),
            255,
        );

        // Each pixel should get ~25% coverage
        let tl = tex.get_pixel(0, 0);
        let tr = tex.get_pixel(1, 0);
        let bl = tex.get_pixel(0, 1);
        let br = tex.get_pixel(1, 1);
        // All should be non-zero (got some coverage)
        assert!(tl.r > 0, "top-left should have coverage");
        assert!(tr.r > 0, "top-right should have coverage");
        assert!(bl.r > 0, "bottom-left should have coverage");
        assert!(br.r > 0, "bottom-right should have coverage");
        // Sum should be ~255
        // 24.8 fixed-point: 3 multiplications + 1 division per pixel, 4 pixels
        // max accumulated error ≈ 4 * 3/256 * 255 ≈ 12
        let sum = tl.r as u16 + tr.r as u16 + bl.r as u16 + br.r as u16;
        assert!(
            (sum as i32 - 255).abs() <= 12,
            "total coverage sum={sum} should be ~255 (±12 for 24.8 precision)"
        );
    }
}
