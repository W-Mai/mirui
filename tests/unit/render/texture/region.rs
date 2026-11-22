use super::*;

#[test]
fn regions_preserve_coordinates_stride_and_storage_without_allocating() {
    for format in [
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ] {
        let mut bytes = [0xa5; 48];
        // The final row deliberately has no trailing padding.
        let len = 32 + 3 * format.bytes_per_pixel();
        let mut texture = Texture::new(&mut bytes[..len], 3, 3, format)
            .unwrap()
            .with_stride(16)
            .unwrap();
        texture
            .pixels_mut()
            .unwrap()
            .set_pixel(2, 2, &Color::rgb(255, 0, 255));
        let full = texture.pixels().unwrap().unwrap();
        let area = Region::new(1, 1, 2, 2).unwrap();
        let allocations = crate::test_support::tracked_allocations(|| {
            let region = texture.pixels_in(area).unwrap().unwrap();
            let mut cx = Context::from_waker(core::task::Waker::noop());
            let Poll::Ready(Ok(prepared)) = texture.poll_pixels_in(area, &mut cx) else {
                panic!("resident region must be immediately ready");
            };
            assert_eq!(prepared.region(), area);
            assert_eq!(prepared.get_pixel(2, 2), region.get_pixel(2, 2));
            let Poll::Ready(Ok(complete)) = texture.poll_read_pixels(&mut cx) else {
                panic!("resident pixels must be immediately ready");
            };
            assert_eq!(complete.bytes().as_ptr(), full.bytes().as_ptr());
            assert_eq!(region.region(), area);
            assert_eq!(region.format(), format);
            let pixels = region.pixels().unwrap();
            let borrowed = region.borrowed_pixels().unwrap();
            assert_eq!(borrowed.bytes().as_ptr(), pixels.bytes().as_ptr());
            assert_eq!(borrowed.get_pixel(1, 1), pixels.get_pixel(1, 1));
            let start = 16 + format.bytes_per_pixel();
            assert_eq!(pixels.bytes().as_ptr(), full.bytes()[start..].as_ptr());
            assert_eq!(pixels.bytes().len(), 16 + 2 * format.bytes_per_pixel());
            assert_eq!(
                (pixels.width(), pixels.height(), pixels.stride()),
                (2, 2, 16)
            );
            assert_eq!(pixels.get_pixel(1, 1), full.get_pixel(2, 2));
            assert_eq!(region.get_pixel(2, 2), Some(full.get_pixel(2, 2)));
            assert_eq!(region.get_pixel(0, 1), None);
            assert_eq!(region.get_pixel(3, 2), None);
            assert_eq!(region.get_pixel(1, u32::MAX), None);
            let mut tiles = region.tiles();
            let (rect, tile) = tiles.next().unwrap();
            assert_eq!(rect, area);
            assert_eq!(tile.bytes().as_ptr(), pixels.bytes().as_ptr());
            assert!(tiles.next().is_none());
        });
        assert_eq!(allocations, 0);
    }
}

#[test]
fn empty_and_invalid_regions_have_distinct_results() {
    let bytes = [0; 16];
    let texture = Texture::from_ref(&bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
    for area in [
        Region::new(0, 0, 0, 2).unwrap(),
        Region::new(2, 2, 0, 0).unwrap(),
        Region::new(0, 2, 2, 0).unwrap(),
    ] {
        let region = texture.pixels_in(area).unwrap().unwrap();
        assert!(region.pixels().is_none());
        assert!(region.borrowed_pixels().is_none());
        assert_eq!(region.tiles().count(), 0);
        assert_eq!(region.get_pixel(area.x(), area.y()), None);
    }
    for area in [
        Region::new(1, 0, 2, 1).unwrap(),
        Region::new(0, 2, 1, 1).unwrap(),
        Region::new(3, 0, 0, 0).unwrap(),
        Region::new(u32::MAX, 0, 0, 1).unwrap(),
    ] {
        assert!(matches!(
            texture.pixels_in(area),
            Err(RenderError::InvalidTexture)
        ));
    }
}

#[test]
fn borrowed_cached_pixels_keep_the_region_lease_and_budget() {
    use crate::render::PlaneLayout;
    use crate::render::budget::MemoryBudget;
    use crate::render::texture::access::CachedPixels;

    let layout = PlaneLayout::packed(2, 2, ColorFormat::RGBA8888).unwrap();
    let charged = layout.required_bytes() + CachedPixels::OWNER_BYTES;
    let budget = MemoryBudget::new(charged);
    let owner = CachedPixels::shared(layout, &budget).unwrap();
    let region = TextureRegion {
        region: Region::new(0, 0, 2, 2).unwrap(),
        format: ColorFormat::RGBA8888,
        pixels: Some(TexturePixels::cached(owner, layout)),
    };
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            let first = region.borrowed_pixels().unwrap();
            let second = region.borrowed_pixels().unwrap();
            assert!(core::ptr::eq(first, second));
            assert_eq!(first.bytes(), &[0; 16]);
            assert_eq!(first.get_pixel(1, 1), Color::rgba(0, 0, 0, 0));
            assert_eq!(budget.allocated(), charged);
        }),
        0
    );
    assert_eq!(budget.allocated(), charged);
    drop(region);
    assert_eq!(budget.allocated(), 0);
}
