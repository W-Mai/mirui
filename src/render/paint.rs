use mirx::scene::{GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode};

use crate::types::{Color, Fixed, Fixed64, Point, Rect, Transform, Transform3D};

#[derive(Clone, Copy)]
struct Affine {
    m00: Fixed64,
    m01: Fixed64,
    tx: Fixed64,
    m10: Fixed64,
    m11: Fixed64,
    ty: Fixed64,
}

impl Affine {
    fn from_draw(transform: Transform) -> Self {
        Self {
            m00: transform.m00.into(),
            m01: transform.m01.into(),
            tx: transform.tx.into(),
            m10: transform.m10.into(),
            m11: transform.m11.into(),
            ty: transform.ty.into(),
        }
    }

    fn from_paint(transform: mirx::types::Transform) -> Self {
        Self {
            m00: Fixed::from(transform.m00).into(),
            m01: Fixed::from(transform.m01).into(),
            tx: Fixed::from(transform.tx).into(),
            m10: Fixed::from(transform.m10).into(),
            m11: Fixed::from(transform.m11).into(),
            ty: Fixed::from(transform.ty).into(),
        }
    }

    fn from_bbox(bbox: Rect, units: GradientUnits) -> Option<Self> {
        match units {
            GradientUnits::UserSpaceOnUse => Some(Self::from_draw(Transform::IDENTITY)),
            GradientUnits::ObjectBoundingBox => {
                if bbox.w <= Fixed::ZERO || bbox.h <= Fixed::ZERO {
                    return None;
                }
                Some(Self {
                    m00: bbox.w.into(),
                    m01: Fixed64::ZERO,
                    tx: bbox.x.into(),
                    m10: Fixed64::ZERO,
                    m11: bbox.h.into(),
                    ty: bbox.y.into(),
                })
            }
        }
    }

    fn compose(self, other: Self) -> Self {
        Self {
            m00: self.m00.mul_wide(other.m00) + self.m01.mul_wide(other.m10),
            m01: self.m00.mul_wide(other.m01) + self.m01.mul_wide(other.m11),
            tx: self.m00.mul_wide(other.tx) + self.m01.mul_wide(other.ty) + self.tx,
            m10: self.m10.mul_wide(other.m00) + self.m11.mul_wide(other.m10),
            m11: self.m10.mul_wide(other.m01) + self.m11.mul_wide(other.m11),
            ty: self.m10.mul_wide(other.tx) + self.m11.mul_wide(other.ty) + self.ty,
        }
    }

    fn inverse(self) -> Option<Self> {
        let det = self.m00.mul_wide(self.m11) - self.m01.mul_wide(self.m10);
        if det == Fixed64::ZERO {
            return None;
        }
        Some(Self {
            m00: self.m11.div_wide(det),
            m01: (-self.m01).div_wide(det),
            tx: (self.m01.mul_wide(self.ty) - self.m11.mul_wide(self.tx)).div_wide(det),
            m10: (-self.m10).div_wide(det),
            m11: self.m00.div_wide(det),
            ty: (self.m10.mul_wide(self.tx) - self.m00.mul_wide(self.ty)).div_wide(det),
        })
    }

    fn for_paint(
        draw: Transform,
        bbox: Rect,
        units: GradientUnits,
        paint: mirx::types::Transform,
    ) -> Option<Self> {
        Self::from_draw(draw)
            .compose(Self::from_bbox(bbox, units)?)
            .compose(Self::from_paint(paint))
            .inverse()
    }
}

