use super::*;
use crate::render::texture::TexBuf;

#[test]
fn region_copies_match_pixel_writes_and_leave_padding_intact() {
    let formats = [
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
        ColorFormat::RGB888,
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
    ];
    for src_format in formats {
        let mut storage = [0; 48];
        let mut source = Texture::new(&mut storage, 3, 3, src_format).unwrap();
        source.stride = 16;
        let mut pixels = source.pixels_mut().unwrap();
        for y in 0..3 {
            for x in 0..3 {
                pixels.set_pixel(x, y, &Color::rgba(29 * x as u8, 103, 97 * y as u8, 61));
            }
        }
        let source = source.read_pixels().unwrap();
        for format in formats {
            let mut actual = [0xa5; 48];
            let mut expected = actual;
            let mut output = Texture::new(&mut actual, 3, 3, format).unwrap();
            output.stride = 16;
            let mut output = output.pixels_mut().unwrap();
            let region = Region::new(1, 0, 2, 2).unwrap();
            assert!(output.copy_region_from(&source, region, [2, 1]).is_err());
            assert!(output.bytes().iter().all(|v| *v == 0xa5));
            output.copy_region_from(&source, region, [0, 1]).unwrap();
            for y in 0..2 {
                for x in 0..2 {
                    format.write_pixel(
                        &mut expected,
                        (y + 1) * 16 + x * format.bytes_per_pixel(),
                        &source.get_pixel(x as i32 + 1, y as i32),
                    );
                }
            }
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn pixel_borrows_share_storage_without_allocating() {
    let mut texture = Texture::owned(2, 2, ColorFormat::RGBA8888).unwrap();
    let address = texture.buf.as_slice().as_ptr();
    let allocations = crate::test_support::tracked_allocations(|| {
        {
            let mut pixels = texture.pixels_mut().unwrap();
            assert_eq!(pixels.bytes().as_ptr(), address);
            pixels.set_pixel(1, 1, &Color::rgba(10, 20, 30, 255));
        }
        let first = texture.read_pixels().unwrap();
        let second = texture.pixels().unwrap().unwrap();
        assert_eq!(first.bytes().as_ptr(), address);
        assert_eq!(second.get_pixel(1, 1), Color::rgba(10, 20, 30, 255));
    });
    assert_eq!(allocations, 0);
    assert_eq!(texture.cache_revision, 1);
}

#[test]
fn read_only_pixels_never_become_owned_on_write() {
    let bytes = [0u8; 8];
    let mut texture = Texture::from_ref(&bytes, 2, 1, ColorFormat::RGBA8888).unwrap();
    assert!(matches!(
        texture.pixels_mut(),
        Err(RenderError::InvalidTexture)
    ));
    assert!(matches!(texture.buf, TexBuf::Ref(_)));
    assert_eq!(texture.cache_revision, 0);
    assert_eq!(
        texture.read_pixels().unwrap().bytes().as_ptr(),
        bytes.as_ptr()
    );
}

#[test]
fn guarded_access_preserves_padding_and_all_pixel_formats() {
    for format in [
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ] {
        let bpp = format.bytes_per_pixel();
        let mut storage = [0x6a; 32];
        let mut texture = Texture::new(&mut storage, 2, 2, format).unwrap();
        texture.stride = 12;
        let color = Color::rgba(255, 0, 255, 255);
        {
            let mut pixels = texture.pixels_mut().unwrap();
            assert_eq!(
                (
                    pixels.width(),
                    pixels.height(),
                    pixels.format(),
                    pixels.stride()
                ),
                (2, 2, format, 12)
            );
            pixels.set_pixel(1, 1, &color);
            pixels.set_pixel(-1, 0, &color);
            pixels.set_pixel(2, 0, &color);
            assert_eq!(pixels.get_pixel(1, 1), color);
        }
        let pixels = texture.read_pixels().unwrap();
        assert_eq!(pixels.get_pixel(1, 1), color);
        assert!(pixels.bytes()[..12 + bpp].iter().all(|byte| *byte == 0x6a));
        assert!(
            pixels.bytes()[12 + 2 * bpp..]
                .iter()
                .all(|byte| *byte == 0x6a)
        );
    }
}

#[test]
fn revision_exhaustion_disables_caching_instead_of_reusing_a_key() {
    let mut texture = Texture::owned(1, 1, ColorFormat::RGBA8888)
        .unwrap()
        .with_cache_revision(u64::MAX);
    texture.pixels_mut().unwrap().bytes_mut().fill(0);
    assert!(texture.transient);
    assert_eq!(texture.cache_revision, u64::MAX);
}

#[test]
fn padded_layout_checks_last_pixel_without_requiring_final_padding() {
    let bytes = [0u8; 14];
    let texture = Texture::from_ref(&bytes, 3, 2, ColorFormat::RGB565)
        .unwrap()
        .with_stride(8)
        .unwrap();
    let pixels = texture.read_pixels().unwrap();
    assert_eq!(pixels.stride(), 8);
    assert_eq!(pixels.get_pixel(2, 1), Color::rgb(0, 0, 0));
    for stride in [5, 9, usize::MAX] {
        assert!(matches!(
            Texture::from_ref(&bytes, 3, 2, ColorFormat::RGB565)
                .unwrap()
                .with_stride(stride),
            Err(RenderError::InvalidTexture)
        ));
    }
}
