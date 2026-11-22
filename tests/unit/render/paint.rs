use super::*;
use alloc::borrow::Cow;

#[test]
fn integer_axis_coordinates_preserve_lifting_and_wrapping() {
    let coefficients = [
        i64::MIN,
        i64::MIN + 1,
        -(1_i64 << 47),
        -65_537,
        -1,
        0,
        1,
        65_537,
        1_i64 << 47,
        i64::MAX,
    ];
    for raw in coefficients {
        let scale = Fixed64::from_ratio(raw, 65_536);
        for pixel in [
            i32::MIN,
            -8_388_609,
            -8_388_608,
            -65_535,
            -1,
            0,
            1,
            65_535,
            8_388_607,
            8_388_608,
            i32::MAX,
        ] {
            let original = scale.mul_wide(Fixed64::from_fixed(Fixed::from_int(pixel)));
            assert_eq!(
                scale.wrapping_mul_int(Fixed::from_int(pixel).to_int()),
                original
            );
        }
    }
}

#[test]
fn stop_boundaries_match_right_continuous_integer_interpolation() {
    let colors = [
        mirx::types::Color {
            r: 19,
            g: 231,
            b: 87,
            a: 255,
        },
        mirx::types::Color {
            r: 254,
            g: 23,
            b: 99,
            a: 17,
        },
        mirx::types::Color {
            r: 0,
            g: 127,
            b: 251,
            a: 180,
        },
        mirx::types::Color {
            r: 231,
            g: 5,
            b: 18,
            a: 0,
        },
    ];
    for offsets in [
        [0, 85, 85, 256],
        [17, 17, 17, 256],
        [64, 64, 64, 64],
        [0, 17, 128, 200],
        [256, 256, 256, 256],
    ] {
        let stops: [_; 4] = core::array::from_fn(|i| GradientStop {
            offset: mirx::types::Fixed::from_ratio(offsets[i], 256),
            color: colors[i],
        });
        for count in 1..=4 {
            let check = |raw: i64| {
                let selected = offsets[..count]
                    .iter()
                    .rposition(|offset| i64::from(*offset) * 256 <= raw);
                let expected = match selected {
                    None => colors[0].into(),
                    Some(index) if index + 1 == count => colors[index].into(),
                    Some(index) => {
                        let begin = i128::from(offsets[index]) * 256;
                        let width = i128::from(offsets[index + 1] - offsets[index]) * 256;
                        let amount = ((i128::from(raw) - begin) * 65_536) / width;
                        let channel = |a: u8, b: u8| {
                            ((i128::from(a) * 65_536 + (i128::from(b) - i128::from(a)) * amount)
                                / 65_536) as u8
                        };
                        let a = colors[index];
                        let b = colors[index + 1];
                        Color::rgba(
                            channel(a.r, b.r),
                            channel(a.g, b.g),
                            channel(a.b, b.b),
                            channel(a.a, b.a),
                        )
                    }
                };
                assert_eq!(
                    sample_stops(&stops[..count], Fixed64::from_ratio(raw, 65_536)),
                    expected,
                    "{offsets:?} {count} {raw}"
                );
            };
            for raw in (-257..=65_793).step_by(17) {
                check(raw);
            }
            for offset in &offsets[..count] {
                for neighbor in [-1, 0, 1] {
                    check(i64::from(*offset) * 256 + neighbor);
                }
            }
        }
    }
}

fn gradient(units: GradientUnits, transform: mirx::types::Transform) -> LinearGradient {
    LinearGradient {
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
        units,
        transform,
    }
}

fn radial(
    focal: mirx::types::Point,
    focal_radius: mirx::types::Fixed,
    transform: mirx::types::Transform,
) -> RadialGradient {
    RadialGradient {
        center: mirx::types::Point::new(
            mirx::types::Fixed::from_ratio(1, 2),
            mirx::types::Fixed::from_ratio(1, 2),
        ),
        radius: mirx::types::Fixed::from_ratio(1, 2),
        focal,
        focal_radius,
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
        transform,
    }
}

