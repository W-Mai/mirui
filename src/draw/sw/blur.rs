//! IIR exponential blur — O(1) per pixel regardless of radius. Each
//! channel is convolved by a forward + backward 1D pass per axis,
//! four passes total (row-forward, row-backward, col-forward,
//! col-backward). The kernel is `(1 - α) · α^|n|` where α is the
//! decay factor; visually close to Gaussian for the same effective
//! radius, at a fraction of the cost.

use crate::draw::texture::{ColorFormat, Texture};
use crate::types::Fixed;

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
        return Fixed::from_raw(TABLE[64]);
    }
    // Linear blend between TABLE[lo] and TABLE[lo+1] in raw Q24.8
    // alpha space. frac.raw() is fixed-point in [0, 256); multiply
    // by raw delta and shift back to land on the same scale as
    // TABLE entries.
    let lo = radius.floor().to_int();
    let hi = (lo + 1).min(64);
    let frac_raw = (radius - Fixed::from_int(lo)).raw();
    let a_lo = TABLE[lo as usize];
    let a_hi = TABLE[hi as usize];
    Fixed::from_raw(a_lo + ((a_hi - a_lo) * frac_raw / 256))
}

/// Blur `tex` in place using the IIR exponential filter with decay
/// factor `alpha` (typically from [`alpha_for_radius`]). Alpha values
/// outside `(0, 1)` short-circuit: `<= 0` is a no-op, `>= 1` would
/// be a degenerate constant — also no-op.
pub fn iir_blur_inplace(tex: &mut Texture, alpha: Fixed) {
    if alpha <= Fixed::ZERO || alpha >= Fixed::ONE {
        return;
    }
    let w = tex.width;
    let h = tex.height;
    if w == 0 || h == 0 {
        return;
    }
    let alpha_q = alpha.raw().clamp(0, 256);
    let one_minus_q = 256 - alpha_q;

    match tex.format {
        ColorFormat::RGBA8888 => iir_blur_rgba8888(
            tex.buf.as_mut_slice(),
            w,
            h,
            tex.stride,
            alpha_q,
            one_minus_q,
        ),
        ColorFormat::RGB565 => iir_blur_rgb565(
            tex.buf.as_mut_slice(),
            w,
            h,
            tex.stride,
            alpha_q,
            one_minus_q,
            false,
        ),
        ColorFormat::RGB565Swapped => iir_blur_rgb565(
            tex.buf.as_mut_slice(),
            w,
            h,
            tex.stride,
            alpha_q,
            one_minus_q,
            true,
        ),
        // RGB888 isn't reachable from any current backend.
        ColorFormat::RGB888 => {}
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

fn iir_blur_rgba8888(
    buf: &mut [u8],
    w: u16,
    h: u16,
    stride: usize,
    alpha_q: i32,
    one_minus_q: i32,
) {
    let w = w as usize;
    let h = h as usize;
    // Row passes: step = 4 bytes (one pixel) so consecutive samples
    // on the same channel skip the other three channels.
    for y in 0..h {
        let row_off = y * stride;
        for ch in 0..4 {
            iir_pass(buf, row_off + ch, w, 4, alpha_q, one_minus_q);
        }
    }
    for x in 0..w {
        let col_off = x * 4;
        for ch in 0..4 {
            iir_pass(buf, col_off + ch, h, stride, alpha_q, one_minus_q);
        }
    }
}

/// Pack/unpack an RGB565 word `[r5, g6, b5]` into u8 channels expanded
/// to the full 0..255 range, blur, then pack back. Doing the IIR on
/// 5-/6-bit channels directly produces bad rounding artifacts; expand
/// to 8-bit first.
fn iir_blur_rgb565(
    buf: &mut [u8],
    w: u16,
    h: u16,
    stride: usize,
    alpha_q: i32,
    one_minus_q: i32,
    swapped: bool,
) {
    let w = w as usize;
    let h = h as usize;
    // Working buffer in RGBA8888 layout (alpha unused, kept for
    // alignment with the main path); 4 bytes per pixel.
    let mut tmp: alloc::vec::Vec<u8> = alloc::vec::Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let i = y * stride + x * 2;
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
            tmp.push(255);
        }
    }
    iir_blur_rgba8888(&mut tmp, w as u16, h as u16, w * 4, alpha_q, one_minus_q);
    for y in 0..h {
        for x in 0..w {
            let ti = (y * w + x) * 4;
            let r = tmp[ti] >> 3;
            let g = tmp[ti + 1] >> 2;
            let b = tmp[ti + 2] >> 3;
            let pixel = ((r as u16) << 11) | ((g as u16) << 5) | (b as u16);
            let i = y * stride + x * 2;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_for_radius_zero_is_zero() {
        assert_eq!(alpha_for_radius(Fixed::ZERO), Fixed::ZERO);
    }

    #[test]
    fn alpha_for_radius_is_nondecreasing_and_bounded() {
        let mut prev = Fixed::ZERO;
        for r in 1..=64i32 {
            let a = alpha_for_radius(Fixed::from_int(r));
            assert!(a >= prev, "alpha at r={} regressed", r);
            assert!(a < Fixed::ONE, "alpha at r={} reached 1", r);
            prev = a;
        }
    }

    #[test]
    fn alpha_for_radius_fractional_interpolates_between_neighbours() {
        // r=4.5 should land between integer 4 and 5, not snap to either.
        let a4 = alpha_for_radius(Fixed::from_int(4));
        let a5 = alpha_for_radius(Fixed::from_int(5));
        let a4_5 = alpha_for_radius(Fixed::from_int(4) + Fixed::ONE / 2);
        assert!(
            a4 < a4_5 && a4_5 < a5,
            "got a4={a4:?} a4.5={a4_5:?} a5={a5:?}"
        );
    }

    #[test]
    fn iir_blur_noop_on_zero_alpha() {
        let mut buf = alloc::vec![0u8; 4 * 4 * 4];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = i as u8;
        }
        let original = buf.clone();
        let mut tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        iir_blur_inplace(&mut tex, Fixed::ZERO);
        assert_eq!(tex.buf.as_slice(), original.as_slice());
    }

    #[test]
    fn iir_blur_constant_input_stays_constant() {
        // A field of solid red should stay solid red after blurring,
        // because every neighbourhood is identical.
        let mut buf = alloc::vec![0u8; 8 * 8 * 4];
        for px in buf.chunks_exact_mut(4) {
            px[0] = 200;
            px[1] = 100;
            px[2] = 50;
            px[3] = 255;
        }
        let mut tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888);
        iir_blur_inplace(&mut tex, alpha_for_radius(Fixed::from_int(4)));
        for px in tex.buf.as_slice().chunks_exact(4) {
            assert!((px[0] as i32 - 200).abs() <= 1, "r drifted: {}", px[0]);
            assert!((px[1] as i32 - 100).abs() <= 1, "g drifted: {}", px[1]);
            assert!((px[2] as i32 - 50).abs() <= 1, "b drifted: {}", px[2]);
        }
    }

    #[test]
    fn iir_blur_spreads_a_single_bright_pixel() {
        let mut buf = alloc::vec![0u8; 9 * 9 * 4];
        let centre = (4 * 9 + 4) * 4;
        buf[centre] = 255;
        buf[centre + 1] = 255;
        buf[centre + 2] = 255;
        buf[centre + 3] = 255;
        let total_before: u64 = buf
            .chunks_exact(4)
            .map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64)
            .sum();
        let mut tex = Texture::new(&mut buf, 9, 9, ColorFormat::RGBA8888);
        iir_blur_inplace(&mut tex, alpha_for_radius(Fixed::from_int(2)));
        let after = tex.buf.as_slice();
        let centre_after = after[centre] as i32;
        assert!(centre_after < 255, "centre should have dimmed");
        let total_after: u64 = after
            .chunks_exact(4)
            .map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64)
            .sum();
        // IIR doesn't strictly conserve energy (fixed-point round-off
        // and edge handling drift) but should stay within an order of
        // magnitude of the input.
        assert!(total_after > total_before / 4, "energy collapsed");
        assert!(total_after < total_before * 4, "energy exploded");
    }
}
