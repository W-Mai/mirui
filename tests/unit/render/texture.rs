use super::*;

#[test]
fn output_receipt_follows_shared_pixel_ownership_without_copying() {
    use crate::core::cache::Handle;
    use crate::render::budget::MemoryBudget;

    let budget = MemoryBudget::new(128);
    let texture = Texture::owned(2, 2, ColorFormat::RGBA8888).unwrap();
    let address = texture.read_pixels().unwrap().bytes().as_ptr();
    let bytes = texture.cache_size();
    let allocation = budget.reserve(bytes).unwrap().commit(bytes).unwrap();
    let mut retained = None;
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            retained = Some(texture.with_allocation(allocation));
        }),
        0
    );
    let mut texture = retained.unwrap();
    assert_eq!(texture.read_pixels().unwrap().bytes().as_ptr(), address);
    assert_eq!(texture.cache_size(), bytes);
    assert_eq!(budget.allocated(), bytes);
    texture
        .pixels_mut()
        .unwrap()
        .set_pixel(1, 1, &Color::rgb(12, 34, 56));
    assert_eq!(texture.cache_revision, 1);
    assert_eq!(
        texture.read_pixels().unwrap().get_pixel(1, 1),
        Color::rgb(12, 34, 56)
    );
    let first = Handle::new(texture.with_stride(8).unwrap());
    let second = first.clone();
    drop(first);
    assert_eq!(budget.allocated(), bytes);
    assert_eq!(second.read_pixels().unwrap().bytes().as_ptr(), address);
    drop(second);
    assert_eq!(budget.available(), 128);
}

#[test]
fn cache_charges_owning_capacity_including_alignment_storage() {
    let mut bytes = Vec::with_capacity(64);
    bytes.resize(4, 0);
    let capacity = bytes.capacity();
    let texture = Texture::from_vec(bytes, 1, 1, ColorFormat::RGBA8888).unwrap();
    assert_eq!(texture.cache_size(), capacity);
    assert_eq!(texture.read_pixels().unwrap().bytes().len(), 4);

    let bytes = AlignedBytes::zeroed(4, 64).unwrap();
    let capacity = bytes.storage.capacity();
    let layout = PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap();
    let texture = Texture::from_layout(TexBuf::Aligned(bytes), layout, false);
    assert_eq!(texture.cache_size(), capacity);
    assert!(capacity >= 67);
    assert_eq!(texture.read_pixels().unwrap().bytes().len(), 4);
}

#[test]
fn static_assets_share_checked_layout_without_allocating() {
    static BYTES: [u8; 16] = [12; 16];
    const ASSET: Texture<'static> = Texture::expect_static(&BYTES, 2, 2, ColorFormat::RGBA8888);
    let allocations = crate::test_support::tracked_allocations(|| {
        let asset = ASSET;
        assert_eq!(
            asset.read_pixels().unwrap().bytes().as_ptr(),
            BYTES.as_ptr()
        );
        assert_eq!(asset.width(), 2);
        assert_eq!(asset.height(), 2);
        assert!(!asset.transient);
        for format in [
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB888,
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
        ] {
            let asset = Texture::expect_static(&BYTES, 2, 2, format);
            let checked = Texture::from_static(&BYTES, 2, 2, format).unwrap();
            assert_eq!(asset.pixel_layout(), checked.pixel_layout());
            assert_eq!(
                asset.read_pixels().unwrap().bytes().as_ptr(),
                BYTES.as_ptr()
            );
        }
    });
    assert_eq!(allocations, 0);
    assert!(
        std::panic::catch_unwind(|| {
            Texture::expect_static(&BYTES, 0, 2, ColorFormat::RGBA8888)
        })
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| {
            Texture::expect_static(&BYTES, 3, 2, ColorFormat::RGBA8888)
        })
        .is_err()
    );
}