#[test]
fn integer_row_reuse_matches_independent_linear_sampling_in_any_order() {
    let transforms = [
        Transform::IDENTITY,
        Transform::translate(Fixed::from_ratio(-7, 3), Fixed::from_ratio(5, 4)),
        Transform::scale(Fixed::from_ratio(3, 2), Fixed::from_ratio(2, 3)),
        Transform {
            m10: Fixed::from_ratio(1, 3),
            ..Transform::IDENTITY
        },
        Transform {
            m00: Fixed::ZERO,
            m01: -Fixed::ONE,
            m10: Fixed::ONE,
            m11: Fixed::ZERO,
            ..Transform::IDENTITY
        },
        Transform::scale(-Fixed::ONE, Fixed::from_int(2)),
    ];
    for vertical in [false, true] {
        for mode in [SpreadMode::Pad, SpreadMode::Repeat, SpreadMode::Reflect] {
            for draw in transforms {
                let mut gradient = gradient(
                    GradientUnits::ObjectBoundingBox,
                    mirx::types::Transform::IDENTITY,
                );
                gradient.spread = mode;
                if vertical {
                    gradient.end.x = mirx::types::Fixed::ZERO;
                    gradient.end.y = mirx::types::Fixed::ONE;
                }
                let bbox = Rect::new(-3, -2, 11, 7);
                let paint = Paint::LinearGradient(gradient);
                let Paint::LinearGradient(gradient) = &paint else {
                    unreachable!()
                };
                let reference = LinearPaint::new(gradient, draw, bbox).unwrap();
                let mut sampler = GradientPaint::new(&paint, draw, bbox).unwrap();
                for y in [-7, 0, 3, 0, 12, -7, 5] {
                    for x in -20..41 {
                        let expected = reference.sample_point(Point::new(x, y));
                        assert_eq!(sampler.sample(x, y), expected);
                        let fractional = Point::new(
                            Fixed::from_ratio(x * 3 - 2, 8),
                            Fixed::from_ratio(y * 5 + 3, 8),
                        );
                        assert_eq!(
                            sampler.sample_point(fractional),
                            reference.sample_point(fractional)
                        );
                    }
                }
            }
        }
    }
    assert!(
        core::mem::size_of::<GradientPaint<'_>>()
            <= core::mem::size_of::<RadialPaint<'_>>() + core::mem::align_of::<RadialPaint<'_>>()
    );
    std::println!(
        "gradient_sampler_bytes={} radial_sampler_bytes={}",
        core::mem::size_of::<GradientPaint<'_>>(),
        core::mem::size_of::<RadialPaint<'_>>()
    );
}

