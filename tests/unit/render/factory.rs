use super::*;
use crate::render::canvas::Paint;
use crate::render::path::Path;
use crate::render::raster::{FillRule, LineCap, LineJoin};
use crate::surface::framebuf::FramebufSurface;
use crate::types::{Color, Fixed, Rect};

#[test]
fn software_factory_retains_path_and_clip_buffers_between_frames() {
    let mut surface = FramebufSurface::new(64, 64, |_, _| {});
    let mut factory = SwRendererFactory::new();
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let clip = Rect::new(0, 0, 64, 64);
    let path = Path::rect(8.into(), 8.into(), 40.into(), 40.into());
    let paint = Paint::Color(Color::rgb(20, 30, 40).into());
    let dash = [Fixed::from_int(4), Fixed::from_int(2)];
    let mut first = None;

    for _ in 0..2 {
        let mut renderer = factory.make(&mut surface, &viewport);
        renderer.push_clip(&path, &crate::types::Transform::IDENTITY, FillRule::EvenOdd);
        renderer.fill_path(&path, &clip, &paint, 255, FillRule::EvenOdd);
        renderer.stroke_path(
            &path,
            &clip,
            Fixed::from_int(3),
            &paint,
            255,
            LineCap::Round,
            LineJoin::Round,
            Fixed::from_int(4),
            &dash,
        );
        renderer.pop_clip();
        drop(renderer);

        let state = factory.scratch.retained_buffer_state();
        if let Some(first) = first {
            assert_eq!(state, first);
        } else {
            first = Some(state);
        }
    }
}

#[test]
fn software_factory_releases_retained_buffers_on_memory_warning() {
    fn flush(_: &[u8], _: crate::types::PhysicalRect) {}
    let mut surface = FramebufSurface::new(64, 64, flush as fn(&[u8], crate::types::PhysicalRect));
    let mut factory = SwRendererFactory::new();
    assert!(factory.scratch.has_no_retained_buffers());
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let clip = Rect::new(0, 0, 64, 64);
    let path = Path::rect(8.into(), 8.into(), 40.into(), 40.into());
    let paint = Paint::Color(Color::rgb(20, 30, 40).into());
    let dash = [Fixed::from_int(4), Fixed::from_int(2)];

    let mut renderer = factory.make(&mut surface, &viewport);
    renderer.push_clip(&path, &crate::types::Transform::IDENTITY, FillRule::EvenOdd);
    renderer.fill_path(&path, &clip, &paint, 255, FillRule::EvenOdd);
    renderer.stroke_path(
        &path,
        &clip,
        Fixed::from_int(3),
        &paint,
        255,
        LineCap::Round,
        LineJoin::Round,
        Fixed::from_int(4),
        &dash,
    );
    renderer.pop_clip();
    drop(renderer);
    assert!(!factory.scratch.has_no_retained_buffers());

    <SwRendererFactory as RendererFactory<
        FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
    >>::trim_memory(&mut factory);
    assert!(factory.scratch.has_no_retained_buffers());
}
