use crate::render::texture::{ColorFormat, Texture};

#[allow(clippy::too_many_arguments)]
pub fn blit_generic_slow(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    sw: u16,
    sh: u16,
    dx0: i32,
    dy0: i32,
    dw: i32,
    dh: i32,
    clip_x0: i32,
    clip_y0: i32,
    clip_x1: i32,
    clip_y1: i32,
) {
    for drow in 0..dh {
        let iy = dy0 + drow;
        if iy < clip_y0 || iy >= clip_y1 {
            continue;
        }
        let sy = sy0 + (drow * sh as i32) / dh;
        for dcol in 0..dw {
            let ix = dx0 + dcol;
            if ix < clip_x0 || ix >= clip_x1 {
                continue;
            }
            let sx = sx0 + (dcol * sw as i32) / dw;
            let src_color = src.get_pixel(sx, sy);
            if src_color.a == 0 {
                continue;
            }
            if src_color.a == 255 {
                dst.set_pixel(ix, iy, &src_color);
            } else {
                dst.blend_pixel_int(ix, iy, &src_color, src_color.a);
            }
        }
    }
}

/// 1× integer-scale blit: dst rect exactly matches src rect size.
/// Specialized per (src_format, dst_format) so the inner loop is a
/// tight byte copy / format convert with no `get_pixel` / `set_pixel`
/// bookkeeping. Caller clips to `dst ∩ clip ∩ screen` before dispatch;
/// anything that doesn't hit an integer-scale case falls back to
/// `blit_dda`.
#[allow(clippy::too_many_arguments)]
pub fn blit_1to1_fast(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    sw: u16,
    sh: u16,
    dx0: i32,
    dy0: i32,
    clip_x0: i32,
    clip_y0: i32,
    clip_x1: i32,
    clip_y1: i32,
) {
    // Restrict to the visible dst subrect (dst ∩ clip). Everything
    // inside is guaranteed to land within the dst texture since the
    // entry in blit() already intersected with screen + dst bounds.
    let vx0 = dx0.max(clip_x0);
    let vy0 = dy0.max(clip_y0);
    let vx1 = (dx0 + sw as i32).min(clip_x1);
    let vy1 = (dy0 + sh as i32).min(clip_y1);
    if vx1 <= vx0 || vy1 <= vy0 {
        return;
    }
    let src_x0 = sx0 + (vx0 - dx0);
    let src_y0 = sy0 + (vy0 - dy0);
    let run_w = (vx1 - vx0) as usize;
    let run_h = (vy1 - vy0) as usize;

    match (src.format, dst.format) {
        (ColorFormat::RGBA8888, ColorFormat::RGBA8888) => {
            blit_1to1_argb_to_argb(dst, src, src_x0, src_y0, vx0, vy0, run_w, run_h)
        }
        (ColorFormat::RGBA8888, ColorFormat::RGB565Swapped) => {
            blit_1to1_argb_to_565sw(dst, src, src_x0, src_y0, vx0, vy0, run_w, run_h)
        }
        (ColorFormat::RGB565Swapped, ColorFormat::RGB565Swapped) => {
            blit_1to1_565sw_to_565sw(dst, src, src_x0, src_y0, vx0, vy0, run_w, run_h)
        }
        (ColorFormat::RGBA8888, ColorFormat::RGB565) => {
            blit_1to1_argb_to_565(dst, src, src_x0, src_y0, vx0, vy0, run_w, run_h)
        }
        _ => blit_generic_slow(
            dst,
            src,
            src_x0,
            src_y0,
            run_w as u16,
            run_h as u16,
            vx0,
            vy0,
            run_w as i32,
            run_h as i32,
            clip_x0,
            clip_y0,
            clip_x1,
            clip_y1,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_1to1_argb_to_argb(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    run_w: usize,
    run_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let blend_aware = dst.alpha_mode == crate::render::texture::AlphaMode::Blend;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for row in 0..run_h {
        let src_row_off = (sy0 as usize + row) * src_stride + sx0 as usize * 4;
        let dst_row_off = (dy0 as usize + row) * dst_stride + dx0 as usize * 4;
        for col in 0..run_w {
            let si = src_row_off + col * 4;
            let di = dst_row_off + col * 4;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            if a == 255 {
                dst_buf[di] = src_buf[si];
                dst_buf[di + 1] = src_buf[si + 1];
                dst_buf[di + 2] = src_buf[si + 2];
                // a == 255: source covers dst regardless of mode; both
                // write 255 (Blend's source-over identity gives 255).
                dst_buf[di + 3] = 255;
            } else {
                // Blend: out = src * a + dst * (255 - a), via integer.
                let inv = 255 - a as u16;
                let sa = a as u16;
                dst_buf[di] = ((src_buf[si] as u16 * sa + dst_buf[di] as u16 * inv) / 255) as u8;
                dst_buf[di + 1] =
                    ((src_buf[si + 1] as u16 * sa + dst_buf[di + 1] as u16 * inv) / 255) as u8;
                dst_buf[di + 2] =
                    ((src_buf[si + 2] as u16 * sa + dst_buf[di + 2] as u16 * inv) / 255) as u8;
                dst_buf[di + 3] = if blend_aware {
                    // Source-over alpha accumulation:
                    //   out.a = src.a + dst.a × (255 − src.a) / 255
                    let dst_a = dst_buf[di + 3] as u16;
                    (sa + (dst_a * inv) / 255) as u8
                } else {
                    255
                };
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_1to1_argb_to_565sw(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    run_w: usize,
    run_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for row in 0..run_h {
        let src_row_off = (sy0 as usize + row) * src_stride + sx0 as usize * 4;
        let dst_row_off = (dy0 as usize + row) * dst_stride + dx0 as usize * 2;
        for col in 0..run_w {
            let si = src_row_off + col * 4;
            let di = dst_row_off + col * 2;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            let (r, g, b) = if a == 255 {
                (src_buf[si], src_buf[si + 1], src_buf[si + 2])
            } else {
                // Decode existing dst 565 → RGB888, blend, re-encode.
                let hi = dst_buf[di] as u16;
                let lo = dst_buf[di + 1] as u16;
                let px = lo | (hi << 8);
                let dr = ((px >> 11) as u8) << 3;
                let dg = (((px >> 5) & 0x3F) as u8) << 2;
                let db = ((px & 0x1F) as u8) << 3;
                let inv = 255 - a as u16;
                let sa = a as u16;
                (
                    ((src_buf[si] as u16 * sa + dr as u16 * inv) / 255) as u8,
                    ((src_buf[si + 1] as u16 * sa + dg as u16 * inv) / 255) as u8,
                    ((src_buf[si + 2] as u16 * sa + db as u16 * inv) / 255) as u8,
                )
            };
            let px = ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3);
            dst_buf[di] = (px >> 8) as u8;
            dst_buf[di + 1] = px as u8;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_1to1_565sw_to_565sw(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    run_w: usize,
    run_h: usize,
) {
    // 565 has no alpha channel — always fully opaque copy. Use
    // copy_from_slice per row for the best chance of memcpy in
    // release builds.
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for row in 0..run_h {
        let src_row_off = (sy0 as usize + row) * src_stride + sx0 as usize * 2;
        let dst_row_off = (dy0 as usize + row) * dst_stride + dx0 as usize * 2;
        dst_buf[dst_row_off..dst_row_off + run_w * 2]
            .copy_from_slice(&src_buf[src_row_off..src_row_off + run_w * 2]);
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_1to1_argb_to_565(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    run_w: usize,
    run_h: usize,
) {
    // Identical logic to 565sw variant except low/high byte order is
    // swapped on encode. Separate functions so the format is a
    // compile-time constant within each.
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for row in 0..run_h {
        let src_row_off = (sy0 as usize + row) * src_stride + sx0 as usize * 4;
        let dst_row_off = (dy0 as usize + row) * dst_stride + dx0 as usize * 2;
        for col in 0..run_w {
            let si = src_row_off + col * 4;
            let di = dst_row_off + col * 2;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            let (r, g, b) = if a == 255 {
                (src_buf[si], src_buf[si + 1], src_buf[si + 2])
            } else {
                let lo = dst_buf[di] as u16;
                let hi = dst_buf[di + 1] as u16;
                let px = lo | (hi << 8);
                let dr = ((px >> 11) as u8) << 3;
                let dg = (((px >> 5) & 0x3F) as u8) << 2;
                let db = ((px & 0x1F) as u8) << 3;
                let inv = 255 - a as u16;
                let sa = a as u16;
                (
                    ((src_buf[si] as u16 * sa + dr as u16 * inv) / 255) as u8,
                    ((src_buf[si + 1] as u16 * sa + dg as u16 * inv) / 255) as u8,
                    ((src_buf[si + 2] as u16 * sa + db as u16 * inv) / 255) as u8,
                )
            };
            let px = ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3);
            dst_buf[di] = px as u8;
            dst_buf[di + 1] = (px >> 8) as u8;
        }
    }
}

/// 2× integer-scale blit: dst is exactly twice src in both axes.
/// Each src pixel writes a 2×2 dst block; src is read once per block
/// instead of 4 times in the generic path. Specialized per
/// (src_format, dst_format) like `blit_1to1_fast`.
#[allow(clippy::too_many_arguments)]
pub fn blit_2to2_fast(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    sw: u16,
    sh: u16,
    dx0: i32,
    dy0: i32,
    clip_x0: i32,
    clip_y0: i32,
    clip_x1: i32,
    clip_y1: i32,
) {
    // Clip to dst rect ∩ clip. 2× constraint requires the visible
    // rect to start at even offsets from (dx0, dy0) for fast path;
    // anything else falls back to DDA which handles it correctly.
    let vx0 = dx0.max(clip_x0);
    let vy0 = dy0.max(clip_y0);
    let vx1 = (dx0 + (sw as i32) * 2).min(clip_x1);
    let vy1 = (dy0 + (sh as i32) * 2).min(clip_y1);
    if vx1 <= vx0 || vy1 <= vy0 {
        return;
    }
    // A dst pixel at (vx0, vy0) corresponds to src pixel
    // (sx0 + (vx0 - dx0) / 2, sy0 + (vy0 - dy0) / 2). If the visible
    // rect doesn't land on even boundaries the block structure
    // breaks; the DDA path is correct so we fall through there.
    let dx_off = vx0 - dx0;
    let dy_off = vy0 - dy0;
    if dx_off & 1 != 0 || dy_off & 1 != 0 || (vx1 - vx0) & 1 != 0 || (vy1 - vy0) & 1 != 0 {
        blit_dda(
            dst,
            src,
            sx0,
            sy0,
            sw,
            sh,
            dx0,
            dy0,
            (sw as i32) * 2,
            (sh as i32) * 2,
            clip_x0,
            clip_y0,
            clip_x1,
            clip_y1,
        );
        return;
    }

    let src_x0 = sx0 + dx_off / 2;
    let src_y0 = sy0 + dy_off / 2;
    let block_w = (vx1 - vx0) as usize / 2;
    let block_h = (vy1 - vy0) as usize / 2;

    match (src.format, dst.format) {
        (ColorFormat::RGBA8888, ColorFormat::RGBA8888) => {
            blit_2to2_argb_to_argb(dst, src, src_x0, src_y0, vx0, vy0, block_w, block_h)
        }
        (ColorFormat::RGBA8888, ColorFormat::RGB565Swapped) => {
            blit_2to2_argb_to_565sw(dst, src, src_x0, src_y0, vx0, vy0, block_w, block_h)
        }
        (ColorFormat::RGB565Swapped, ColorFormat::RGB565Swapped) => {
            blit_2to2_565sw_to_565sw(dst, src, src_x0, src_y0, vx0, vy0, block_w, block_h)
        }
        (ColorFormat::RGBA8888, ColorFormat::RGB565) => {
            blit_2to2_argb_to_565(dst, src, src_x0, src_y0, vx0, vy0, block_w, block_h)
        }
        _ => blit_dda(
            dst,
            src,
            sx0,
            sy0,
            sw,
            sh,
            dx0,
            dy0,
            (sw as i32) * 2,
            (sh as i32) * 2,
            clip_x0,
            clip_y0,
            clip_x1,
            clip_y1,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_2to2_argb_to_argb(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    block_w: usize,
    block_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for block_row in 0..block_h {
        let src_row_off = (sy0 as usize + block_row) * src_stride + sx0 as usize * 4;
        let dst_row0 = (dy0 as usize + block_row * 2) * dst_stride + dx0 as usize * 4;
        let dst_row1 = dst_row0 + dst_stride;
        for col in 0..block_w {
            let si = src_row_off + col * 4;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            let px = [src_buf[si], src_buf[si + 1], src_buf[si + 2], a];
            if a == 255 {
                let d00 = dst_row0 + col * 8;
                let d01 = d00 + 4;
                let d10 = dst_row1 + col * 8;
                let d11 = d10 + 4;
                dst_buf[d00..d00 + 4].copy_from_slice(&px);
                dst_buf[d01..d01 + 4].copy_from_slice(&px);
                dst_buf[d10..d10 + 4].copy_from_slice(&px);
                dst_buf[d11..d11 + 4].copy_from_slice(&px);
            } else {
                // Four blends; hand-roll instead of a helper to keep
                // bounds check elided by the compiler per index.
                let inv = 255 - a as u16;
                let sa = a as u16;
                for &di in &[
                    dst_row0 + col * 8,
                    dst_row0 + col * 8 + 4,
                    dst_row1 + col * 8,
                    dst_row1 + col * 8 + 4,
                ] {
                    dst_buf[di] = ((px[0] as u16 * sa + dst_buf[di] as u16 * inv) / 255) as u8;
                    dst_buf[di + 1] =
                        ((px[1] as u16 * sa + dst_buf[di + 1] as u16 * inv) / 255) as u8;
                    dst_buf[di + 2] =
                        ((px[2] as u16 * sa + dst_buf[di + 2] as u16 * inv) / 255) as u8;
                    dst_buf[di + 3] = 255;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_2to2_argb_to_565sw(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    block_w: usize,
    block_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for block_row in 0..block_h {
        let src_row_off = (sy0 as usize + block_row) * src_stride + sx0 as usize * 4;
        let dst_row0 = (dy0 as usize + block_row * 2) * dst_stride + dx0 as usize * 2;
        let dst_row1 = dst_row0 + dst_stride;
        for col in 0..block_w {
            let si = src_row_off + col * 4;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            // α==255 path: encode once, splat 4 times.
            if a == 255 {
                let r = src_buf[si] as u16;
                let g = src_buf[si + 1] as u16;
                let b = src_buf[si + 2] as u16;
                let px = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
                let hi = (px >> 8) as u8;
                let lo = px as u8;
                let d00 = dst_row0 + col * 4;
                let d10 = dst_row1 + col * 4;
                dst_buf[d00] = hi;
                dst_buf[d00 + 1] = lo;
                dst_buf[d00 + 2] = hi;
                dst_buf[d00 + 3] = lo;
                dst_buf[d10] = hi;
                dst_buf[d10 + 1] = lo;
                dst_buf[d10 + 2] = hi;
                dst_buf[d10 + 3] = lo;
            } else {
                // Partial α on 565 loses precision either way. Use
                // the generic 1-pixel helper 4 times; not hot.
                let sr = src_buf[si];
                let sg = src_buf[si + 1];
                let sb = src_buf[si + 2];
                let inv = 255 - a as u16;
                let sa = a as u16;
                for &di in &[
                    dst_row0 + col * 4,
                    dst_row0 + col * 4 + 2,
                    dst_row1 + col * 4,
                    dst_row1 + col * 4 + 2,
                ] {
                    let hi = dst_buf[di] as u16;
                    let lo = dst_buf[di + 1] as u16;
                    let dpx = lo | (hi << 8);
                    let dr = ((dpx >> 11) as u8) << 3;
                    let dg = (((dpx >> 5) & 0x3F) as u8) << 2;
                    let db = ((dpx & 0x1F) as u8) << 3;
                    let br = ((sr as u16 * sa + dr as u16 * inv) / 255) as u8;
                    let bg = ((sg as u16 * sa + dg as u16 * inv) / 255) as u8;
                    let bb = ((sb as u16 * sa + db as u16 * inv) / 255) as u8;
                    let px = ((br as u16 >> 3) << 11) | ((bg as u16 >> 2) << 5) | (bb as u16 >> 3);
                    dst_buf[di] = (px >> 8) as u8;
                    dst_buf[di + 1] = px as u8;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_2to2_565sw_to_565sw(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    block_w: usize,
    block_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for block_row in 0..block_h {
        let src_row_off = (sy0 as usize + block_row) * src_stride + sx0 as usize * 2;
        let dst_row0 = (dy0 as usize + block_row * 2) * dst_stride + dx0 as usize * 2;
        let dst_row1 = dst_row0 + dst_stride;
        for col in 0..block_w {
            let si = src_row_off + col * 2;
            let hi = src_buf[si];
            let lo = src_buf[si + 1];
            let d00 = dst_row0 + col * 4;
            let d10 = dst_row1 + col * 4;
            dst_buf[d00] = hi;
            dst_buf[d00 + 1] = lo;
            dst_buf[d00 + 2] = hi;
            dst_buf[d00 + 3] = lo;
            dst_buf[d10] = hi;
            dst_buf[d10 + 1] = lo;
            dst_buf[d10 + 2] = hi;
            dst_buf[d10 + 3] = lo;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_2to2_argb_to_565(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    dx0: i32,
    dy0: i32,
    block_w: usize,
    block_h: usize,
) {
    let src_stride = src.stride;
    let dst_stride = dst.stride;
    let src_buf = src.buf.as_slice();
    let dst_buf = dst.buf.as_mut_slice();
    for block_row in 0..block_h {
        let src_row_off = (sy0 as usize + block_row) * src_stride + sx0 as usize * 4;
        let dst_row0 = (dy0 as usize + block_row * 2) * dst_stride + dx0 as usize * 2;
        let dst_row1 = dst_row0 + dst_stride;
        for col in 0..block_w {
            let si = src_row_off + col * 4;
            let a = src_buf[si + 3];
            if a == 0 {
                continue;
            }
            if a == 255 {
                let r = src_buf[si] as u16;
                let g = src_buf[si + 1] as u16;
                let b = src_buf[si + 2] as u16;
                let px = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
                let lo = px as u8;
                let hi = (px >> 8) as u8;
                let d00 = dst_row0 + col * 4;
                let d10 = dst_row1 + col * 4;
                dst_buf[d00] = lo;
                dst_buf[d00 + 1] = hi;
                dst_buf[d00 + 2] = lo;
                dst_buf[d00 + 3] = hi;
                dst_buf[d10] = lo;
                dst_buf[d10 + 1] = hi;
                dst_buf[d10 + 2] = lo;
                dst_buf[d10 + 3] = hi;
            } else {
                let sr = src_buf[si];
                let sg = src_buf[si + 1];
                let sb = src_buf[si + 2];
                let inv = 255 - a as u16;
                let sa = a as u16;
                for &di in &[
                    dst_row0 + col * 4,
                    dst_row0 + col * 4 + 2,
                    dst_row1 + col * 4,
                    dst_row1 + col * 4 + 2,
                ] {
                    let lo = dst_buf[di] as u16;
                    let hi = dst_buf[di + 1] as u16;
                    let dpx = lo | (hi << 8);
                    let dr = ((dpx >> 11) as u8) << 3;
                    let dg = (((dpx >> 5) & 0x3F) as u8) << 2;
                    let db = ((dpx & 0x1F) as u8) << 3;
                    let br = ((sr as u16 * sa + dr as u16 * inv) / 255) as u8;
                    let bg = ((sg as u16 * sa + dg as u16 * inv) / 255) as u8;
                    let bb = ((sb as u16 * sa + db as u16 * inv) / 255) as u8;
                    let px = ((br as u16 >> 3) << 11) | ((bg as u16 >> 2) << 5) | (bb as u16 >> 3);
                    dst_buf[di] = px as u8;
                    dst_buf[di + 1] = (px >> 8) as u8;
                }
            }
        }
    }
}

/// DDA (digital differential analyzer) blit: one divide per axis at
/// the top of the function, then each row/column just adds the step.
/// Step is stored in Q16.16 so the high word lands on an integer src
/// sample index and the low word carries the fractional error across
/// iterations — identical sampling result to `(drow * sh) / dh` but
/// without RV32's software divide in the inner loop.
#[allow(clippy::too_many_arguments)]
pub fn blit_dda(
    dst: &mut Texture,
    src: &Texture,
    sx0: i32,
    sy0: i32,
    sw: u16,
    sh: u16,
    dx0: i32,
    dy0: i32,
    dw: i32,
    dh: i32,
    clip_x0: i32,
    clip_y0: i32,
    clip_x1: i32,
    clip_y1: i32,
) {
    // Step values in Q16.16 fixed-point. `(sw << 16) / dw` lands the
    // integer portion in the high 16 bits; adding step each iteration
    // is exact modular arithmetic on a u32.
    let sx_step = ((sw as u32) << 16) / dw as u32;
    let sy_step = ((sh as u32) << 16) / dh as u32;

    let mut sy_acc: u32 = 0;
    for drow in 0..dh {
        let iy = dy0 + drow;
        if iy < clip_y0 || iy >= clip_y1 {
            sy_acc = sy_acc.wrapping_add(sy_step);
            continue;
        }
        let sy = sy0 + (sy_acc >> 16) as i32;
        sy_acc = sy_acc.wrapping_add(sy_step);

        let mut sx_acc: u32 = 0;
        for dcol in 0..dw {
            let ix = dx0 + dcol;
            if ix < clip_x0 || ix >= clip_x1 {
                sx_acc = sx_acc.wrapping_add(sx_step);
                continue;
            }
            let sx = sx0 + (sx_acc >> 16) as i32;
            sx_acc = sx_acc.wrapping_add(sx_step);

            let src_color = src.get_pixel(sx, sy);
            if src_color.a == 0 {
                continue;
            }
            if src_color.a == 255 {
                dst.set_pixel(ix, iy, &src_color);
            } else {
                dst.blend_pixel_int(ix, iy, &src_color, src_color.a);
            }
        }
    }
}
