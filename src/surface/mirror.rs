use crate::render::texture::Texture;
use crate::types::PhysicalRect;

/// Same-position physical-pixel copy between two framebuffer slots.
pub(crate) fn blit_region(dst: &mut Texture, src: &Texture, area: PhysicalRect) {
    assert_eq!(src.format, dst.format, "mirror src/dst format mismatch");
    assert_eq!(src.stride, dst.stride, "mirror src/dst stride mismatch");
    assert_eq!(src.width, dst.width, "mirror src/dst width mismatch");
    assert_eq!(src.height, dst.height, "mirror src/dst height mismatch");
    let x0 = usize::from(area.x());
    let y0 = usize::from(area.y());
    let x1 = usize::from(area.right());
    let y1 = usize::from(area.bottom());
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
    area: PhysicalRect,
    dx_phys: i32,
    dy_phys: i32,
) {
    if dx_phys == 0 && dy_phys == 0 {
        return;
    }
    let sx0 = i32::from(area.x());
    let sy0 = i32::from(area.y());
    let sx1 = i32::from(area.right());
    let sy1 = i32::from(area.bottom());
    let bpp = tex.format.bytes_per_pixel();
    let stride = tex.stride;
    let buf = tex.buf.as_mut_slice();

    let mut copy_row = |src_y: i32| {
        let dst_y = src_y + dy_phys;
        if dst_y < sy0 || dst_y >= sy1 {
            return;
        }
        if dx_phys == 0 {
            let src_off = src_y as usize * stride + sx0 as usize * bpp;
            let dst_off = dst_y as usize * stride + sx0 as usize * bpp;
            let row_bytes = (sx1 - sx0) as usize * bpp;
            buf.copy_within(src_off..src_off + row_bytes, dst_off);
            return;
        }
        let dst_x0 = (sx0 + dx_phys).max(sx0);
        let dst_x1 = (sx1 + dx_phys).min(sx1);
        if dst_x1 <= dst_x0 {
            return;
        }
        let src_x0 = dst_x0 - dx_phys;
        let copy_w = (dst_x1 - dst_x0) as usize * bpp;
        let src_off = src_y as usize * stride + src_x0 as usize * bpp;
        let dst_off = dst_y as usize * stride + dst_x0 as usize * bpp;
        buf.copy_within(src_off..src_off + copy_w, dst_off);
    };

    if dy_phys >= 0 {
        for src_y in (sy0..sy1).rev() {
            copy_row(src_y);
        }
    } else {
        for src_y in sy0..sy1 {
            copy_row(src_y);
        }
    }
}
