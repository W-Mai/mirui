use super::*;
use crate::render::command::DrawCommand;
use crate::render::raster::{LineCap, LineJoin};
use crate::render::renderer::{DrawRequest, RenderError, Renderer};
use crate::render::texture::{ColorFormat, Texture};
use alloc::borrow::Cow;
use mirx::scene::{GradientStop, GradientUnits, LinearGradient, RadialGradient, SpreadMode};

#[test]
fn transformed_linear_fill_tracks_path_and_paint_transforms() {
    let mut renderer = SwRenderer::new(
        Texture::owned(220, 44, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let path = Path::rect(0.into(), 0.into(), 100.into(), 20.into());
    let gradient = LinearGradient {
        start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
        end: mirx::types::Point::new(mirx::types::Fixed::from_int(100), mirx::types::Fixed::ZERO),
        stops: Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(0, 0, 0),
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(255, 0, 0),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::UserSpaceOnUse,
        transform: mirx::types::Transform::translate(
            mirx::types::Fixed::from_int(20),
            mirx::types::Fixed::ZERO,
        ),
    };
    renderer.fill_path_transformed(
        &path,
        Rect::new(0, 0, 220, 44),
        &Transform::scale(Fixed::from_int(2), Fixed::from_int(2)),
        &Paint::LinearGradient(gradient),
        255,
        FillRule::EvenOdd,
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(20, 20).r,
        0
    );
    assert!((126..=130).contains(&renderer.target.read_pixels().unwrap().get_pixel(140, 20).r));
}

#[test]
fn checked_linear_fill_rejects_empty_stops_before_drawing() {
    let mut renderer = SwRenderer::new(
        Texture::owned(32, 32, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let path = Path::rect(0.into(), 0.into(), 20.into(), 20.into());
    let paint = Paint::LinearGradient(LinearGradient {
        start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
        end: mirx::types::Point::new(mirx::types::Fixed::ONE, mirx::types::Fixed::ZERO),
        stops: Cow::Borrowed(&[]),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: mirx::types::Transform::IDENTITY,
    });
    let command = DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        opa: 255,
        fill_rule: FillRule::EvenOdd,
    };
    let clip = Rect::new(0, 0, 32, 32);
    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip)),
        Err(RenderError::InvalidGeometry)
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(10, 10).r,
        0
    );
}

#[test]
fn linear_stop_alpha_modulates_path_coverage() {
    let mut renderer = SwRenderer::new(
        Texture::owned(16, 16, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let path = Path::rect(0.into(), 0.into(), 16.into(), 16.into());
    let stop = mirx::types::Color::rgba(255, 0, 0, 128);
    let paint = Paint::LinearGradient(LinearGradient {
        start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
        end: mirx::types::Point::new(mirx::types::Fixed::from_int(16), mirx::types::Fixed::ZERO),
        stops: Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: stop,
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: stop,
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::UserSpaceOnUse,
        transform: mirx::types::Transform::IDENTITY,
    });
    renderer.fill_path_transformed(
        &path,
        Rect::new(0, 0, 16, 16),
        &Transform::IDENTITY,
        &paint,
        255,
        FillRule::EvenOdd,
    );
    assert!((126..=129).contains(&renderer.target.read_pixels().unwrap().get_pixel(8, 8).r));
}

#[test]
fn object_bbox_linear_stroke_uses_outline_bounds_for_flat_path() {
    let mut renderer = SwRenderer::new(
        Texture::owned(48, 24, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let mut path = Path::new();
    path.move_to(crate::types::Point::new(4, 12))
        .line_to(crate::types::Point::new(44, 12));
    let paint = Paint::LinearGradient(LinearGradient {
        start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
        end: mirx::types::Point::new(mirx::types::Fixed::ONE, mirx::types::Fixed::ZERO),
        stops: Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(0, 0, 0),
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(255, 0, 0),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: mirx::types::Transform::IDENTITY,
    });
    let command = DrawCommand::StrokePath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        width: Fixed::from_int(4),
        opa: 255,
        line_cap: LineCap::Butt,
        line_join: LineJoin::Miter,
        miter_limit: Fixed::from_int(4),
        dash: &[],
    };
    let clip = Rect::new(0, 0, 48, 24);
    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip)),
        Ok(())
    );
    assert!((100..=150).contains(&renderer.target.read_pixels().unwrap().get_pixel(24, 12).r));
}

#[test]
fn checked_radial_fill_renders_the_focal_circle() {
    let mut renderer = SwRenderer::new(
        Texture::owned(104, 54, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let path = Path::rect(0.into(), 0.into(), 100.into(), 50.into());
    let paint = Paint::RadialGradient(RadialGradient {
        center: mirx::types::Point::new(
            mirx::types::Fixed::from_ratio(1, 2),
            mirx::types::Fixed::from_ratio(1, 2),
        ),
        radius: mirx::types::Fixed::from_ratio(1, 2),
        focal: mirx::types::Point::new(
            mirx::types::Fixed::from_ratio(1, 4),
            mirx::types::Fixed::from_ratio(1, 2),
        ),
        focal_radius: mirx::types::Fixed::ZERO,
        stops: Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(0, 0, 0),
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(255, 0, 0),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: mirx::types::Transform::IDENTITY,
    });
    let command = DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        opa: 255,
        fill_rule: FillRule::EvenOdd,
    };
    let clip = Rect::new(0, 0, 104, 54);
    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip)),
        Ok(())
    );
    assert!((83..=87).contains(&renderer.target.read_pixels().unwrap().get_pixel(50, 25).r));
    assert!(renderer.target.read_pixels().unwrap().get_pixel(98, 25).r > 240);
}
