use crate::cache::HasSize;
use crate::types::{Color, Fixed};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorFormat {
    RGB565,
    RGB565Swapped,
    RGB888,
    RGBA8888,
    BGRA8888,
}

impl ColorFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::RGB565 | Self::RGB565Swapped => 2,
            Self::RGB888 => 3,
            Self::RGBA8888 | Self::BGRA8888 => 4,
        }
    }

    /// Pack a [`Color`] into the little-endian byte layout of this format.
    /// Returns the packed bytes in a u32 (LSB-first for 2-byte formats).
    pub fn pack(self, color: &Color) -> u32 {
        match self {
            Self::RGBA8888 => {
                (color.r as u32)
                    | ((color.g as u32) << 8)
                    | ((color.b as u32) << 16)
                    | ((color.a as u32) << 24)
            }
            Self::BGRA8888 => {
                (color.b as u32)
                    | ((color.g as u32) << 8)
                    | ((color.r as u32) << 16)
                    | ((color.a as u32) << 24)
            }
            Self::RGB888 => (color.r as u32) | ((color.g as u32) << 8) | ((color.b as u32) << 16),
            Self::RGB565 => {
                let px = ((color.r as u16 >> 3) << 11)
                    | ((color.g as u16 >> 2) << 5)
                    | (color.b as u16 >> 3);
                px as u32
            }
            Self::RGB565Swapped => {
                let px = ((color.r as u16 >> 3) << 11)
                    | ((color.g as u16 >> 2) << 5)
                    | (color.b as u16 >> 3);
                ((px >> 8) as u32) | (((px & 0xFF) as u32) << 8)
            }
        }
    }
}

pub enum TexBuf<'a> {
    Ref(&'a [u8]),
    Mut(&'a mut [u8]),
    Owned(Vec<u8>),
}

impl TexBuf<'_> {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Ref(s) => s,
            Self::Mut(s) => s,
            Self::Owned(v) => v,
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            Self::Ref(data) => {
                *self = Self::Owned(data.to_vec());
                match self {
                    Self::Owned(v) => v,
                    _ => unreachable!(),
                }
            }
            Self::Mut(s) => s,
            Self::Owned(v) => v,
        }
    }
}

/// Destination buffer interpretation for blend writes.
///
/// `Opaque` is the framebuffer path: `dst.a` is written as 255 on
/// every pixel (ignoring whatever alpha the source carried). `Blend`
/// is the alpha-aware path used when the destination buffer's alpha
/// channel matters downstream — `dst.a` accumulates via
/// non-premultiplied source-over so a sampler reading the buffer's
/// alpha sees a correct silhouette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Blend,
}

pub struct Texture<'a> {
    pub buf: TexBuf<'a>,
    pub width: u16,
    pub height: u16,
    pub format: ColorFormat,
    pub stride: usize,
    pub alpha_mode: AlphaMode,
}

impl HasSize for Texture<'_> {
    fn cache_size(&self) -> usize {
        self.buf.as_slice().len()
    }
}