pub(super) enum GradientPaint<'a> {
    Linear {
        paint: LinearPaint<'a>,
        row: Option<(i32, Color)>,
    },
    AxisRadial(AxisRadialPaint<'a>),
    Radial(RadialPaint<'a>),
}

impl<'a> GradientPaint<'a> {
    pub(super) fn new(paint: &'a Paint, draw: Transform, bbox: Rect) -> Option<Self> {
        match paint {
            Paint::Color(_) => None,
            Paint::LinearGradient(gradient) => Some(Self::Linear {
                paint: LinearPaint::new(gradient, draw, bbox)?,
                row: None,
            }),
            Paint::RadialGradient(gradient) => {
                let paint = RadialPaint::new(gradient, draw, bbox)?;
                Some(
                    if paint.is_centered()
                        && paint.inverse.m01 == Fixed64::ZERO
                        && paint.inverse.m10 == Fixed64::ZERO
                    {
                        Self::AxisRadial(AxisRadialPaint::new(paint))
                    } else {
                        Self::Radial(paint)
                    },
                )
            }
        }
    }

    pub(super) fn sample(&mut self, px: i32, py: i32) -> Color {
        match self {
            Self::Linear { paint, row } => {
                if paint.x == Fixed64::ZERO
                    && let Some((previous_y, color)) = *row
                    && previous_y == py
                {
                    return color;
                }
                let color = paint.sample_point(Point::new(px, py));
                if paint.x == Fixed64::ZERO {
                    *row = Some((py, color));
                }
                color
            }
            Self::AxisRadial(paint) => paint.sample(px, py),
            Self::Radial(paint) => paint.sample_point(Point::new(px, py)),
        }
    }

    fn sample_point(&self, point: Point) -> Color {
        match self {
            Self::Linear { paint, .. } => paint.sample_point(point),
            Self::AxisRadial(paint) => paint.sample_point(point),
            Self::Radial(paint) => paint.sample_point(point),
        }
    }
}

pub(super) struct AxisRadialPaint<'a> {
    x: Fixed64,
    y: Fixed64,
    tx: Fixed64,
    ty: Fixed64,
    fx: Fixed64,
    fy: Fixed64,
    quadratic: Fixed64,
    spread: SpreadMode,
    stops: &'a [GradientStop],
    row: Option<(i32, Fixed64)>,
}

const _: () = assert!(
    core::mem::size_of::<AxisRadialPaint<'static>>()
        <= core::mem::size_of::<RadialPaint<'static>>()
);

impl<'a> AxisRadialPaint<'a> {
    fn new(paint: RadialPaint<'a>) -> Self {
        Self {
            x: paint.inverse.m00,
            y: paint.inverse.m11,
            tx: paint.inverse.tx,
            ty: paint.inverse.ty,
            fx: paint.fx,
            fy: paint.fy,
            quadratic: paint.quadratic,
            spread: paint.spread,
            stops: paint.stops,
            row: None,
        }
    }

    fn sample(&mut self, px: i32, py: i32) -> Color {
        let y_squared = match self.row {
            Some((previous_y, squared)) if previous_y == py => squared,
            _ => {
                let y =
                    self.y.mul_wide(Fixed64::from_fixed(Fixed::from_int(py))) + self.ty - self.fy;
                let squared = y.mul_wide(y);
                self.row = Some((py, squared));
                squared
            }
        };
        self.sample_with_y_squared(Fixed::from_int(px), y_squared)
    }

    fn sample_point(&self, point: Point) -> Color {
        let y = self.y.mul_wide(Fixed64::from_fixed(point.y)) + self.ty - self.fy;
        self.sample_with_y_squared(point.x, y.mul_wide(y))
    }

    fn sample_with_y_squared(&self, px: Fixed, y_squared: Fixed64) -> Color {
        let x = self.x.mul_wide(Fixed64::from_fixed(px)) + self.tx - self.fx;
        let distance = x.mul_wide(x) + y_squared;
        centered_radial_color(
            self.quadratic,
            -self.quadratic.mul_wide(distance),
            self.spread,
            self.stops,
        )
    }
}

pub(super) struct ProjectiveGradientPaint<'a> {
    inverse: Transform3D,
    paint: GradientPaint<'a>,
}

impl<'a> ProjectiveGradientPaint<'a> {
    pub(super) fn new(paint: &'a Paint, physical: Transform3D, bbox: Rect) -> Option<Self> {
        Some(Self {
            inverse: physical.inverse()?,
            paint: GradientPaint::new(paint, Transform::IDENTITY, bbox)?,
        })
    }

