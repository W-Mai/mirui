extern crate std;
use super::*;
use crate::prelude::Point;
use crate::render::canvas::Canvas;
use crate::render::path::Path;
use crate::render::texture::{ColorFormat, Texture};
use mirx::scene::{GradientStop, GradientUnits, LinearGradient, SpreadMode};

fn linear_paint_obb() -> Paint {
    Paint::LinearGradient(LinearGradient {
        start: mirx::types::Point {
            x: Fixed::ZERO.into(),
            y: Fixed::ZERO.into(),
        },
        end: mirx::types::Point {
            x: Fixed::ONE.into(),
            y: Fixed::ONE.into(),
        },
        stops: std::borrow::Cow::Owned(std::vec![
            GradientStop {
                offset: Fixed::ZERO.into(),
                color: Color::rgb(50, 120, 255).into()
            },
            GradientStop {
                offset: Fixed::ONE.into(),
                color: Color::rgb(255, 70, 90).into()
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    })
}

#[test]
fn linear_object_bounding_box_maps_to_path_bbox() {
    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let clip = Rect::new(0, 0, 64, 64);

    let mut path = Path::new();
    path.move_to(Point {
        x: Fixed::from_int(10),
        y: Fixed::from_int(10),
    });
    path.line_to(Point {
        x: Fixed::from_int(50),
        y: Fixed::from_int(10),
    });
    path.line_to(Point {
        x: Fixed::from_int(50),
        y: Fixed::from_int(50),
    });
    path.line_to(Point {
        x: Fixed::from_int(10),
        y: Fixed::from_int(50),
    });
    path.close();

    let paint = linear_paint_obb();
    backend.fill_path(&path, &clip, &paint, 255, FillRule::EvenOdd);

    let tl = backend.target.read_pixels().unwrap().get_pixel(12, 12);
    let br = backend.target.read_pixels().unwrap().get_pixel(48, 48);
    std::eprintln!("tl={:?} br={:?}", tl, br);
    assert!(
        tl.b > tl.r,
        "top-left (near gradient start) should be blue-ish, got {:?}",
        tl
    );
    assert!(
        br.r > br.b,
        "bottom-right (near gradient end) should be red-ish, got {:?}",
        br
    );
}
