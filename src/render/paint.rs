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

    pub(super) fn has_constant_rows(&self) -> bool {
        matches!(self, Self::Linear { paint, .. } if paint.x == Fixed64::ZERO)
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
    pad_endpoint: Option<Fixed64>,
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
            pad_endpoint: (paint.spread == SpreadMode::Pad
                && paint.quadratic >= -Fixed64::from_int(1 << 23))
            .then(|| paint.quadratic.mul_wide(paint.quadratic)),
            spread: paint.spread,
            stops: paint.stops,
            row: None,
        }
    }

    fn sample(&mut self, px: i32, py: i32) -> Color {
        let y_squared = match self.row {
            Some((previous_y, squared)) if previous_y == py => squared,
            _ => {
                let y = self.y.wrapping_mul_int(Fixed::from_int(py).to_int()) + self.ty - self.fy;
                let squared = y.mul_wide(y);
                self.row = Some((py, squared));
                squared
            }
        };
        let x = self.x.wrapping_mul_int(Fixed::from_int(px).to_int()) + self.tx - self.fx;
        self.sample_with_distance(x, y_squared)
    }

    fn sample_point(&self, point: Point) -> Color {
        let y = self.y.mul_wide(Fixed64::from_fixed(point.y)) + self.ty - self.fy;
        self.sample_with_y_squared(point.x, y.mul_wide(y))
    }

    fn sample_with_y_squared(&self, px: Fixed, y_squared: Fixed64) -> Color {
        let x = self.x.mul_wide(Fixed64::from_fixed(px)) + self.tx - self.fx;
        self.sample_with_distance(x, y_squared)
    }

    fn sample_with_distance(&self, x: Fixed64, y_squared: Fixed64) -> Color {
        let distance = x.mul_wide(x) + y_squared;
        let discriminant = -self.quadratic.mul_wide(distance);
        if self
            .pad_endpoint
            .is_some_and(|endpoint| discriminant > endpoint)
        {
            return self.stops[self.stops.len() - 1].color.into();
        }
        centered_radial_interior_color(self.quadratic, discriminant, self.spread, self.stops)
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
    // A Q48.16 square root uses at most 40 raw bits; shifting it by 16 fits i64.
    (discriminant >= Fixed64::ZERO).then(|| -discriminant.sqrt() / quadratic)
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
    centered_radial_interior_color(quadratic, discriminant, mode, stops)
}

fn centered_radial_interior_color(
    quadratic: Fixed64,
    discriminant: Fixed64,
    mode: SpreadMode,
    stops: &[GradientStop],
) -> Color {
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

fn stop_offset(offset: mirx::types::Fixed) -> Fixed64 {
    // Normalized Q24.8 offsets are exactly representable in f32.
    Fixed64::from_fixed(Fixed::from_f32(offset.to_f32()))
}

fn interpolate_stop_color(
    start: mirx::types::Color,
    end: mirx::types::Color,
    amount: Fixed64,
) -> Color {
    // The normalized fraction and byte channels keep all f32 intermediates exact.
    let channel = |start: u8, end: u8| {
        (Fixed64::from_int(i64::from(start)) + amount * (i64::from(end) - i64::from(start)))
            .to_int() as u8
    };
    Color::rgba(
        channel(start.r, end.r),
        channel(start.g, end.g),
        channel(start.b, end.b),
        channel(start.a, end.a),
    )
}

pub(super) fn sample_stops(stops: &[GradientStop], t: Fixed64) -> Color {
    let Some(first) = stops.first() else {
        return Color::rgba(0, 0, 0, 0);
    };
    let mut start_offset = stop_offset(first.offset);
    if t < start_offset {
        return first.color.into();
    }
    for pair in stops.windows(2) {
        let [start, end] = pair else { unreachable!() };
        let end_offset = stop_offset(end.offset);
        if t < end_offset {
            let interval = end_offset - start_offset;
            if interval == Fixed64::ZERO {
                return end.color.into();
            }
            return interpolate_stop_color(
                start.color,
                end.color,
                (t - start_offset).div_unit_interval(interval),
            );
        }
        start_offset = end_offset;
    }
    stops[stops.len() - 1].color.into()
}

#[cfg(test)]
#[path = "../../tests/unit/render/paint.rs"]
mod tests;