#[test]
fn radial_scanlines_match_independent_sampling_across_row_order_and_transforms() {
    let center = mirx::types::Point::new(
        mirx::types::Fixed::from_ratio(1, 2),
        mirx::types::Fixed::from_ratio(1, 2),
    );
    let transforms = [
        Transform::IDENTITY,
        Transform::translate(Fixed::from_ratio(-7, 3), Fixed::from_ratio(5, 4)),
        Transform::scale(Fixed::from_ratio(3, 2), Fixed::from_ratio(2, 3)),
        Transform::scale(-Fixed::ONE, Fixed::from_int(2)),
        Transform {
            m01: Fixed::from_ratio(1, 3),
            ..Transform::IDENTITY
        },
        Transform {
            m00: Fixed::ZERO,
            m01: -Fixed::ONE,
            m10: Fixed::ONE,
            m11: Fixed::ZERO,
            ..Transform::IDENTITY
        },
    ];
    for mode in [SpreadMode::Pad, SpreadMode::Repeat, SpreadMode::Reflect] {
        for units in [
            GradientUnits::ObjectBoundingBox,
            GradientUnits::UserSpaceOnUse,
        ] {
            for draw in transforms {
                for paint_transform in [
                    mirx::types::Transform::IDENTITY,
                    mirx::types::Transform::translate(
                        mirx::types::Fixed::from_ratio(3, 7),
                        mirx::types::Fixed::from_ratio(-5, 9),
                    ),
                ] {
                    let mut gradient = radial(center, mirx::types::Fixed::ZERO, paint_transform);
                    gradient.spread = mode;
                    gradient.units = units;
                    if units == GradientUnits::UserSpaceOnUse {
                        gradient.radius = mirx::types::Fixed::from_int(23);
                    }
                    let bbox = Rect::new(-3, 4, 43, 29);
                    let paint = Paint::RadialGradient(gradient);
                    let Paint::RadialGradient(gradient) = &paint else {
                        unreachable!()
                    };
                    let original = RadialPaint::new(gradient, draw, bbox).unwrap();
                    let allocations = crate::test_support::tracked_allocations(|| {
                        let mut sampler = GradientPaint::new(&paint, draw, bbox).unwrap();
                        assert_eq!(
                            matches!(sampler, GradientPaint::AxisRadial(_)),
                            original.inverse.m01 == Fixed64::ZERO
                                && original.inverse.m10 == Fixed64::ZERO
                        );
                        for y in [4, 4, 17, -2, 31, 17, 4] {
                            for x in (-20..=80).rev() {
                                assert_eq!(sampler.sample(x, y), original.sample(x, y));
                                let point = Point::new(
                                    Fixed::from_ratio(x * 4 + 1, 4),
                                    Fixed::from_ratio(y * 4 - 1, 4),
                                );
                                assert_eq!(
                                    sampler.sample_point(point),
                                    original.sample_point(point)
                                );
                            }
                        }
                    });
                    assert_eq!(allocations, 0);
                }
            }
        }
    }
    for (focal, inner) in [
        (center, mirx::types::Fixed::from_ratio(1, 8)),
        (
            mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
            mirx::types::Fixed::ZERO,
        ),
    ] {
        let paint = Paint::RadialGradient(radial(focal, inner, mirx::types::Transform::IDENTITY));
        assert!(matches!(
            GradientPaint::new(&paint, Transform::IDENTITY, Rect::new(0, 0, 40, 30)),
            Some(GradientPaint::Radial(_))
        ));
    }
}

#[test]
fn bounded_gradient_divisions_match_wide_arithmetic_at_numeric_limits() {
    let maximum = Fixed64::from_int((1_i64 << 47) - 1) + Fixed64::from_ratio(65_535, 65_536);
    let denominators = [
        -Fixed64::from_ratio(1, 65_536),
        -Fixed64::from_ratio(1, 4),
        -Fixed64::ONE,
        -Fixed64::from_int(64),
        -maximum,
        Fixed64::from_int(-(1_i64 << 47)),
    ];
    let mut seed = 0x7391_A5D3_5E81_6CA7_u64;
    for index in 0..4096 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let discriminant = match index {
            0 => Fixed64::ZERO,
            1 => Fixed64::from_ratio(1, 65_536),
            2 => maximum,
            _ => Fixed64::from_ratio((seed >> 1) as i64, 65_536),
        };
        for quadratic in denominators {
            assert_eq!(
                centered_radial_parameter(quadratic, discriminant),
                Some((-discriminant.sqrt()).div_wide(quadratic))
            );
        }
    }
    assert_eq!(
        centered_radial_parameter(-Fixed64::ONE, -Fixed64::ONE),
        None
    );
    for width in [1, 2, 3, 17, 127, 255, 256] {
        let interval = Fixed64::from_ratio(width, 256);
        for fraction in 0..=65_536 {
            let numerator = Fixed64::from_ratio(fraction, 65_536).min(interval);
            assert_eq!(numerator / interval, numerator.div_wide(interval));
        }
    }
}