#[test]
fn constructors_reject_invalid_storage_before_exposing_pixels() {
    const SHORT: Result<Texture<'static>, RenderError> =
        Texture::from_static(&[0; 3], 1, 1, ColorFormat::RGBA8888);
    const VALID: Result<Texture<'static>, RenderError> =
        Texture::from_static(&[10, 20, 30, 40], 1, 1, ColorFormat::RGBA8888);
    assert!(matches!(SHORT, Err(RenderError::InvalidTexture)));
    assert_eq!(
        VALID.unwrap().read_pixels().unwrap().get_pixel(0, 0),
        Color::rgba(10, 20, 30, 40)
    );
    for format in [
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ] {
        let mut bytes = [0; 4];
        let short = format.bytes_per_pixel() - 1;
        assert!(matches!(
            Texture::new(&mut bytes[..short], 1, 1, format),
            Err(RenderError::InvalidTexture)
        ));
        assert!(matches!(
            Texture::from_ref(&bytes[..short], 1, 1, format),
            Err(RenderError::InvalidTexture)
        ));
        assert!(matches!(
            Texture::from_vec(bytes[..short].to_vec(), 1, 1, format),
            Err(RenderError::InvalidTexture)
        ));
        assert!(matches!(
            Texture::owned(0, 1, format),
            Err(RenderError::InvalidTexture)
        ));
        assert!(matches!(
            Texture::owned(1, 0, format),
            Err(RenderError::InvalidTexture)
        ));
        let owned = Texture::owned(1, 1, format).unwrap();
        assert!(owned.buf.as_slice().iter().all(|byte| *byte == 0));
        let borrowed = Texture::from_ref(&bytes, 1, 1, format).unwrap();
        assert_eq!(borrowed.buf.as_slice().as_ptr(), bytes.as_ptr());
    }
}

#[test]
fn storage_validation_uses_the_last_pixel_of_padded_rows() {
    let mut texture =
        Texture::from_ref(&[0u8; 12], 2, 2, ColorFormat::RGB565).expect("valid pixel storage");
    texture.stride = 8;
    assert!(texture.valid_storage());
    texture.stride = 9;
    assert!(!texture.valid_storage());
    assert!(matches!(
        Texture::from_ref(&[], 0, 0, ColorFormat::RGBA8888),
        Err(RenderError::InvalidTexture)
    ));
}

#[test]
fn owned_pixel_bytes_move_into_texture_without_copying() {
    let bytes = alloc::vec![7u8; 16];
    let address = bytes.as_ptr();
    let texture = Texture::from_vec(bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
    assert_eq!(texture.buf.as_slice().as_ptr(), address);
    assert_eq!(texture.stride, 8);
    assert!(texture.transient);
    assert!(Texture::from_vec(alloc::vec![0u8; 15], 2, 2, ColorFormat::RGBA8888).is_err());
}

#[test]
fn argb8888_roundtrip() {
    let mut buf = [0u8; 4];
    let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
    let c = Color::rgba(100, 200, 50, 255);
    tex.pixels_mut().unwrap().set_pixel(0, 0, &c);
    assert_eq!(tex.read_pixels().unwrap().get_pixel(0, 0), c);
}

#[test]
fn bgra8888_roundtrip() {
    let mut buf = [0u8; 4];
    let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::BGRA8888).expect("valid pixel storage");
    let c = Color::rgba(100, 200, 50, 255);
    tex.pixels_mut().unwrap().set_pixel(0, 0, &c);
    assert_eq!(tex.read_pixels().unwrap().get_pixel(0, 0), c);
    assert_eq!(buf, [c.b, c.g, c.r, c.a]);
}

#[test]
fn bgra8888_pack_byte_order() {
    let c = Color::rgba(0xAA, 0xBB, 0xCC, 0xDD);
    let bgra = ColorFormat::BGRA8888.pack(&c);
    // LE u32: byte 0=B, 1=G, 2=R, 3=A.
    assert_eq!(bgra & 0xFF, c.b as u32);
    assert_eq!((bgra >> 8) & 0xFF, c.g as u32);
    assert_eq!((bgra >> 16) & 0xFF, c.r as u32);
    assert_eq!((bgra >> 24) & 0xFF, c.a as u32);
}

#[test]
fn rgb565_roundtrip() {
    let mut buf = [0u8; 2];
    let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGB565).expect("valid pixel storage");
    let c = Color::rgb(255, 255, 255);
    tex.pixels_mut().unwrap().set_pixel(0, 0, &c);
    let got = tex.read_pixels().unwrap().get_pixel(0, 0);
    assert_eq!(got.r, c.r);
    assert_eq!(got.g, c.g);
    assert_eq!(got.b, c.b);
}

