use crate::draw::texture::Texture;

/// Same-position physical-pixel copy between two framebuffer slots.
/// Bounds must come from `Viewport::rect_to_physical_pixel_bounds`.
pub(crate) fn blit_region(dst: &mut Texture, src: &Texture, x0: i32, y0: i32, x1: i32, y1: i32) {
    assert_eq!(src.format, dst.format, "mirror src/dst format mismatch");
    assert_eq!(src.stride, dst.stride, "mirror src/dst stride mismatch");
    assert_eq!(src.width, dst.width, "mirror src/dst width mismatch");
    assert_eq!(src.height, dst.height, "mirror src/dst height mismatch");
    let x0 = x0.max(0) as usize;
    let y0 = y0.max(0) as usize;
    let x1 = (x1.max(0) as usize).min(src.width as usize);
    let y1 = (y1.max(0) as usize).min(src.height as usize);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let bpp = src.format.bytes_per_pixel();
    let row = (x1 - x0) * bpp;
    let stride = src.stride;
    let s = src.buf.as_slice();
    let d = dst.buf.as_mut_slice();
    for y in y0..y1 {
        let so = y * stride + x0 * bpp;
        let dofs = y * stride + x0 * bpp;
        d[dofs..dofs + row].copy_from_slice(&s[so..so + row]);
    }
}

/// In-place memmove of `[x0,x1) × [y0,y1)` by `(dx_phys, dy_phys)`.
/// Row order picked so source rows are never clobbered before read.
pub(crate) fn texture_scroll_in_place(
    tex: &mut Texture,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    dx_phys: i32,
    dy_phys: i32,
) {
    if dx_phys == 0 && dy_phys == 0 {
        return;
    }
    let target_w = tex.width as i32;
    let target_h = tex.height as i32;
    let sx0 = x0.max(0);
    let sy0 = y0.max(0);
    let sx1 = x1.min(target_w);
    let sy1 = y1.min(target_h);
    if sx1 <= sx0 || sy1 <= sy0 {
        return;
    }
    let bpp = tex.format.bytes_per_pixel();
    let stride = tex.stride;
    let buf = tex.buf.as_mut_slice();

    let row_iter: alloc::vec::Vec<i32> = if dy_phys >= 0 {
        (sy0..sy1).rev().collect()
    } else {
        (sy0..sy1).collect()
    };

    for src_y in row_iter {
        let dst_y = src_y + dy_phys;
        if dst_y < sy0 || dst_y >= sy1 {
            continue;
        }
        if dx_phys == 0 {
            let src_off = src_y as usize * stride + sx0 as usize * bpp;
            let dst_off = dst_y as usize * stride + sx0 as usize * bpp;
            let row_bytes = (sx1 - sx0) as usize * bpp;
            buf.copy_within(src_off..src_off + row_bytes, dst_off);
            continue;
        }
        let dst_x0 = (sx0 + dx_phys).max(sx0);
        let dst_x1 = (sx1 + dx_phys).min(sx1);
        if dst_x1 <= dst_x0 {
            continue;
        }
        let src_x0 = dst_x0 - dx_phys;
        let copy_w = (dst_x1 - dst_x0) as usize * bpp;
        let src_off = src_y as usize * stride + src_x0 as usize * bpp;
        let dst_off = dst_y as usize * stride + dst_x0 as usize * bpp;
        buf.copy_within(src_off..src_off + copy_w, dst_off);
    }
}