#[test]
fn integer_stop_colors_preserve_float_interpolation_and_normalized_offsets() {
    let check = |a: u8, b: u8, fraction: i64| {
        let amount = Fixed64::from_ratio(fraction, 65_536);
        let start = mirx::types::Color {
            r: a,
            g: b,
            b: a ^ 0x55,
            a: 255 - a,
        };
        let end = mirx::types::Color {
            r: b,
            g: a,
            b: b ^ 0xAA,
            a: 255 - b,
        };
        let channel = |start: u8, end: u8| {
            (f32::from(start) + (f32::from(end) - f32::from(start)) * amount.to_f32()) as u8
        };
        let reference = Color::rgba(
            channel(start.r, end.r),
            channel(start.g, end.g),
            channel(start.b, end.b),
            channel(start.a, end.a),
        );
        assert_eq!(interpolate_stop_color(start, end, amount), reference);
    };
    for a in 0..=255 {
        for b in 0..=255 {
            for fraction in [
                0, 1, 127, 255, 256, 257, 32_767, 32_768, 32_769, 65_279, 65_280, 65_535, 65_536,
            ] {
                check(a, b, fraction);
            }
        }
    }
    for (a, b) in [
        (0, 255),
        (255, 0),
        (1, 254),
        (254, 1),
        (127, 128),
        (128, 127),
    ] {
        for fraction in 0..=65_536 {
            check(a, b, fraction);
        }
    }
    for value in 0..=256 {
        let offset = mirx::types::Fixed::from_ratio(value, 256);
        assert_eq!(stop_offset(offset), Fixed64::from_fixed(offset.into()));
    }
}

#[test]
fn prepared_radial_endpoint_preserves_tiny_radii_and_square_boundaries() {
    let center = mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO);
    for radius in [
        mirx::types::Fixed::from_ratio(1, 256),
        mirx::types::Fixed::from_ratio(1, 2),
        mirx::types::Fixed::ONE,
        mirx::types::Fixed::from_int(2896),
        mirx::types::Fixed::from_int(2897),
    ] {
        for mode in [SpreadMode::Pad, SpreadMode::Repeat, SpreadMode::Reflect] {
            let mut gradient = radial(
                center,
                mirx::types::Fixed::ZERO,
                mirx::types::Transform::IDENTITY,
            );
            gradient.center = center;
            gradient.radius = radius;
            gradient.spread = mode;
            gradient.units = GradientUnits::UserSpaceOnUse;
            let bbox = Rect::new(0, 0, 64, 64);
            let original = RadialPaint::new(&gradient, Transform::IDENTITY, bbox).unwrap();
            let mut prepared = AxisRadialPaint::new(
                RadialPaint::new(&gradient, Transform::IDENTITY, bbox).unwrap(),
            );
            let radius: Fixed = radius.into();
            for y in [0, -2, 5, 0] {
                for x in [-4, 0, 7, -1, 0] {
                    assert_eq!(prepared.sample(x, y), original.sample(x, y));
                }
                for x in [
                    radius - Fixed::from_ratio(1, 256),
                    radius,
                    radius + Fixed::from_ratio(1, 256),
                ] {
                    let point = Point::new(x, Fixed::from_int(y));
                    assert_eq!(prepared.sample_point(point), original.sample_point(point));
                }
            }
        }
    }
    std::println!(
        "prepared_axis_bytes={} gradient_sampler_bytes={}",
        core::mem::size_of::<AxisRadialPaint<'_>>(),
        core::mem::size_of::<GradientPaint<'_>>()
    );
}

#[test]
fn object_bbox_and_paint_transform_map_into_physical_pixels() {
    let gradient = gradient(
        GradientUnits::ObjectBoundingBox,
        mirx::types::Transform::translate(
            mirx::types::Fixed::from_ratio(1, 4),
            mirx::types::Fixed::ZERO,
        ),
    );
    let sampler = LinearPaint::new(
        &gradient,
        Transform::scale(Fixed::from_int(2), Fixed::from_int(2)),
        Rect::new(10, 20, 100, 50),
    )
    .unwrap();
    assert!(sampler.sample(20, 40).r < 2);
    assert!((126..=130).contains(&sampler.sample(170, 40).r));
    assert!(sampler.sample(270, 40).r >= 254);
}

