use super::*;
use crate::render::{path::Path, raster::FillRule};
use crate::types::{Fixed, Rect, Transform};
use alloc::borrow::Cow;
use mirx::scene::{GradientUnits, LinearGradient};

fn paint() -> Paint {
    Paint::LinearGradient(LinearGradient {
        start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
        end: mirx::types::Point::new(mirx::types::Fixed::from_int(100), mirx::types::Fixed::ZERO),
        stops: Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(255, 0, 0)
            },
            GradientStop {
                offset: mirx::types::Fixed::from_ratio(1, 4),
                color: mirx::types::Color::rgb(0, 255, 0)
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(0, 0, 255)
            },
        ]),
        units: GradientUnits::UserSpaceOnUse,
        spread: SpreadMode::Pad,
        transform: mirx::types::Transform::IDENTITY,
    })
}

#[test]
fn linear_bands_partition_mesh_without_crossing_color_stops_or_allocating_when_warm() {
    let paint = paint();
    let path = Path::rounded_rect((-20).into(), 0.into(), 140.into(), 100.into(), Fixed::ZERO);
    let command = DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        opa: 200,
        fill_rule: FillRule::NonZero,
    };
    let viewport = Viewport::new(160, 100, Fixed::ONE);
    let plan = linear(
        &DrawRequest::new(&command, Rect::new(0, 0, 160, 100)),
        viewport,
    )
    .unwrap();
    let mut tess = TessellationCache::new();
    tess.fill(
        path.commands(),
        None,
        &Color::rgb(255, 255, 255),
        255,
        FillRule::NonZero,
    )
    .unwrap();
    let mut area = 0.0_f64;
    plan.emit(&mut tess, 200, |batch| {
        assert!(batch.len() <= BATCH_VERTICES);
        for triangle in batch.chunks_exact(3) {
            let p = [triangle[0], triangle[1], triangle[2]]
                .map(|v| (v.position.x as f64, v.position.y as f64));
            area += ((p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1))
                .abs()
                * 0.5;
            let values = triangle
                .iter()
                .map(|v| plan.plane[0] * v.position.x as f64 + plan.plane[2]);
            let min = values.clone().fold(f64::INFINITY, f64::min);
            let max = values.fold(f64::NEG_INFINITY, f64::max);
            for edge in [0.0, 0.25, 1.0] {
                assert!(!(min < edge - 0.000001 && max > edge + 0.000001));
            }
            for vertex in triangle {
                assert_eq!(vertex.color.a, 200);
            }
        }
        Ok(())
    })
    .unwrap();
    assert!((area - 14000.0).abs() < 0.02, "{area}");
    assert_eq!(
        crate::test_support::tracked_allocations(|| plan.emit(&mut tess, 200, |_| Ok(())).unwrap()),
        0
    );
    assert_eq!(
        plan.emit(&mut tess, 200, |_| Err(RenderError::BackendFailure)),
        Err(RenderError::BackendFailure)
    );
}

#[test]
fn native_linear_admission_preserves_unsupported_spreads_and_hard_edges_for_fallback() {
    let mut paint = paint();
    let path = Path::rounded_rect(0.into(), 0.into(), 100.into(), 100.into(), 0.into());
    let accepted = |paint: &Paint, transform| {
        let cmd = DrawCommand::FillPath {
            path: &path,
            transform,
            paint,
            opa: 255,
            fill_rule: FillRule::NonZero,
        };
        linear(
            &DrawRequest::new(&cmd, Rect::new(0, 0, 100, 100)),
            Viewport::new(100, 100, Fixed::ONE),
        )
        .is_some()
    };
    assert!(accepted(&paint, Transform::rotate_deg(15.into())));
    assert!(!accepted(
        &paint,
        Transform::scale(Fixed::ZERO, Fixed::ZERO)
    ));
    let Paint::LinearGradient(ref mut gradient) = paint else {
        unreachable!()
    };
    gradient.spread = SpreadMode::Repeat;
    assert!(!accepted(&paint, Transform::IDENTITY));
    let Paint::LinearGradient(ref mut gradient) = paint else {
        unreachable!()
    };
    gradient.spread = SpreadMode::Pad;
    gradient.stops.to_mut()[1].offset = mirx::types::Fixed::ZERO;
    assert!(!accepted(&paint, Transform::IDENTITY));
}