#[test]
fn blend_50_percent() {
    let mut buf = [0u8; 4];
    let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
    tex.pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &Color::rgb(0, 0, 0));
    tex.pixels_mut()
        .unwrap()
        .blend_pixel(Fixed::ZERO, Fixed::ZERO, &Color::rgb(200, 100, 50), 128);
    let got = tex.read_pixels().unwrap().get_pixel(0, 0);
    assert!((got.r as i32 - 100).abs() <= 1);
    assert!((got.g as i32 - 50).abs() <= 1);
    assert!((got.b as i32 - 25).abs() <= 1);
}

#[test]
fn blend_mode_keeps_straight_rgb_over_a_transparent_target() {
    let mut buf = [0u8; 4];
    let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
    tex.alpha_mode = AlphaMode::Blend;
    tex.pixels_mut()
        .unwrap()
        .blend_pixel_int(0, 0, &Color::rgba(0, 0, 255, 128), 128);
    assert_eq!(
        tex.read_pixels().unwrap().get_pixel(0, 0),
        Color::rgba(0, 0, 255, 128)
    );
    tex.pixels_mut()
        .unwrap()
        .blend_pixel_int(0, 0, &Color::rgba(255, 0, 0, 128), 128);
    let pixel = tex.read_pixels().unwrap().get_pixel(0, 0);
    assert!((pixel.r as i32 - 170).abs() <= 1);
    assert_eq!(pixel.g, 0);
    assert!((pixel.b as i32 - 85).abs() <= 1);
    assert!((pixel.a as i32 - 192).abs() <= 1);
}

#[test]
fn rgba8_upload_conversion_ignores_stride_padding_in_each_format() {
    let mut rgba = Texture::from_ref(
        &[1, 2, 3, 4, 99, 99, 99, 99, 5, 6, 7, 8],
        1,
        2,
        ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    rgba.stride = 8;
    assert_eq!(
        rgba.read_pixels()
            .unwrap()
            .rgba8_upload_pixels()
            .unwrap()
            .as_ref(),
        &[1, 2, 3, 4, 5, 6, 7, 8]
    );

    let mut bgra = Texture::from_ref(
        &[3, 2, 1, 4, 99, 99, 99, 99, 7, 6, 5, 8],
        1,
        2,
        ColorFormat::BGRA8888,
    )
    .expect("valid pixel storage");
    bgra.stride = 8;
    assert_eq!(
        bgra.read_pixels().unwrap().rgba8_upload_pixels(),
        rgba.read_pixels().unwrap().rgba8_upload_pixels()
    );

    let mut rgb = Texture::from_ref(&[1, 2, 3, 99, 5, 6, 7], 1, 2, ColorFormat::RGB888)
        .expect("valid pixel storage");
    rgb.stride = 4;
    assert_eq!(
        rgb.read_pixels()
            .unwrap()
            .rgba8_upload_pixels()
            .unwrap()
            .as_ref(),
        &[1, 2, 3, 255, 5, 6, 7, 255]
    );

    rgb.stride = 2;
    assert!(matches!(
        rgb.read_pixels(),
        Err(RenderError::InvalidTexture)
    ));
}

#[test]
fn rgba8_upload_borrows_packed_pixels_and_converts_padded_rows() {
    let pixels = [1, 2, 3, 4, 5, 6, 7, 8];
    let packed =
        Texture::from_ref(&pixels, 1, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    let allocations = crate::test_support::tracked_allocations(|| {
        let source = packed.read_pixels().unwrap();
        assert!(matches!(
            source.rgba8_upload_pixels(),
            Ok(Cow::Borrowed(bytes)) if bytes.as_ptr() == pixels.as_ptr()
        ));
    });
    assert_eq!(allocations, 0);

    let mut padded = Texture::from_ref(
        &[1, 2, 3, 4, 99, 99, 99, 99, 5, 6, 7, 8],
        1,
        2,
        ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    padded.stride = 8;
    assert!(matches!(
        padded.read_pixels().unwrap().rgba8_upload_pixels(),
        Ok(Cow::Owned(bytes)) if bytes == pixels
    ));
}

#[test]
fn composite_modes_keep_source_color_on_transparent_targets() {
    use crate::render::command::CompositeMode;

    for mode in [
        CompositeMode::Add,
        CompositeMode::Screen,
        CompositeMode::Multiply,
        CompositeMode::Darken,
        CompositeMode::Lighten,
        CompositeMode::Difference,
    ] {
        let mut buf = [0u8; 4];
        let mut tex =
            Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
        tex.alpha_mode = AlphaMode::Blend;
        tex.pixels_mut()
            .unwrap()
            .composite_pixel(0, 0, &Color::rgba(200, 80, 40, 128), mode);
        assert_eq!(
            tex.read_pixels().unwrap().get_pixel(0, 0),
            Color::rgba(200, 80, 40, 128),
            "{mode:?}"
        );
    }
}

#[test]
fn composite_mode_uses_destination_alpha_in_blend_result() {
    use crate::render::command::CompositeMode;

    let mut buf = [0u8; 4];
    let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
    tex.alpha_mode = AlphaMode::Blend;
    tex.pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &Color::rgba(255, 0, 0, 128));
    tex.pixels_mut().unwrap().composite_pixel(
        0,
        0,
        &Color::rgba(0, 0, 255, 128),
        CompositeMode::Multiply,
    );
    let pixel = tex.read_pixels().unwrap().get_pixel(0, 0);
    assert!((pixel.r as i32 - 85).abs() <= 1);
    assert_eq!(pixel.g, 0);
    assert!((pixel.b as i32 - 85).abs() <= 1);
    assert!((pixel.a as i32 - 192).abs() <= 1);
}

#[test]
fn blend_rgb565() {
    let mut buf = [0u8; 2];
    let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGB565).expect("valid pixel storage");
    tex.pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &Color::rgb(0, 0, 0));
    tex.pixels_mut().unwrap().blend_pixel(
        Fixed::ZERO,
        Fixed::ZERO,
        &Color::rgb(255, 255, 255),
        255,
    );
    let got = tex.read_pixels().unwrap().get_pixel(0, 0);
    assert_eq!(got.r, 255);
    assert_eq!(got.g, 255);
    assert_eq!(got.b, 255);
}