    pub(super) fn sample(&self, px: i32, py: i32) -> Color {
        self.sample_point(Point::new(px, py))
    }

    fn sample_point(&self, point: Point) -> Color {
        let Some(point) = self.inverse.apply_point(point) else {
            return Color::rgba(0, 0, 0, 0);
        };
        self.paint.sample_point(point)
    }
}

pub(super) struct LinearPaint<'a> {
    x: Fixed64,
    y: Fixed64,
    bias: Fixed64,
    spread: SpreadMode,
    stops: &'a [GradientStop],
}

impl<'a> LinearPaint<'a> {
    #[cfg(feature = "sdl-gpu")]
    pub(super) fn plane(&self) -> [f64; 3] {
        [
            self.x.to_f32() as f64,
            self.y.to_f32() as f64,
            self.bias.to_f32() as f64,
        ]
    }

    pub(super) fn new(gradient: &'a LinearGradient, draw: Transform, bbox: Rect) -> Option<Self> {
        if !GradientStop::sequence_is_valid(&gradient.stops) {
            return None;
        }
        let inverse = Affine::for_paint(draw, bbox, gradient.units, gradient.transform)?;
        let sx: Fixed64 = Fixed::from(gradient.start.x).into();
        let sy: Fixed64 = Fixed::from(gradient.start.y).into();
        let dx = Fixed64::from_fixed(Fixed::from(gradient.end.x)) - sx;
        let dy = Fixed64::from_fixed(Fixed::from(gradient.end.y)) - sy;
        let length = dx.mul_wide(dx) + dy.mul_wide(dy);
        if length == Fixed64::ZERO {
            return None;
        }
        let axis_x = dx.div_wide(length);
        let axis_y = dy.div_wide(length);
        let x = axis_x.mul_wide(inverse.m00) + axis_y.mul_wide(inverse.m10);
        let y = axis_x.mul_wide(inverse.m01) + axis_y.mul_wide(inverse.m11);
        let bias = axis_x.mul_wide(inverse.tx - sx) + axis_y.mul_wide(inverse.ty - sy);
        Some(Self {
            x,
            y,
            bias,
            spread: gradient.spread,
            stops: &gradient.stops,
        })
    }

    #[cfg(test)]
    fn sample(&self, px: i32, py: i32) -> Color {
        self.sample_point(Point::new(px, py))
    }

    fn sample_point(&self, point: Point) -> Color {
        let t = self.x.mul_wide(Fixed64::from_fixed(point.x))
            + self.y.mul_wide(Fixed64::from_fixed(point.y))
            + self.bias;
        sample_stops(self.stops, spread(t, self.spread))
    }
}

pub(super) struct RadialPaint<'a> {
    inverse: Affine,
    fx: Fixed64,
    fy: Fixed64,
    dx: Fixed64,
    dy: Fixed64,
    inner_radius: Fixed64,
    radius_delta: Fixed64,
    quadratic: Fixed64,
    spread: SpreadMode,
    stops: &'a [GradientStop],
}

impl<'a> RadialPaint<'a> {
    pub(super) fn new(gradient: &'a RadialGradient, draw: Transform, bbox: Rect) -> Option<Self> {
        if !GradientStop::sequence_is_valid(&gradient.stops) {
            return None;
        }
        let inverse = Affine::for_paint(draw, bbox, gradient.units, gradient.transform)?;
        let fx: Fixed64 = Fixed::from(gradient.focal.x).into();
        let fy: Fixed64 = Fixed::from(gradient.focal.y).into();
        let cx: Fixed64 = Fixed::from(gradient.center.x).into();
        let cy: Fixed64 = Fixed::from(gradient.center.y).into();
        let outer_radius: Fixed64 = Fixed::from(gradient.radius).into();
        let inner_radius: Fixed64 = Fixed::from(gradient.focal_radius).into();
        if inner_radius < Fixed64::ZERO || outer_radius < Fixed64::ZERO {
            return None;
        }
        let dx = cx - fx;
        let dy = cy - fy;
        let radius_delta = outer_radius - inner_radius;
        if dx == Fixed64::ZERO && dy == Fixed64::ZERO && radius_delta == Fixed64::ZERO {
            return None;
        }
        let quadratic = dx.mul_wide(dx) + dy.mul_wide(dy) - radius_delta.mul_wide(radius_delta);
        Some(Self {
            inverse,
            fx,
            fy,
            dx,
            dy,
            inner_radius,
            radius_delta,
            quadratic,
            spread: gradient.spread,
            stops: &gradient.stops,
        })
    }