#[test]
fn projective_sampler_recovers_the_original_paint_coordinate() {
    let paint = Paint::LinearGradient(gradient(
        GradientUnits::ObjectBoundingBox,
        mirx::types::Transform::IDENTITY,
    ));
    let bbox = Rect::new(10, 20, 100, 50);
    let projection =
        Transform3D::rotate_y_perspective(Fixed::from_int(24), Fixed::from_int(280)).compose(
            &Transform3D::translate(Fixed::from_int(40), Fixed::from_int(12)),
        );
    let original = Point::new(60, 35);
    let projected = projection.apply_point(original).unwrap();
    let sampler = ProjectiveGradientPaint::new(&paint, projection, bbox).unwrap();
    let color = sampler.sample_point(projected);

    assert!((125..=130).contains(&color.r));
    assert_eq!(color.g, 0);
    assert_eq!(color.b, 0);
}

#[test]
fn duplicate_stops_form_a_hard_color_edge() {
    let half = mirx::types::Fixed::from_ratio(1, 2);
    let stops = [
        GradientStop {
            offset: mirx::types::Fixed::ZERO,
            color: mirx::types::Color::rgb(0, 0, 0),
        },
        GradientStop {
            offset: half,
            color: mirx::types::Color::rgb(255, 0, 0),
        },
        GradientStop {
            offset: half,
            color: mirx::types::Color::rgb(0, 0, 255),
        },
        GradientStop {
            offset: mirx::types::Fixed::ONE,
            color: mirx::types::Color::rgb(255, 255, 255),
        },
    ];
    assert!(GradientStop::sequence_is_valid(&stops));
    assert_eq!(sample_stops(&stops, Fixed64::from_ratio(1, 2)).b, 255);
    assert!(!GradientStop::sequence_is_valid(&[]));
    assert!(!GradientStop::sequence_is_valid(&[
        stops[2], stops[1], stops[0]
    ]));
}

#[test]
fn centered_radial_parameters_match_the_general_solver_exactly() {
    let center = mirx::types::Point::new(
        mirx::types::Fixed::from_ratio(1, 2),
        mirx::types::Fixed::from_ratio(1, 2),
    );
    let mut gradient = radial(
        center,
        mirx::types::Fixed::ZERO,
        mirx::types::Transform::IDENTITY,
    );
    for radius in [1, 2, 7, 31, 127, 128, 129, 256, 1024] {
        gradient.radius = mirx::types::Fixed::from_ratio(radius, 256);
        let sampler =
            RadialPaint::new(&gradient, Transform::IDENTITY, Rect::new(0, 0, 100, 50)).unwrap();
        for y in -35..=35 {
            for x in -35..=35 {
                let qx = Fixed64::from_ratio(x, 31);
                let qy = Fixed64::from_ratio(y, 23);
                assert_eq!(
                    sampler
                        .centered_discriminant(qx, qy)
                        .and_then(|d| centered_radial_parameter(sampler.quadratic, d)),
                    sampler.conical_parameter(qx, qy),
                    "radius={radius} x={x} y={y}",
                );
                let reference = sampler
                    .conical_parameter(qx, qy)
                    .map_or(Color::rgba(0, 0, 0, 0), |t| {
                        sample_stops(sampler.stops, spread(t, sampler.spread))
                    });
                assert_eq!(
                    sampler.sample_relative(qx, qy),
                    reference,
                    "padded radius={radius} x={x} y={y}"
                );
            }
        }
    }
}

#[test]
fn radial_endpoint_shortcut_preserves_repetition_and_large_square_boundaries() {
    let center = mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO);
    for radius in [1, 4096, 65536] {
        for mode in [SpreadMode::Pad, SpreadMode::Reflect, SpreadMode::Repeat] {
            let mut gradient = radial(
                center,
                mirx::types::Fixed::ZERO,
                mirx::types::Transform::IDENTITY,
            );
            gradient.center = center;
            gradient.units = GradientUnits::UserSpaceOnUse;
            gradient.radius = mirx::types::Fixed::from_int(radius);
            gradient.spread = mode;
            let sampler =
                RadialPaint::new(&gradient, Transform::IDENTITY, Rect::new(0, 0, 100, 100))
                    .unwrap();
            for (x, y) in [(0, 0), (1, 1), (4, 0), (-20, 3)] {
                let qx = Fixed64::from_int(x);
                let qy = Fixed64::from_int(y);
                let reference = sampler
                    .conical_parameter(qx, qy)
                    .map_or(Color::rgba(0, 0, 0, 0), |t| {
                        sample_stops(sampler.stops, spread(t, mode))
                    });
                assert_eq!(sampler.sample_relative(qx, qy), reference);
            }
        }
    }
}

