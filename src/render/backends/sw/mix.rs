//! Linear blend of two textures, channel-by-channel:
//! `out = (1 - mix) * a + mix * b`. Used by `TemporalMix` to blend
//! current and previous frames.
//!
//! Both textures must match in size and format. `mix` is u8 (0..255)
//! so the math stays in i32 without rounding tricks.

use crate::render::texture::{ColorFormat, Texture};

/// Mix `b` into `a` in place. `mix=0` leaves `a` untouched; `mix=255`
/// makes `a` a copy of `b`.
pub fn mix_inplace(a: &mut Texture, b: &Texture, mix: u8) {
    if a.width != b.width || a.height != b.height || a.format != b.format {
        return;
    }
    if mix == 0 {
        return;
    }
    let w = a.width as usize;
    let h = a.height as usize;
    match a.format {
        // Per-channel lerp is byte-order-agnostic.
        ColorFormat::RGBA8888 | ColorFormat::BGRA8888 => mix_bytewise(
            a.buf.as_mut_slice(),
            a.stride,
            b.buf.as_slice(),
            b.stride,
            w,
            h,
            4,
            mix,
        ),
        ColorFormat::RGB888 => mix_bytewise(
            a.buf.as_mut_slice(),
            a.stride,
            b.buf.as_slice(),
            b.stride,
            w,
            h,
            3,
            mix,
        ),
        ColorFormat::RGB565 | ColorFormat::RGB565Swapped => mix_565(
            a.buf.as_mut_slice(),
            a.stride,
            b.buf.as_slice(),
            b.stride,
            w,
            h,
            mix,
            a.format == ColorFormat::RGB565Swapped,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn mix_bytewise(
    a: &mut [u8],
    a_stride: usize,
    b: &[u8],
    b_stride: usize,
    w: usize,
    h: usize,
    bpp: usize,
    mix: u8,
) {
    let m = mix as i32;
    let inv = 255 - m;
    for y in 0..h {
        let ao = y * a_stride;
        let bo = y * b_stride;
        for x in 0..w {
            let ai = ao + x * bpp;
            let bi = bo + x * bpp;
            for c in 0..bpp {
                let av = a[ai + c] as i32;
                let bv = b[bi + c] as i32;
                a[ai + c] = ((inv * av + m * bv) / 255) as u8;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn mix_565(
    a: &mut [u8],
    a_stride: usize,
    b: &[u8],
    b_stride: usize,
    w: usize,
    h: usize,
    mix: u8,
    swapped: bool,
) {
    let m = mix as i32;
    let inv = 255 - m;
    for y in 0..h {
        let ao = y * a_stride;
        let bo = y * b_stride;
        for x in 0..w {
            let ai = ao + x * 2;
            let bi = bo + x * 2;
            let (a_lo, a_hi, b_lo, b_hi) = if swapped {
                (a[ai + 1], a[ai], b[bi + 1], b[bi])
            } else {
                (a[ai], a[ai + 1], b[bi], b[bi + 1])
            };
            let pa = ((a_hi as u16) << 8) | (a_lo as u16);
            let pb = ((b_hi as u16) << 8) | (b_lo as u16);
            let ar = (((pa >> 11) & 0x1F) << 3) as i32;
            let ag = (((pa >> 5) & 0x3F) << 2) as i32;
            let ab = ((pa & 0x1F) << 3) as i32;
            let br = (((pb >> 11) & 0x1F) << 3) as i32;
            let bg = (((pb >> 5) & 0x3F) << 2) as i32;
            let bb = ((pb & 0x1F) << 3) as i32;
            let r = (((inv * ar + m * br) / 255) as u16) >> 3;
            let g = (((inv * ag + m * bg) / 255) as u16) >> 2;
            let b_ = (((inv * ab + m * bb) / 255) as u16) >> 3;
            let out = (r << 11) | (g << 5) | b_;
            if swapped {
                a[ai] = (out >> 8) as u8;
                a[ai + 1] = out as u8;
            } else {
                a[ai] = out as u8;
                a[ai + 1] = (out >> 8) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rgba(fill: [u8; 4]) -> alloc::vec::Vec<u8> {
        let mut v = alloc::vec![0u8; 16 * 4];
        for px in v.chunks_exact_mut(4) {
            px.copy_from_slice(&fill);
        }
        v
    }

    #[test]
    fn mix_zero_is_noop() {
        let mut a_buf = make_rgba([100, 100, 100, 255]);
        let original = a_buf.clone();
        let mut b_buf = make_rgba([200, 0, 0, 255]);
        let b = Texture::new(&mut b_buf, 4, 4, ColorFormat::RGBA8888);
        let mut a = Texture::new(&mut a_buf, 4, 4, ColorFormat::RGBA8888);
        mix_inplace(&mut a, &b, 0);
        assert_eq!(a.buf.as_slice(), original.as_slice());
    }

    #[test]
    fn mix_full_replaces_a_with_b() {
        let mut a_buf = make_rgba([100, 100, 100, 255]);
        let mut b_buf = make_rgba([200, 50, 75, 255]);
        let b = Texture::new(&mut b_buf, 4, 4, ColorFormat::RGBA8888);
        let mut a = Texture::new(&mut a_buf, 4, 4, ColorFormat::RGBA8888);
        mix_inplace(&mut a, &b, 255);
        for px in a.buf.as_slice().chunks_exact(4) {
            assert_eq!(px, &[200, 50, 75, 255]);
        }
    }

    #[test]
    fn mix_half_lands_between() {
        let mut a_buf = make_rgba([100, 100, 100, 255]);
        let mut b_buf = make_rgba([200, 0, 0, 255]);
        let b = Texture::new(&mut b_buf, 4, 4, ColorFormat::RGBA8888);
        let mut a = Texture::new(&mut a_buf, 4, 4, ColorFormat::RGBA8888);
        mix_inplace(&mut a, &b, 128);
        for px in a.buf.as_slice().chunks_exact(4) {
            // Integer mix: (127 * a + 128 * b) / 255. Tolerance ±1 for
            // truncation; b is identical-rgb so all three channels
            // land at the same expected value modulo a/b values.
            assert!((px[0] as i32 - 150).abs() <= 1, "r={}", px[0]);
            assert!((px[1] as i32 - 50).abs() <= 1, "g={}", px[1]);
            assert!((px[2] as i32 - 50).abs() <= 1, "b={}", px[2]);
        }
    }

    #[test]
    fn mix_size_mismatch_is_noop() {
        let mut a_buf = make_rgba([10, 10, 10, 255]);
        let original = a_buf.clone();
        let mut b_buf = alloc::vec![99u8; 8 * 4];
        let b = Texture::new(&mut b_buf, 2, 4, ColorFormat::RGBA8888);
        let mut a = Texture::new(&mut a_buf, 4, 4, ColorFormat::RGBA8888);
        mix_inplace(&mut a, &b, 128);
        assert_eq!(a.buf.as_slice(), original.as_slice());
    }
}