    #[cfg(feature = "sdl-gpu")]
    pub(super) fn centered_space(&self) -> Option<[f64; 6]> {
        if !self.is_centered() {
            return None;
        }
        let radius = self.radius_delta.to_f32() as f64;
        let scale = 0.5 / radius;
        Some([
            self.inverse.m00.to_f32() as f64 * scale,
            self.inverse.m01.to_f32() as f64 * scale,
            (self.inverse.tx - self.fx).to_f32() as f64 * scale + 0.5,
            self.inverse.m10.to_f32() as f64 * scale,
            self.inverse.m11.to_f32() as f64 * scale,
            (self.inverse.ty - self.fy).to_f32() as f64 * scale + 0.5,
        ])
    }

    #[cfg(feature = "sdl-gpu")]
    pub(super) fn sample_centered(&self, x: f32, y: f32) -> Color {
        let radius = self.radius_delta.to_f32();
        self.sample_relative(Fixed64::from_f32(x * radius), Fixed64::from_f32(y * radius))
    }

    #[cfg(test)]
    fn sample(&self, px: i32, py: i32) -> Color {
        self.sample_point(Point::new(px, py))
    }

    fn sample_point(&self, point: Point) -> Color {
        let x = Fixed64::from_fixed(point.x);
        let y = Fixed64::from_fixed(point.y);
        let qx =
            self.inverse.m00.mul_wide(x) + self.inverse.m01.mul_wide(y) + self.inverse.tx - self.fx;
        let qy =
            self.inverse.m10.mul_wide(x) + self.inverse.m11.mul_wide(y) + self.inverse.ty - self.fy;
        self.sample_relative(qx, qy)
    }

    fn sample_relative(&self, qx: Fixed64, qy: Fixed64) -> Color {
        if let Some(discriminant) = self.centered_discriminant(qx, qy) {
            return centered_radial_color(self.quadratic, discriminant, self.spread, self.stops);
        }
        let Some(t) = self.conical_parameter(qx, qy) else {
            return Color::rgba(0, 0, 0, 0);
        };
        sample_stops(self.stops, spread(t, self.spread))
    }

    fn centered_discriminant(&self, qx: Fixed64, qy: Fixed64) -> Option<Fixed64> {
        if self.is_centered() {
            // Keep the conical solver's rounding; the second root has negative radius.
            let distance = qx.mul_wide(qx) + qy.mul_wide(qy);
            return Some(-self.quadratic.mul_wide(distance));
        }
        None
    }

    fn is_centered(&self) -> bool {
        self.dx == Fixed64::ZERO
            && self.dy == Fixed64::ZERO
            && self.inner_radius == Fixed64::ZERO
            && self.quadratic < Fixed64::ZERO
    }

    fn conical_parameter(&self, qx: Fixed64, qy: Fixed64) -> Option<Fixed64> {
        let linear = qx.mul_wide(self.dx)
            + qy.mul_wide(self.dy)
            + self.inner_radius.mul_wide(self.radius_delta);
        let distance =
            qx.mul_wide(qx) + qy.mul_wide(qy) - self.inner_radius.mul_wide(self.inner_radius);
        let t = if self.quadratic == Fixed64::ZERO {
            if linear == Fixed64::ZERO {
                if distance == Fixed64::ZERO {
                    Some(Fixed64::ZERO)
                } else {
                    None
                }
            } else {
                Some(distance.div_wide(linear) / 2)
            }
        } else {
            let discriminant = linear.mul_wide(linear) - self.quadratic.mul_wide(distance);
            if discriminant < Fixed64::ZERO {
                None
            } else {
                let root = discriminant.sqrt();
                let first = (linear - root).div_wide(self.quadratic);
                let second = (linear + root).div_wide(self.quadratic);
                [first, second]
                    .into_iter()
                    .filter(|candidate| {
                        self.inner_radius + self.radius_delta.mul_wide(*candidate) >= Fixed64::ZERO
                    })
                    .max()
            }
        };
        t.filter(|candidate| {
            self.inner_radius + self.radius_delta.mul_wide(*candidate) >= Fixed64::ZERO
        })
    }
}