#[test]
fn blend_subpixel_spreads_to_neighbors() {
    // A point at (0.5, 0.5) should spread to all 4 pixels
    let mut buf = [0u8; 4 * 4]; // 2x2 RGBA8888
    let mut tex = Texture::new(&mut buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    tex.pixels_mut()
        .unwrap()
        .set_pixel(0, 0, &Color::rgb(0, 0, 0));
    tex.pixels_mut()
        .unwrap()
        .set_pixel(1, 0, &Color::rgb(0, 0, 0));
    tex.pixels_mut()
        .unwrap()
        .set_pixel(0, 1, &Color::rgb(0, 0, 0));
    tex.pixels_mut()
        .unwrap()
        .set_pixel(1, 1, &Color::rgb(0, 0, 0));

    tex.pixels_mut().unwrap().blend_pixel(
        Fixed::HALF,
        Fixed::HALF,
        &Color::rgb(255, 255, 255),
        255,
    );

    // Each pixel should get ~25% coverage
    let tl = tex.read_pixels().unwrap().get_pixel(0, 0);
    let tr = tex.read_pixels().unwrap().get_pixel(1, 0);
    let bl = tex.read_pixels().unwrap().get_pixel(0, 1);
    let br = tex.read_pixels().unwrap().get_pixel(1, 1);
    // All should be non-zero (got some coverage)
    assert!(tl.r > 0, "top-left should have coverage");
    assert!(tr.r > 0, "top-right should have coverage");
    assert!(bl.r > 0, "bottom-left should have coverage");
    assert!(br.r > 0, "bottom-right should have coverage");
    // Sum should be ~255
    // 24.8 fixed-point: 3 multiplications + 1 division per pixel, 4 pixels
    // max accumulated error ≈ 4 * 3/256 * 255 ≈ 12
    let sum = tl.r as u16 + tr.r as u16 + bl.r as u16 + br.r as u16;
    assert!(
        (sum as i32 - 255).abs() <= 12,
        "total coverage sum={sum} should be ~255 (±12 for 24.8 precision)"
    );
}

use crate::core::resource::HasProbe;

fn build_flat_rgb565_2x1() -> &'static [u8] {
    let bytes = mirx::Document::new_flat(mirx::image::ImageAsset::new(
        2,
        1,
        mirx::image::ColorFormat::RGB565,
        4,
        alloc::borrow::Cow::Borrowed(&[0xAA, 0xBB, 0xCC, 0xDD]),
    ))
    .unwrap()
    .finish()
    .unwrap()
    .into_owned();
    Box::leak(bytes.into_boxed_slice())
}

