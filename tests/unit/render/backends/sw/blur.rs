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
    let mut tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    {
        let r = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(tex.width as i32),
            Fixed::from_int(tex.height as i32),
        );
        iir_blur_inplace(&mut tex.pixels_mut().unwrap(), Fixed::ZERO, r);
    }
    assert_eq!(tex.read_pixels().unwrap().bytes(), original.as_slice());
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
    let mut tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    {
        let r = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(tex.width as i32),
            Fixed::from_int(tex.height as i32),
        );
        iir_blur_inplace(
            &mut tex.pixels_mut().unwrap(),
            alpha_for_radius(Fixed::from_int(4)),
            r,
        );
    }
    for px in tex.read_pixels().unwrap().bytes().chunks_exact(4) {
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
    let mut tex = Texture::new(&mut buf, 9, 9, ColorFormat::RGBA8888).expect("valid pixel storage");
    {
        let r = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(tex.width as i32),
            Fixed::from_int(tex.height as i32),
        );
        iir_blur_inplace(
            &mut tex.pixels_mut().unwrap(),
            alpha_for_radius(Fixed::from_int(2)),
            r,
        );
    }
    let pixels = tex.read_pixels().unwrap();
    let after = pixels.bytes();
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

#[test]
fn iir_blur_rgb565_constant_red_stays_red() {
    let mut buf = alloc::vec::Vec::with_capacity(8 * 8 * 2);
    for _ in 0..(8 * 8) {
        buf.push(0x00);
        buf.push(0xF8);
    }
    let mut tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGB565).expect("valid pixel storage");
    {
        let r = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(tex.width as i32),
            Fixed::from_int(tex.height as i32),
        );
        iir_blur_inplace(
            &mut tex.pixels_mut().unwrap(),
            alpha_for_radius(Fixed::from_int(3)),
            r,
        );
    }
    for px in tex.read_pixels().unwrap().bytes().chunks_exact(2) {
        let pixel = ((px[1] as u16) << 8) | (px[0] as u16);
        let r5 = (pixel >> 11) & 0x1F;
        let g6 = (pixel >> 5) & 0x3F;
        let b5 = pixel & 0x1F;
        // 8-bit roundtrip allows ±1 here (31 → 30 acceptable).
        assert!(r5 >= 30, "r5 dropped: pixel=0x{pixel:04X}");
        assert!(g6 <= 1, "g6 leaked: pixel=0x{pixel:04X}");
        assert!(b5 <= 1, "b5 leaked: pixel=0x{pixel:04X}");
    }
}

#[test]
fn iir_blur_rgb565_centre_pixel_spreads() {
    let mut buf = alloc::vec![0; 9 * 9 * 2];
    let centre_x = 4;
    let centre_y = 4;
    let i = (centre_y * 9 + centre_x) * 2;
    buf[i] = 0xFF;
    buf[i + 1] = 0xFF;
    let mut tex = Texture::new(&mut buf, 9, 9, ColorFormat::RGB565).expect("valid pixel storage");
    {
        let r = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(tex.width as i32),
            Fixed::from_int(tex.height as i32),
        );
        iir_blur_inplace(
            &mut tex.pixels_mut().unwrap(),
            alpha_for_radius(Fixed::from_int(2)),
            r,
        );
    }
    let pixels = tex.read_pixels().unwrap();
    let after = pixels.bytes();
    let centre_pixel = ((after[i + 1] as u16) << 8) | (after[i] as u16);
    let centre_r5 = (centre_pixel >> 11) & 0x1F;
    assert!(
        centre_r5 < 31,
        "centre pixel should have dimmed, r5={centre_r5}"
    );
    let total: u64 = after
        .chunks_exact(2)
        .map(|p| {
            let pixel = ((p[1] as u16) << 8) | (p[0] as u16);
            let r5 = (pixel >> 11) & 0x1F;
            let g6 = (pixel >> 5) & 0x3F;
            let b5 = pixel & 0x1F;
            r5 as u64 + g6 as u64 + b5 as u64
        })
        .sum();
    assert!(total > 5, "blur sapped all energy");
}