#[test]
fn radial_focal_circle_and_object_bounds_map_as_an_ellipse() {
    let gradient = radial(
        mirx::types::Point::new(
            mirx::types::Fixed::from_ratio(1, 4),
            mirx::types::Fixed::from_ratio(1, 2),
        ),
        mirx::types::Fixed::ZERO,
        mirx::types::Transform::IDENTITY,
    );
    let sampler =
        RadialPaint::new(&gradient, Transform::IDENTITY, Rect::new(0, 0, 100, 50)).unwrap();
    assert_eq!(sampler.sample(25, 25).r, 0);
    assert!((83..=87).contains(&sampler.sample(50, 25).r));
    assert!(sampler.sample(100, 25).r >= 254);
    assert!(sampler.sample(50, 50).r >= 254);
}

#[test]
fn radial_paint_transform_moves_the_focal_circle() {
    let gradient = radial(
        mirx::types::Point::new(
            mirx::types::Fixed::from_ratio(1, 4),
            mirx::types::Fixed::from_ratio(1, 2),
        ),
        mirx::types::Fixed::ZERO,
        mirx::types::Transform::translate(
            mirx::types::Fixed::from_ratio(1, 4),
            mirx::types::Fixed::ZERO,
        ),
    );
    let sampler =
        RadialPaint::new(&gradient, Transform::IDENTITY, Rect::new(0, 0, 100, 50)).unwrap();
    assert_eq!(sampler.sample(50, 25).r, 0);
    assert!((83..=87).contains(&sampler.sample(75, 25).r));
}

#[test]
fn radial_focal_radius_sets_the_inner_circle() {
    let center = mirx::types::Point::new(
        mirx::types::Fixed::from_ratio(1, 2),
        mirx::types::Fixed::from_ratio(1, 2),
    );
    let gradient = radial(
        center,
        mirx::types::Fixed::from_ratio(1, 10),
        mirx::types::Transform::IDENTITY,
    );
    let sampler =
        RadialPaint::new(&gradient, Transform::IDENTITY, Rect::new(0, 0, 100, 50)).unwrap();
    assert_eq!(sampler.sample(59, 25).r, 0);
    assert!((125..=130).contains(&sampler.sample(80, 25).r));
}

#[test]
fn radial_non_nested_circles_leave_uncovered_pixels_transparent() {
    let mut gradient = radial(
        mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
        mirx::types::Fixed::ZERO,
        mirx::types::Transform::IDENTITY,
    );
    let sampler =
        RadialPaint::new(&gradient, Transform::IDENTITY, Rect::new(0, 0, 100, 50)).unwrap();
    assert_eq!(sampler.sample(0, 0).r, 0);
    assert_eq!(sampler.sample(-100, 50).a, 0);
    gradient.focal = gradient.center;
    gradient.focal_radius = gradient.radius;
    assert!(RadialPaint::new(&gradient, Transform::IDENTITY, Rect::new(0, 0, 100, 50)).is_none());
}

#[test]
fn spread_handles_large_negative_values_without_looping() {
    let t = Fixed64::from_int(-4096) - Fixed64::from_ratio(1, 4);
    assert_eq!(spread(t, SpreadMode::Repeat), Fixed64::from_ratio(3, 4));
    assert_eq!(spread(t, SpreadMode::Reflect), Fixed64::from_ratio(1, 4));
    assert_eq!(
        spread(Fixed64::from_fixed(Fixed::MIN), SpreadMode::Repeat),
        Fixed64::ZERO
    );
}