#[derive(Clone, Copy)]
enum TestCoding {
    Pixel,
    Rle,
    Lz4,
    FrequencyReversible,
    FrequencyQuantized,
}

fn build_encoded_rgb(coding: TestCoding) -> &'static [u8] {
    use mirx::{
        Document,
        coding::{Frequency, FrequencyGeometry, Lz4, Pixel, Rle},
        image::{ColorDescription, EncodedImageAsset, SampleLayout, SurfaceDescriptor},
    };

    let samples = [17, 42, 91, 111, 7, 203];
    let surface =
        SurfaceDescriptor::new(2, 1, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
    let mut encoded = [0; 512];
    let mut frequency_params = [0];
    let (record, len) = match coding {
        TestCoding::Pixel => {
            let codec = Pixel::new(SampleLayout::RGB888).unwrap();
            (
                codec.record(),
                codec.encode_into(&samples, &mut encoded).unwrap(),
            )
        }
        TestCoding::Rle => {
            let codec = Rle::new().with_element_size(3).unwrap();
            (
                codec.record(),
                codec.encode_into(&samples, &mut encoded).unwrap(),
            )
        }
        TestCoding::Lz4 => {
            let codec = Lz4::new();
            let mut table = [0; Lz4::TABLE_LEN];
            let len = codec
                .encoder(&mut table)
                .unwrap()
                .encode_into(&samples, &mut encoded)
                .unwrap();
            (codec.record(), len)
        }
        TestCoding::FrequencyReversible => {
            let codec = Frequency::reversible();
            let geometry = FrequencyGeometry::for_plane(SampleLayout::RGB888, 0, 2, 1).unwrap();
            (
                codec.record_into(&mut frequency_params),
                codec.encode_into(geometry, &samples, &mut encoded).unwrap(),
            )
        }
        TestCoding::FrequencyQuantized => {
            let codec = Frequency::quantized(50).unwrap();
            let geometry = FrequencyGeometry::for_plane(SampleLayout::RGB888, 0, 2, 1).unwrap();
            (
                codec.record_into(&mut frequency_params),
                codec.encode_into(geometry, &samples, &mut encoded).unwrap(),
            )
        }
    };
    let image = EncodedImageAsset::new(surface, record, &encoded[..len]);
    let mut document = Document::new();
    let id = document.push_encoded_image(&image).unwrap();
    document.set_primary(id).unwrap();
    Box::leak(
        document
            .encode(&Default::default())
            .unwrap()
            .into_boxed_slice(),
    )
}

#[test]
fn from_mirx_flat_rgb565_round_trip() {
    let tex = Texture::from_mirx(build_flat_rgb565_2x1()).expect("flat parse");
    assert_eq!(tex.width, 2);
    assert_eq!(tex.height, 1);
    assert_eq!(tex.format, ColorFormat::RGB565);
    assert_eq!(tex.alpha_mode, AlphaMode::Opaque);
    assert_eq!(tex.buf.as_slice(), &[0xAA, 0xBB, 0xCC, 0xDD]);
}

#[test]
fn from_mirx_zero_copy_borrow() {
    let bytes = build_flat_rgb565_2x1();
    let tex = Texture::from_mirx(bytes).unwrap();
    let pix_ptr = tex.buf.as_slice().as_ptr();
    let buf_ptr = bytes.as_ptr();
    assert_eq!(pix_ptr as usize - buf_ptr as usize, 28);
}

#[test]
fn raw_mirx_with_explicit_reconstruction_owns_the_output() {
    let bytes = build_flat_rgb565_2x1();
    let options = MirxTextureOptions::new().with_input_memory(mirx::image::MemoryPlacement::Flash);
    let tex = Texture::from_mirx_with(bytes, options).unwrap();
    assert!(matches!(&tex.buf, TexBuf::Aligned(_)));
    assert_eq!(tex.buf.as_slice(), &[0xAA, 0xBB, 0xCC, 0xDD]);
}

#[test]
fn from_mirx_decodes_every_lossless_profile() {
    for coding in [
        TestCoding::Pixel,
        TestCoding::Rle,
        TestCoding::Lz4,
        TestCoding::FrequencyReversible,
    ] {
        let tex = Texture::from_mirx(build_encoded_rgb(coding)).unwrap();
        assert_eq!(tex.width, 2);
        assert_eq!(tex.height, 1);
        assert_eq!(tex.stride, 6);
        assert_eq!(tex.format, ColorFormat::RGB888);
        assert_eq!(tex.buf.as_slice(), &[17, 42, 91, 111, 7, 203]);
        assert!(matches!(tex.buf, TexBuf::Aligned(_)));
    }
}

