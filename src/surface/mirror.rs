use crate::render::RenderError;
use crate::render::texture::Texture;
use crate::types::PhysicalRect;

/// Same-position physical-pixel copy between two framebuffer slots.
pub(crate) fn blit_region(
    dst: &mut Texture,
    src: &Texture,
    area: PhysicalRect,
) -> Result<(), RenderError> {
    let src = src.read_pixels()?;
    if src.format() != dst.format() || src.width() != dst.width() || src.height() != dst.height() {
        return Err(RenderError::InvalidTexture);
    }
    if area.right() > src.width() || area.bottom() > src.height() {
        return Err(RenderError::InvalidGeometry);
    }
    let mut dst = dst.pixels_mut()?;
    let x0 = usize::from(area.x());
    let y0 = usize::from(area.y());
    let x1 = usize::from(area.right());
    let y1 = usize::from(area.bottom());
    let bpp = src.format().bytes_per_pixel();
    let row = (x1 - x0) * bpp;
    let src_stride = src.stride();
    let dst_stride = dst.stride();
    let s = src.bytes();
    let d = dst.bytes_mut();
    for y in y0..y1 {
        let so = y * src_stride + x0 * bpp;
        let dofs = y * dst_stride + x0 * bpp;
        d[dofs..dofs + row].copy_from_slice(&s[so..so + row]);
    }
    Ok(())
}

/// In-place memmove of `[x0,x1) × [y0,y1)` by `(dx_phys, dy_phys)`.
/// Row order picked so source rows are never clobbered before read.
pub(crate) fn texture_scroll_in_place(
    tex: &mut Texture,
    area: PhysicalRect,
    dx_phys: i32,
    dy_phys: i32,
) -> Result<(), RenderError> {
    if dx_phys == 0 && dy_phys == 0 {
        return Ok(());
    }
    if area.right() > tex.width() || area.bottom() > tex.height() {
        return Err(RenderError::InvalidGeometry);
    }
    let sx0 = i32::from(area.x());
    let sy0 = i32::from(area.y());
    let sx1 = i32::from(area.right());
    let sy1 = i32::from(area.bottom());
    let mut pixels = tex.pixels_mut()?;
    let bpp = pixels.format().bytes_per_pixel();
    let stride = pixels.stride();
    let buf = pixels.bytes_mut();

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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::texture::ColorFormat;

    #[test]
    fn mirror_copies_pixels_across_padded_rows_without_allocating() {
        for format in [
            ColorFormat::RGB565,
            ColorFormat::RGB888,
            ColorFormat::RGBA8888,
        ] {
            let bpp = format.bytes_per_pixel();
            let mut source_bytes = [0x39; 64];
            let mut target_bytes = [0xa5; 64];
            let source = Texture::new(&mut source_bytes[..24 + 3 * bpp], 3, 3, format)
                .unwrap()
                .with_stride(12)
                .unwrap();
            let mut target = Texture::new(&mut target_bytes[..32 + 3 * bpp], 3, 3, format)
                .unwrap()
                .with_stride(16)
                .unwrap()
                .with_cache_revision(41);
            let allocations = crate::test_support::tracked_allocations(|| {
                blit_region(&mut target, &source, PhysicalRect::new(1, 1, 2, 2).unwrap()).unwrap();
            });
            assert_eq!(allocations, 0);
            assert_eq!(target.cache_revision, 42);
            for (offset, byte) in target.read_pixels().unwrap().bytes().iter().enumerate() {
                let copied = offset / 16 >= 1 && (bpp..3 * bpp).contains(&(offset % 16));
                assert_eq!(*byte, if copied { 0x39 } else { 0xa5 });
            }
        }
    }

    #[test]
    fn transfers_reject_read_only_or_invalid_destinations_without_copying() {
        let bytes = [0u8; 16];
        let source = Texture::from_ref(&bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
        let mut readonly = Texture::from_ref(&bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
        let area = PhysicalRect::from_size(2, 2);
        assert_eq!(
            blit_region(&mut readonly, &source, area),
            Err(RenderError::InvalidTexture)
        );
        assert_eq!(
            texture_scroll_in_place(&mut readonly, area, 0, 1),
            Err(RenderError::InvalidTexture)
        );
        assert_eq!(
            readonly.read_pixels().unwrap().bytes().as_ptr(),
            bytes.as_ptr()
        );
        assert_eq!(readonly.cache_revision, 0);

        let mut target = Texture::owned(2, 2, ColorFormat::RGBA8888).unwrap();
        let outside = PhysicalRect::from_size(3, 2);
        assert_eq!(
            blit_region(&mut target, &source, outside),
            Err(RenderError::InvalidGeometry)
        );
        assert_eq!(
            texture_scroll_in_place(&mut target, outside, 0, 1),
            Err(RenderError::InvalidGeometry)
        );
        assert_eq!(target.cache_revision, 0);
        assert!(
            target
                .read_pixels()
                .unwrap()
                .bytes()
                .iter()
                .all(|byte| *byte == 0)
        );
    }

    #[test]
    fn scroll_advances_revision_once_and_preserves_padding() {
        let mut bytes = [0x72; 28];
        let mut target = Texture::new(&mut bytes, 2, 3, ColorFormat::RGB565)
            .unwrap()
            .with_stride(12)
            .unwrap()
            .with_cache_revision(7);
        let area = PhysicalRect::from_size(2, 3);
        let allocations = crate::test_support::tracked_allocations(|| {
            texture_scroll_in_place(&mut target, area, 0, 0).unwrap();
            assert_eq!(target.cache_revision, 7);
            texture_scroll_in_place(&mut target, area, 0, 1).unwrap();
            assert_eq!(target.cache_revision, 8);
        });
        assert_eq!(allocations, 0);
        assert_eq!(target.read_pixels().unwrap().bytes(), &[0x72; 28]);
    }
}
