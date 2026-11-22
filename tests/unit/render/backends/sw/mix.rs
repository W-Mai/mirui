use super::*;
use crate::render::texture::Texture;

fn make_rgba(fill: [u8; 4]) -> alloc::vec::Vec<u8> {
    let mut v = alloc::vec![0u8; 16 * 4];
    for px in v.chunks_exact_mut(4) {
        px.copy_from_slice(&fill);
    }
    v
}

#[test]
fn mix_zero_is_noop() {
    let mut a_buf = make_rgba([100, 100, 100, 255]);
    let original = a_buf.clone();
    let mut b_buf = make_rgba([200, 0, 0, 255]);
    let b = Texture::new(&mut b_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut a = Texture::new(&mut a_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    mix_inplace(&mut a.pixels_mut().unwrap(), &b.read_pixels().unwrap(), 0);
    assert_eq!(a.read_pixels().unwrap().bytes(), original.as_slice());
}

#[test]
fn mix_full_replaces_a_with_b() {
    let mut a_buf = make_rgba([100, 100, 100, 255]);
    let mut b_buf = make_rgba([200, 50, 75, 255]);
    let b = Texture::new(&mut b_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut a = Texture::new(&mut a_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    mix_inplace(&mut a.pixels_mut().unwrap(), &b.read_pixels().unwrap(), 255);
    for px in a.read_pixels().unwrap().bytes().chunks_exact(4) {
        assert_eq!(px, &[200, 50, 75, 255]);
    }
}

#[test]
fn mix_half_lands_between() {
    let mut a_buf = make_rgba([100, 100, 100, 255]);
    let mut b_buf = make_rgba([200, 0, 0, 255]);
    let b = Texture::new(&mut b_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut a = Texture::new(&mut a_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    mix_inplace(&mut a.pixels_mut().unwrap(), &b.read_pixels().unwrap(), 128);
    for px in a.read_pixels().unwrap().bytes().chunks_exact(4) {
        // Integer mix: (127 * a + 128 * b) / 255. Tolerance ±1 for
        // truncation; b is identical-rgb so all three channels
        // land at the same expected value modulo a/b values.
        assert!((px[0] as i32 - 150).abs() <= 1, "r={}", px[0]);
        assert!((px[1] as i32 - 50).abs() <= 1, "g={}", px[1]);
        assert!((px[2] as i32 - 50).abs() <= 1, "b={}", px[2]);
    }
}

#[test]
fn mix_size_mismatch_is_noop() {
    let mut a_buf = make_rgba([10, 10, 10, 255]);
    let original = a_buf.clone();
    let mut b_buf = alloc::vec![99u8; 8 * 4];
    let b = Texture::new(&mut b_buf, 2, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut a = Texture::new(&mut a_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    mix_inplace(&mut a.pixels_mut().unwrap(), &b.read_pixels().unwrap(), 128);
    assert_eq!(a.read_pixels().unwrap().bytes(), original.as_slice());
}