#[test]
fn from_mirx_decodes_quantized_frequency_pixels() {
    let tex = Texture::from_mirx(build_encoded_rgb(TestCoding::FrequencyQuantized)).unwrap();
    assert_eq!((tex.width, tex.height, tex.stride), (2, 1, 6));
    assert_eq!(tex.format, ColorFormat::RGB888);
    assert_ne!(tex.buf.as_slice(), &[17, 42, 91, 111, 7, 203]);
    for (before, after) in [17u8, 42, 91, 111, 7, 203]
        .into_iter()
        .zip(tex.buf.as_slice().iter().copied())
    {
        assert!(before.abs_diff(after) <= 52);
    }
}

#[test]
fn mirx_plan_applies_gpu_width_stride_and_address_constraints() {
    let bytes = build_encoded_rgb(TestCoding::Rle);
    let requirements = mirx::image::SurfaceRequirements::new()
        .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
        .with_plane_alignment(mirx::types::ByteAlignment::new(64).unwrap())
        .with_width_multiple(64)
        .with_stride_multiple(64);
    let options = MirxTextureOptions::new().with_requirements(requirements);
    let tex = Texture::from_mirx_with(bytes, options).unwrap();
    assert_eq!(tex.width, 2);
    assert_eq!(tex.height, 1);
    assert_eq!(tex.stride, 192);
    assert_eq!(tex.buf.as_slice().as_ptr() as usize % 64, 0);
    assert_eq!(&tex.buf.as_slice()[..6], &[17, 42, 91, 111, 7, 203]);
    assert_eq!(&tex.buf.as_slice()[6..], &[0; 186]);
    let TexBuf::Aligned(storage) = tex.buf else {
        panic!("encoded output must own its aligned allocation")
    };
    assert_eq!(storage.alignment(), 64);
}

#[test]
fn mirx_plan_decodes_into_fixed_caller_storage() {
    #[repr(align(64))]
    struct Output([u8; 256]);

    #[repr(align(64))]
    struct Workspace([u8; 64]);

    let bytes = build_encoded_rgb(TestCoding::Pixel);
    let requirements = mirx::image::SurfaceRequirements::new()
        .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
        .with_stride_multiple(64);
    let options = MirxTextureOptions::new()
        .with_requirements(requirements)
        .with_input_memory(mirx::image::MemoryPlacement::Flash)
        .with_output_memory(mirx::image::MemoryPlacement::SharedNoncoherent)
        .with_workspace_memory(mirx::image::MemoryPlacement::SharedCoherent)
        .with_workspace_alignment(mirx::types::ByteAlignment::new(64).unwrap());
    let mut groups = [None];
    let plan = Texture::plan_mirx(bytes, options, &mut groups).unwrap();
    assert_eq!(plan.output_len(), 64);
    assert_eq!(plan.output_alignment(), 64);
    assert_eq!(plan.workspace_len(), 6);
    assert_eq!(plan.workspace_alignment(), 64);
    assert_eq!(plan.decode_request(), options.decode_request());
    assert_eq!(plan.input_sync(), mirx::image::CacheSync::None);
    assert_eq!(plan.output_sync(), mirx::image::CacheSync::CleanAfterWrite);
    let mut output = Output([0xa5; 256]);
    let mut workspace = Workspace([0x5a; 64]);
    let texture = plan.decode_into(&mut output.0, &mut workspace.0).unwrap();
    assert_eq!(texture.buf.as_slice().as_ptr() as usize % 64, 0);
    assert_eq!(texture.stride, 64);
    assert_eq!(&texture.buf.as_slice()[..6], &[17, 42, 91, 111, 7, 203]);
    assert_eq!(&texture.buf.as_slice()[6..], &[0; 58]);
    assert_eq!(&output.0[64..], &[0xa5; 192]);
    assert_eq!(&workspace.0[6..], &[0x5a; 58]);
}