impl<'a> Texture<'a> {
    pub const fn from_static(buf: &'a [u8], width: u16, height: u16, format: ColorFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        Self {
            buf: TexBuf::Ref(buf),
            width,
            height,
            format,
            stride,
            alpha_mode: AlphaMode::Opaque,
        }
    }

    pub fn new(buf: &'a mut [u8], width: u16, height: u16, format: ColorFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        Self {
            buf: TexBuf::Mut(buf),
            width,
            height,
            format,
            stride,
            alpha_mode: AlphaMode::Opaque,
        }
    }

    pub fn from_ref(buf: &'a [u8], width: u16, height: u16, format: ColorFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        Self {
            buf: TexBuf::Ref(buf),
            width,
            height,
            format,
            stride,
            alpha_mode: AlphaMode::Opaque,
        }
    }

    pub fn owned(width: u16, height: u16, format: ColorFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        let buf = alloc::vec![0u8; stride * height as usize];
        Self {
            buf: TexBuf::Owned(buf),
            width,
            height,
            format,
            stride,
            alpha_mode: AlphaMode::Opaque,
        }
    }

    #[inline(always)]
    fn offset(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(y as usize * self.stride + x as usize * self.format.bytes_per_pixel())
    }

    #[inline(always)]
    pub fn get_pixel(&self, x: i32, y: i32) -> Color {
        let Some(i) = self.offset(x, y) else {
            return Color::rgb(0, 0, 0);
        };
        let buf = self.buf.as_slice();
        match self.format {
            ColorFormat::RGBA8888 => Color::rgba(buf[i], buf[i + 1], buf[i + 2], buf[i + 3]),
            ColorFormat::BGRA8888 => Color::rgba(buf[i + 2], buf[i + 1], buf[i], buf[i + 3]),
            ColorFormat::RGB888 => Color::rgb(buf[i], buf[i + 1], buf[i + 2]),
            ColorFormat::RGB565 => {
                let lo = buf[i] as u16;
                let hi = buf[i + 1] as u16;
                let px = lo | (hi << 8);
                Color::rgb(
                    ((px >> 11) as u8) << 3,
                    (((px >> 5) & 0x3F) as u8) << 2,
                    ((px & 0x1F) as u8) << 3,
                )
            }
            ColorFormat::RGB565Swapped => {
                let hi = buf[i] as u16;
                let lo = buf[i + 1] as u16;
                let px = lo | (hi << 8);
                Color::rgb(
                    ((px >> 11) as u8) << 3,
                    (((px >> 5) & 0x3F) as u8) << 2,
                    ((px & 0x1F) as u8) << 3,
                )
            }
        }
    }

    #[inline(always)]
    pub fn set_pixel(&mut self, x: i32, y: i32, color: &Color) {
        let Some(i) = self.offset(x, y) else { return };
        let buf = self.buf.as_mut_slice();
        match self.format {
            ColorFormat::RGBA8888 => {
                buf[i] = color.r;
                buf[i + 1] = color.g;
                buf[i + 2] = color.b;
                buf[i + 3] = color.a;
            }
            ColorFormat::BGRA8888 => {
                buf[i] = color.b;
                buf[i + 1] = color.g;
                buf[i + 2] = color.r;
                buf[i + 3] = color.a;
            }
            ColorFormat::RGB888 => {
                buf[i] = color.r;
                buf[i + 1] = color.g;
                buf[i + 2] = color.b;
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
                buf[i] = b0;
                buf[i + 1] = b1;
            }
        }
    }

    #[inline(always)]
    pub fn blend_pixel(&mut self, x: Fixed, y: Fixed, color: &Color, opa: u8) {
        if opa == 0 {
            return;
        }

        if x.is_integer() && y.is_integer() {
            let a = ((color.a as u16) * (opa as u16) / 255) as u8;
            self.blend_pixel_int(x.to_int(), y.to_int(), color, a);
            return;
        }

        self.blend_pixel_subpixel(x, y, color, opa);
    }

    #[cold]
    #[inline(never)]
    fn blend_pixel_subpixel(&mut self, x: Fixed, y: Fixed, color: &Color, opa: u8) {
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

    #[inline(always)]
    pub fn blend_pixel_int(&mut self, x: i32, y: i32, color: &Color, a: u8) {
        if a == 0 {
            return;
        }
        if a == 255 {
            // Fully opaque source: covers dst regardless of mode. The
            // source-over identity (1·src + 0·dst) gives both `out.rgb
            // = src.rgb` and `out.a = src.a` — `set_pixel` does both.
            self.set_pixel(x, y, color);
            return;
        }
        // Alpha blend in plain u8 space: out = (src·a + dst·(255−a) + 127)/255.
        // Avoids the NormColor round-trip (8 divisions per call) that the
        // old implementation did; exact within ±1 over the full range.
        let dst = self.get_pixel(x, y);
        let ia = 255 - a as u32;
        let aa = a as u32;
        let blend = |src: u8, dst: u8| -> u8 {
            let sum = src as u32 * aa + dst as u32 * ia + 127;
            ((sum + (sum >> 8)) >> 8) as u8
        };
        // Blend mode accumulates dst.a via non-premultiplied source-over:
        //   out.a = src.a + dst.a × (255 − src.a) / 255
        // so a downstream sampler reading the buffer's alpha sees a
        // correct silhouette. Opaque mode writes 255 — matches the
        // pre-AlphaMode behaviour for the framebuffer path.
        let out_a = match self.alpha_mode {
            AlphaMode::Opaque => 255,
            AlphaMode::Blend => {
                let src_a = a as u32;
                let dst_a = dst.a as u32;
                let inv = 255 - src_a;
                let sum = src_a * 255 + dst_a * inv + 127;
                ((sum + (sum >> 8)) >> 8) as u8
            }
        };
        let out = Color {
            r: blend(color.r, dst.r),
            g: blend(color.g, dst.g),
            b: blend(color.b, dst.b),
            a: out_a,
        };
        self.set_pixel(x, y, &out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb8888_roundtrip() {
        let mut buf = [0u8; 4];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888);
        let c = Color::rgba(100, 200, 50, 255);
        tex.set_pixel(0, 0, &c);
        assert_eq!(tex.get_pixel(0, 0), c);
    }

    #[test]
    fn bgra8888_roundtrip() {
        let mut buf = [0u8; 4];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::BGRA8888);
        let c = Color::rgba(100, 200, 50, 255);
        tex.set_pixel(0, 0, &c);
        assert_eq!(tex.get_pixel(0, 0), c);
        assert_eq!(buf, [c.b, c.g, c.r, c.a]);
    }

    #[test]
    fn bgra8888_pack_byte_order() {
        let c = Color::rgba(0xAA, 0xBB, 0xCC, 0xDD);
        let bgra = ColorFormat::BGRA8888.pack(&c);
        // LE u32: byte 0=B, 1=G, 2=R, 3=A.
        assert_eq!(bgra & 0xFF, c.b as u32);
        assert_eq!((bgra >> 8) & 0xFF, c.g as u32);
        assert_eq!((bgra >> 16) & 0xFF, c.r as u32);
        assert_eq!((bgra >> 24) & 0xFF, c.a as u32);
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
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888);
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
        let mut buf = [0u8; 4 * 4]; // 2x2 RGBA8888
        let mut tex = Texture::new(&mut buf, 2, 2, ColorFormat::RGBA8888);
        tex.set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.set_pixel(1, 0, &Color::rgb(0, 0, 0));
        tex.set_pixel(0, 1, &Color::rgb(0, 0, 0));
        tex.set_pixel(1, 1, &Color::rgb(0, 0, 0));

        tex.blend_pixel(Fixed::HALF, Fixed::HALF, &Color::rgb(255, 255, 255), 255);

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
