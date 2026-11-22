extern crate std;
use super::*;
use crate::render::canvas::Canvas;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::types::{Point, Transform};

fn circle_path(cx: i32, cy: i32, r: i32) -> Path {
    let k = (r as f32 * 0.552_284_8) as i32;
    let mut path = Path::new();
    let cx_f = Fixed::from_int(cx);
    let cy_f = Fixed::from_int(cy);
    let r_f = Fixed::from_int(r);
    let k_f = Fixed::from_int(k);
    path.move_to(Point {
        x: cx_f + r_f,
        y: cy_f,
    });
    path.cubic_to(
        Point {
            x: cx_f + r_f,
            y: cy_f + k_f,
        },
        Point {
            x: cx_f + k_f,
            y: cy_f + r_f,
        },
        Point {
            x: cx_f,
            y: cy_f + r_f,
        },
    );
    path.cubic_to(
        Point {
            x: cx_f - k_f,
            y: cy_f + r_f,
        },
        Point {
            x: cx_f - r_f,
            y: cy_f + k_f,
        },
        Point {
            x: cx_f - r_f,
            y: cy_f,
        },
    );
    path.cubic_to(
        Point {
            x: cx_f - r_f,
            y: cy_f - k_f,
        },
        Point {
            x: cx_f - k_f,
            y: cy_f - r_f,
        },
        Point {
            x: cx_f,
            y: cy_f - r_f,
        },
    );
    path.cubic_to(
        Point {
            x: cx_f + k_f,
            y: cy_f - r_f,
        },
        Point {
            x: cx_f + r_f,
            y: cy_f - k_f,
        },
        Point {
            x: cx_f + r_f,
            y: cy_f,
        },
    );
    path.close();
    path
}

#[test]
fn fill_rect_inside_circular_clip_masks_outside_pixels() {
    let w = 32;
    let h = 32;
    let mut buf = std::vec![0u8; (w * h * 4) as usize];
    let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888)
        .expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let clip = Rect::new(0, 0, w, h);

    let circle = circle_path(16, 16, 10);
    backend.push_clip(&circle, &Transform::IDENTITY, FillRule::EvenOdd);

    backend.fill_rect(
        &Rect::new(0, 0, w, h),
        &clip,
        &Color::rgb(255, 0, 0),
        Fixed::ZERO,
        255,
    );

    backend.pop_clip();

    let center = backend.target.read_pixels().unwrap().get_pixel(16, 16);
    assert_eq!(
        center.r, 255,
        "center pixel (inside circle) should be filled red"
    );

    let corner = backend.target.read_pixels().unwrap().get_pixel(0, 0);
    assert_eq!(
        corner.r, 0,
        "corner pixel (outside circle) must be untouched, got r={}",
        corner.r
    );

    let edge = backend.target.read_pixels().unwrap().get_pixel(31, 16);
    assert_eq!(
        edge.r, 0,
        "right-edge pixel (outside circle) must be untouched, got r={}",
        edge.r
    );
}

#[test]
fn fill_rect_no_clip_stack_unchanged_behavior() {
    let w = 8;
    let h = 8;
    let mut buf = std::vec![0u8; (w * h * 4) as usize];
    let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888)
        .expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let clip = Rect::new(0, 0, w, h);

    backend.fill_rect(
        &Rect::new(0, 0, w, h),
        &clip,
        &Color::rgb(255, 0, 0),
        Fixed::ZERO,
        255,
    );

    assert_eq!(backend.target.read_pixels().unwrap().get_pixel(0, 0).r, 255);
    assert_eq!(backend.target.read_pixels().unwrap().get_pixel(7, 7).r, 255);
}