#[test]
fn managed_mirx_load_rejects_external_memory_and_invalid_execution() {
    let bytes = build_encoded_rgb(TestCoding::Rle);
    let external =
        MirxTextureOptions::new().with_output_memory(mirx::image::MemoryPlacement::SharedCoherent);
    assert!(matches!(
        Texture::from_mirx_with(bytes, external),
        Err(MirxLoadError::ExternalMemoryRequired)
    ));

    let bad: &'static [u8] = Box::leak(Box::new([0u8; 8]));
    let direct =
        MirxTextureOptions::new().with_execution(mirx::image::DecodeExecution::DirectUpload);
    assert!(matches!(
        Texture::from_mirx_with(bad, direct),
        Err(MirxLoadError::Request(
            mirx::image::DecodeRequestError::UnsupportedExecution(
                mirx::image::DecodeExecution::DirectUpload
            )
        ))
    ));
}

#[test]
fn from_mirx_bad_magic_propagates_parse_error() {
    let bad: &'static [u8] = Box::leak(Box::new([0u8; 8]));
    assert!(matches!(
        Texture::from_mirx(bad),
        Err(MirxLoadError::Read(mirx::reader::ReadError::BadMagic))
    ));
}

#[test]
fn extract_meta_returns_geometry() {
    let tex = Texture::from_mirx(build_flat_rgb565_2x1()).unwrap();
    let meta = tex.extract_meta();
    assert_eq!(meta.width, 2);
    assert_eq!(meta.height, 1);
    assert_eq!(meta.format, ColorFormat::RGB565);
}

use crate::core::cache::MaxSize;
use crate::core::resource::ResourceManager;

fn texture_manager() -> ResourceManager<Texture<'static>> {
    let m = ResourceManager::<Texture<'static>>::new(
        MaxSize::Bytes(1024),
        Texture::from_mirx(build_flat_rgb565_2x1()).unwrap(),
    );
    m.enable_probes(
        MaxSize::Count(8),
        TextureMeta {
            width: 0,
            height: 0,
            format: ColorFormat::RGB565,
        },
    );
    m
}

#[test]
fn add_mirx_bytes_populates_probe_and_value() {
    let m = texture_manager();
    let bytes = build_flat_rgb565_2x1();
    m.add_mirx_bytes("logo", bytes).expect("register ok");

    assert_eq!(
        m.probe("logo"),
        Some(TextureMeta {
            width: 2,
            height: 1,
            format: ColorFormat::RGB565,
        })
    );

    let tex = m.resolve("logo");
    assert_eq!(tex.width, 2);
    assert_eq!(tex.format, ColorFormat::RGB565);
}

#[test]
fn add_mirx_bytes_probes_then_caches_decoded_storage() {
    let m = texture_manager();
    let bytes = build_encoded_rgb(TestCoding::Lz4);
    m.add_mirx_bytes("compressed", bytes).unwrap();

    assert_eq!(
        m.probe("compressed"),
        Some(TextureMeta {
            width: 2,
            height: 1,
            format: ColorFormat::RGB888,
        })
    );
    let first = m.resolve("compressed");
    let second = m.resolve("compressed");
    assert!(first.ptr_eq(&second));
    assert_eq!(first.buf.as_slice(), &[17, 42, 91, 111, 7, 203]);
    assert!(matches!(first.buf, TexBuf::Aligned(_)));
}

#[test]
fn add_mirx_bytes_rejects_bad_input() {
    let m = texture_manager();
    let bad: &'static [u8] = Box::leak(Box::new([0u8; 8]));
    let err = m.add_mirx_bytes("bad", bad).unwrap_err();
    assert!(matches!(
        err,
        MirxLoadError::Read(mirx::reader::ReadError::BadMagic)
    ));
}

#[test]
fn mirx_loader_resolves_via_chain() {
    let m = texture_manager();
    let bytes = build_flat_rgb565_2x1();
    m.add_loader(MirxLoader::new(move |t| {
        if t == "via-loader" { Some(bytes) } else { None }
    }));

    let tex = m.resolve("via-loader");
    assert_eq!(tex.width, 2);
    assert_eq!(tex.format, ColorFormat::RGB565);
}

#[test]
fn mirx_loader_returns_not_mine_when_fetch_misses() {
    let m = texture_manager();
    let bytes = build_flat_rgb565_2x1();
    m.add_loader(MirxLoader::new(move |t| {
        if t == "logo" { Some(bytes) } else { None }
    }));

    let tex = m.resolve("nope");
    assert_eq!(tex.width, 2, "unhandled tokens fall through to fallback");
}
