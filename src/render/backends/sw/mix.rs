//! Linear blend of two pixel borrows, channel-by-channel: `out = (1 - mix) * a + mix * b`.
//!
//! Both planes must match in size and format. `mix` is u8 (0..255); interpolation uses integer arithmetic.

use crate::render::texture::{ColorFormat, TexturePixels, TexturePixelsMut};

/// Mix `b` into `a` in place. `mix=0` leaves `a` untouched; `mix=255` copies `b`. Mismatched dimensions or formats leave `a` unchanged.
pub fn mix_inplace(a: &mut TexturePixelsMut<'_>, b: &TexturePixels<'_>, mix: u8) {
    if a.width() != b.width() || a.height() != b.height() || Some(a.format()) != b.direct_format() {
        return;
    }
    if mix == 0 {
        return;
    }
    let w = a.width() as usize;
    let h = a.height() as usize;
    let stride = a.stride();
    let format = a.format();
    match format {
        // Per-channel lerp is byte-order-agnostic.
        ColorFormat::RGBA8888 | ColorFormat::BGRA8888 => {
            mix_bytewise(a.bytes_mut(), stride, b.bytes(), b.stride(), w, h, 4, mix)
        }
        ColorFormat::RGB888 => {
            mix_bytewise(a.bytes_mut(), stride, b.bytes(), b.stride(), w, h, 3, mix)
        }
        ColorFormat::RGB565 | ColorFormat::RGB565Swapped => mix_565(
            a.bytes_mut(),
            stride,
            b.bytes(),
            b.stride(),
            w,
            h,
            mix,
            format == ColorFormat::RGB565Swapped,
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
#[path = "../../../../tests/unit/render/backends/sw/mix.rs"]
mod tests;
