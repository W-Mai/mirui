use super::*;
use crate::render::{path::Path, raster::FillRule};
use crate::types::{Rect, Transform};
use alloc::borrow::Cow;
use mirx::scene::{GradientUnits, RadialGradient};

fn paint(end: Color) -> Paint {
    let zero = mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO);
    Paint::RadialGradient(RadialGradient {
        center: zero,
        focal: zero,
        radius: mirx::types::Fixed::from_int(20),
        focal_radius: mirx::types::Fixed::ZERO,
        stops: Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgba(255, 255, 255, 255)
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: end.into()
            }
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::UserSpaceOnUse,
        transform: mirx::types::Transform::IDENTITY,
    })
}

pub(crate) fn with_plan<R>(end: Color, f: impl FnOnce(&Radial<'_>) -> R) -> R {
    let path = Path::rounded_rect(
        (-70).into(),
        (-70).into(),
        140.into(),
        140.into(),
        Fixed::ZERO,
    );
    let paint = paint(end);
    let command = DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        opa: 255,
        fill_rule: FillRule::NonZero,
    };
    let request = DrawRequest::new(&command, Rect::new(-100, -100, 200, 200));
    f(&plan(&request, Viewport::new(200, 200, Fixed::ONE)).unwrap())
}

#[test]
fn palette_generation_preserves_quantized_samples_and_stride_without_allocations() {
    let path = Path::rect(0.into(), 0.into(), 100.into(), 100.into());
    for radius in [
        mirx::types::Fixed::from_ratio(1, 2),
        mirx::types::Fixed::from_ratio(133, 256),
        mirx::types::Fixed::from_ratio(230, 256),
        mirx::types::Fixed::from_int(20),
        mirx::types::Fixed::from_int(2048),
    ] {
        let mut paint = paint(Color::rgba(17, 43, 91, 3));
        let Paint::RadialGradient(gradient) = &mut paint else {
            unreachable!()
        };
        gradient.radius = radius;
        let command = DrawCommand::FillPath {
            path: &path,
            paint: &paint,
            transform: Transform::IDENTITY,
            opa: 255,
            fill_rule: FillRule::NonZero,
        };
        let mut plan = plan(
            &DrawRequest::new(&command, Rect::new(0, 0, 100, 100)),
            Viewport::new(100, 100, Fixed::ONE),
        )
        .unwrap();
        for size in [32, 64, 256, 512, MAX_PALETTE_SIZE] {
            plan.key.size = size as u16;
            let pitch = size * 4 + 12;
            let mut pixels = alloc::vec![0xA5; pitch * size + 32];
            assert_eq!(
                crate::test_support::tracked_allocations(|| plan
                    .write_pixels(&mut pixels, pitch)
                    .unwrap()),
                0
            );
            for y in 0..size {
                for x in 0..size {
                    let color = plan.texel(x, y);
                    let offset = y * pitch + x * 4;
                    assert_eq!(
                        pixels[offset..offset + 4],
                        [color.r, color.g, color.b, color.a],
                        "radius={radius:?} size={size} x={x} y={y}"
                    );
                }
                assert!(
                    pixels[y * pitch + size * 4..(y + 1) * pitch]
                        .iter()
                        .all(|&value| value == 0xA5)
                );
            }
            assert!(pixels[pitch * size..].iter().all(|&value| value == 0xA5));
        }
    }
}

#[test]
fn palette_generation_rejects_invalid_storage_before_writing() {
    with_plan(Color::rgba(1, 2, 3, 4), |plan| {
        let size = usize::from(plan.key.size);
        let mut pixels = alloc::vec![0xA5; size * size * 4];
        for pitch in [size * 4 - 1, size * 4 + 1, usize::MAX] {
            assert_eq!(
                plan.write_pixels(&mut pixels, pitch),
                Err(RenderError::InvalidTexture)
            );
            assert!(pixels.iter().all(|&value| value == 0xA5));
        }
        assert_eq!(
            plan.write_pixels(&mut pixels[..size * size * 4 - 1], size * 4),
            Err(RenderError::InvalidTexture)
        );
    });
}

#[test]
fn radial_admission_keeps_focal_spread_sharp_and_quantized_fields_on_exact_fallback() {
    let path = Path::rect(0.into(), 0.into(), 100.into(), 100.into());
    let accepts = |paint: &Paint| {
        let command = DrawCommand::FillPath {
            path: &path,
            paint,
            transform: Transform::IDENTITY,
            opa: 255,
            fill_rule: FillRule::NonZero,
        };
        plan(
            &DrawRequest::new(&command, Rect::new(0, 0, 100, 100)),
            Viewport::new(100, 100, Fixed::ONE),
        )
        .is_some()
    };
    let paint = paint(Color::rgba(0, 0, 0, 0));
    assert!(accepts(&paint));
    let Paint::RadialGradient(original) = paint else {
        unreachable!()
    };
    let mut gradient = original.clone();
    gradient.focal.x = mirx::types::Fixed::ONE;
    assert!(!accepts(&Paint::RadialGradient(gradient)));
    let mut gradient = original.clone();
    gradient.spread = SpreadMode::Reflect;
    assert!(!accepts(&Paint::RadialGradient(gradient)));
    let mut gradient = original.clone();
    gradient.radius = mirx::types::Fixed::from_ratio(1, 256);
    assert!(!accepts(&Paint::RadialGradient(gradient)));
    let mut gradient = original.clone();
    gradient.radius = mirx::types::Fixed::from_int(4096);
    assert!(!accepts(&Paint::RadialGradient(gradient)));
    let mut gradient = original;
    gradient.stops.to_mut()[1].offset = mirx::types::Fixed::from_ratio(1, 256);
    assert!(!accepts(&Paint::RadialGradient(gradient)));
}

#[test]
fn radial_pad_partition_preserves_area_and_uses_constant_outside_samples_without_hot_allocations() {
    with_plan(Color::rgba(10, 20, 30, 40), |plan| {
        let path = Path::rounded_rect(
            (-70).into(),
            (-70).into(),
            140.into(),
            140.into(),
            Fixed::ZERO,
        );
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
        let mut outside = 0;
        plan.emit(&mut tess, 173, |batch| {
            for tri in batch.chunks_exact(3) {
                let p =
                    [tri[0], tri[1], tri[2]].map(|v| (v.position.x as f64, v.position.y as f64));
                area += ((p[1].0 - p[0].0) * (p[2].1 - p[0].1)
                    - (p[2].0 - p[0].0) * (p[1].1 - p[0].1))
                    .abs()
                    * 0.5;
                for vertex in tri {
                    assert!((0.0..=1.0).contains(&vertex.tex_coord.x));
                    assert!((0.0..=1.0).contains(&vertex.tex_coord.y));
                    assert_eq!(vertex.color.a, 173);
                }
                if tri.iter().all(|v| {
                    v.tex_coord.x == 0.5 / f32::from(plan.key.size)
                        && v.tex_coord.y == 0.5 / f32::from(plan.key.size)
                }) {
                    outside += 1;
                }
            }
            Ok(())
        })
        .unwrap();
        assert!((area - 19600.0).abs() < 0.02, "{area}");
        assert!(outside > 0);
        for n in 0..usize::from(plan.key.size) {
            assert_eq!(plan.texel(0, n), plan.last);
        }
        assert_eq!(
            crate::test_support::tracked_allocations(|| plan
                .emit(&mut tess, 173, |_| Ok(()))
                .unwrap()),
            0
        );
    });
}
