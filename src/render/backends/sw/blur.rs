//! IIR exponential blur — O(1) per pixel regardless of radius. Each
//! channel is convolved by a forward + backward 1D pass per axis,
//! four passes total (row-forward, row-backward, col-forward,
//! col-backward). The kernel is `(1 - α) · α^|n|` where α is the
//! decay factor; visually close to Gaussian for the same effective
//! radius, at a fraction of the cost.

#[cfg(test)]
use crate::render::texture::Texture;
use crate::render::texture::{ColorFormat, TexturePixelsMut};
use crate::types::{Fixed, Rect};

/// Convert a Gaussian-equivalent radius (in pixels) to the IIR decay
/// factor α used by [`iir_blur_inplace`]. The mapping `α ≈ exp(-1/r)`
/// makes the IIR's effective half-width track the requested radius.
/// `r ≤ 0` returns α = 0 (no blur). `r` is clamped at 64 because
/// larger values converge so close to 1.0 the Q24.8 fixed-point
/// representation runs out of headroom.
///
/// Fractional `r` is interpolated linearly between adjacent table
/// entries, so animating `radius` produces a smooth blur ramp instead
/// of stepping each integer pixel.
pub fn alpha_for_radius(radius: Fixed) -> Fixed {
    // Lookup table for exp(-1/r) in Q24.8 (×256). Generated offline;
    // reproducible via `(exp(-1.0 / r) * 256.0).round() as i32`,
    // bumped where needed to stay non-decreasing in the rounding tail.
    // Index 0 unused (radius ≤ 0 short-circuited below).
    const TABLE: [i32; 65] = [
        0, 94, 155, 183, 199, 210, 217, 222, 226, 229, 232, 234, 236, 237, 238, 239, 240, 241, 242,
        243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255, 255, 255, 255, 255, 255,
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ];
    if radius <= Fixed::ZERO {
        return Fixed::ZERO;
    }
    if radius >= Fixed::from_int(64) {
        return Fixed::from_ratio(TABLE[64], 256);
    }
    // Linear blend between adjacent Q24.8 table entries.
    let lo = radius.floor().to_int();
    let hi = (lo + 1).min(64);
    let fraction = radius - Fixed::from_int(lo);
    let a_lo = TABLE[lo as usize];
    let a_hi = TABLE[hi as usize];
    Fixed::from_ratio(a_lo, 256) + Fixed::from_ratio(a_hi - a_lo, 256) * fraction
}

/// Blur `tex` in place using the IIR exponential filter with decay
/// factor `alpha` (typically from [`alpha_for_radius`]). Alpha values
/// outside `(0, 1)` short-circuit: `<= 0` is a no-op, `>= 1` would
/// be a degenerate constant — also no-op.
pub fn iir_blur_inplace(tex: &mut TexturePixelsMut<'_>, alpha: Fixed, region: Rect) {
    if alpha <= Fixed::ZERO || alpha >= Fixed::ONE {
        return;
    }
    let w = tex.width();
    let h = tex.height();
    if w == 0 || h == 0 {
        return;
    }
    let screen = Rect::new(0, 0, w as i32, h as i32);
    let Some(r) = region.intersect(&screen) else {
        return;
    };
    let (x0, y0, x1, y1) = r.pixel_bounds();
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let alpha_q = (alpha * Fixed::from_int(256)).to_int().clamp(0, 256);
    let one_minus_q = 256 - alpha_q;
    let rw = (x1 - x0) as usize;
    let rh = (y1 - y0) as usize;

    let stride = tex.stride();
    match tex.format() {
        ColorFormat::RGBA8888 => iir_blur_rgba8888(
            tex.bytes_mut(),
            w as usize,
            h as usize,
            stride,
            alpha_q,
            one_minus_q,
            x0 as usize,
            y0 as usize,
            rw,
            rh,
        ),
        ColorFormat::RGB565 => iir_blur_rgb565(
            tex.bytes_mut(),
            w as usize,
            h as usize,
            stride,
            alpha_q,
            one_minus_q,
            false,
            x0 as usize,
            y0 as usize,
            rw,
            rh,
        ),
        ColorFormat::RGB565Swapped => iir_blur_rgb565(
            tex.bytes_mut(),
            w as usize,
            h as usize,
            stride,
            alpha_q,
            one_minus_q,
            true,
            x0 as usize,
            y0 as usize,
            rw,
            rh,
        ),
        ColorFormat::RGB888 | ColorFormat::BGRA8888 => {}
    }
}