fn centered_radial_parameter(quadratic: Fixed64, discriminant: Fixed64) -> Option<Fixed64> {
    (discriminant >= Fixed64::ZERO).then(|| (-discriminant.sqrt()).div_wide(quadratic))
}

fn centered_radial_color(
    quadratic: Fixed64,
    discriminant: Fixed64,
    mode: SpreadMode,
    stops: &[GradientStop],
) -> Color {
    // A Q48.16 value of magnitude <= 2^23 has a square that fits its storage.
    if mode == SpreadMode::Pad
        && quadratic >= -Fixed64::from_int(1 << 23)
        && discriminant > quadratic.mul_wide(quadratic)
    {
        // A discriminant above A² guarantees t >= 1 after integer truncation.
        return stops[stops.len() - 1].color.into();
    }
    let Some(t) = centered_radial_parameter(quadratic, discriminant) else {
        return Color::rgba(0, 0, 0, 0);
    };
    sample_stops(stops, spread(t, mode))
}

fn spread(t: Fixed64, mode: SpreadMode) -> Fixed64 {
    match mode {
        SpreadMode::Pad => t,
        SpreadMode::Repeat => t - Fixed64::from_int(t.to_int()),
        SpreadMode::Reflect => {
            let period = t.to_int();
            let fraction = t - Fixed64::from_int(period);
            if period.rem_euclid(2) == 0 {
                fraction
            } else {
                Fixed64::ONE - fraction
            }
        }
    }
}

pub(super) fn sample_stops(stops: &[GradientStop], t: Fixed64) -> Color {
    let Some(first) = stops.first() else {
        return Color::rgba(0, 0, 0, 0);
    };
    if t < Fixed64::from_fixed(first.offset.into()) {
        return first.color.into();
    }
    for pair in stops.windows(2) {
        let [start, end] = pair else { unreachable!() };
        let end_offset = Fixed64::from_fixed(end.offset.into());
        if t < end_offset {
            let start_offset = Fixed64::from_fixed(start.offset.into());
            let interval = end_offset - start_offset;
            if interval == Fixed64::ZERO {
                return end.color.into();
            }
            let amount = (t - start_offset).div_wide(interval).to_f32();
            let channel = |start: u8, end: u8| {
                (f32::from(start) + (f32::from(end) - f32::from(start)) * amount) as u8
            };
            return Color::rgba(
                channel(start.color.r, end.color.r),
                channel(start.color.g, end.color.g),
                channel(start.color.b, end.color.b),
                channel(start.color.a, end.color.a),
            );
        }
    }
    stops[stops.len() - 1].color.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::borrow::Cow;

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
                <= core::mem::size_of::<RadialPaint<'_>>()
                    + core::mem::align_of::<RadialPaint<'_>>()
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
                        let mut gradient =
                            radial(center, mirx::types::Fixed::ZERO, paint_transform);
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
            let paint =
                Paint::RadialGradient(radial(focal, inner, mirx::types::Transform::IDENTITY));
            assert!(matches!(
                GradientPaint::new(&paint, Transform::IDENTITY, Rect::new(0, 0, 40, 30)),
                Some(GradientPaint::Radial(_))
            ));
        }
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
        assert!(
            RadialPaint::new(&gradient, Transform::IDENTITY, Rect::new(0, 0, 100, 50)).is_none()
        );
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
}