/// IIR forward + backward over a 1D series of `i32` samples (one
/// channel). `step` is the byte stride between consecutive samples
/// (so the same routine handles both row-major and column-major
/// traversal). Result is written back in place.
#[inline(always)]
fn iir_pass(
    buf: &mut [u8],
    offset: usize,
    count: usize,
    step: usize,
    alpha_q: i32,
    one_minus_q: i32,
) {
    if count < 2 {
        return;
    }
    let mut acc = buf[offset] as i32;
    let mut idx = offset + step;
    for _ in 1..count {
        let v = buf[idx] as i32;
        acc = (alpha_q * acc + one_minus_q * v) >> 8;
        buf[idx] = acc as u8;
        idx += step;
    }
    let last = offset + (count - 1) * step;
    acc = buf[last] as i32;
    let mut idx = last;
    for _ in 1..count {
        idx -= step;
        let v = buf[idx] as i32;
        acc = (alpha_q * acc + one_minus_q * v) >> 8;
        buf[idx] = acc as u8;
    }
}

#[allow(clippy::too_many_arguments)]
fn iir_blur_rgba8888(
    buf: &mut [u8],
    w: usize,
    h: usize,
    stride: usize,
    alpha_q: i32,
    one_minus_q: i32,
    x0: usize,
    y0: usize,
    rw: usize,
    rh: usize,
) {
    let _ = h;
    for y in 0..rh {
        let row_off = (y0 + y) * stride + x0 * 4;
        for ch in 0..4 {
            iir_pass(buf, row_off + ch, rw, 4, alpha_q, one_minus_q);
        }
    }
    for x in 0..rw {
        let col_off = (x0 + x) * 4 + y0 * stride;
        for ch in 0..4 {
            iir_pass(buf, col_off + ch, rh, stride, alpha_q, one_minus_q);
        }
    }
    let _ = w;
}

/// Pack/unpack an RGB565 word `[r5, g6, b5]` into u8 channels expanded
/// to the full 0..255 range, blur, then pack back. Doing the IIR on
/// 5-/6-bit channels directly produces bad rounding artifacts; expand
/// to 8-bit first.
#[allow(clippy::too_many_arguments)]
fn iir_blur_rgb565(
    buf: &mut [u8],
    w: usize,
    h: usize,
    stride: usize,
    alpha_q: i32,
    one_minus_q: i32,
    swapped: bool,
    x0: usize,
    y0: usize,
    rw: usize,
    rh: usize,
) {
    let _ = (w, h);
    let mut tmp: alloc::vec::Vec<u8> = alloc::vec::Vec::with_capacity(rw * rh * 3);
    for y in 0..rh {
        for x in 0..rw {
            let i = (y0 + y) * stride + (x0 + x) * 2;
            let (lo, hi) = if swapped {
                (buf[i + 1], buf[i])
            } else {
                (buf[i], buf[i + 1])
            };
            let pixel = ((hi as u16) << 8) | (lo as u16);
            let r5 = ((pixel >> 11) & 0x1F) as u8;
            let g6 = ((pixel >> 5) & 0x3F) as u8;
            let b5 = (pixel & 0x1F) as u8;
            tmp.push((r5 << 3) | (r5 >> 2));
            tmp.push((g6 << 2) | (g6 >> 4));
            tmp.push((b5 << 3) | (b5 >> 2));
        }
    }
    iir_blur_packed_rgb(&mut tmp, rw, rh, rw * 3, 3, alpha_q, one_minus_q);
    for y in 0..rh {
        for x in 0..rw {
            let ti = (y * rw + x) * 3;
            let r = tmp[ti] >> 3;
            let g = tmp[ti + 1] >> 2;
            let b = tmp[ti + 2] >> 3;
            let pixel = ((r as u16) << 11) | ((g as u16) << 5) | (b as u16);
            let i = (y0 + y) * stride + (x0 + x) * 2;
            if swapped {
                buf[i] = (pixel >> 8) as u8;
                buf[i + 1] = pixel as u8;
            } else {
                buf[i] = pixel as u8;
                buf[i + 1] = (pixel >> 8) as u8;
            }
        }
    }
}

/// IIR blur over a tightly packed N-channel buffer. RGBA8888 keeps a
/// `bpp = 4` specialised version on the framebuffer hot path.
fn iir_blur_packed_rgb(
    buf: &mut [u8],
    w: usize,
    h: usize,
    stride: usize,
    bpp: usize,
    alpha_q: i32,
    one_minus_q: i32,
) {
    for y in 0..h {
        let row_off = y * stride;
        for ch in 0..bpp {
            iir_pass(buf, row_off + ch, w, bpp, alpha_q, one_minus_q);
        }
    }
    for x in 0..w {
        let col_off = x * bpp;
        for ch in 0..bpp {
            iir_pass(buf, col_off + ch, h, stride, alpha_q, one_minus_q);
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sw/blur.rs"]
mod tests;
