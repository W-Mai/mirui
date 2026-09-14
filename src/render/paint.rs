use mirx::scene::{GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode};

use crate::types::{Color, Fixed, Fixed64, Rect, Transform};

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
    Linear(LinearPaint<'a>),
    Radial(RadialPaint<'a>),
}

impl<'a> GradientPaint<'a> {
    pub(super) fn new(paint: &'a Paint, draw: Transform, bbox: Rect) -> Option<Self> {
        match paint {
            Paint::Color(_) => None,
            Paint::LinearGradient(gradient) => {
                Some(Self::Linear(LinearPaint::new(gradient, draw, bbox)?))
            }
            Paint::RadialGradient(gradient) => {
                Some(Self::Radial(RadialPaint::new(gradient, draw, bbox)?))
            }
        }
    }

    pub(super) fn sample(&self, px: i32, py: i32) -> Color {
        match self {
            Self::Linear(paint) => paint.sample(px, py),
            Self::Radial(paint) => paint.sample(px, py),
        }
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
    pub(super) fn new(gradient: &'a LinearGradient, draw: Transform, bbox: Rect) -> Option<Self> {
        if !valid_stops(&gradient.stops) {
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

    pub(super) fn sample(&self, px: i32, py: i32) -> Color {
        let t = self.x.mul_wide(Fixed64::from_int(i64::from(px)))
            + self.y.mul_wide(Fixed64::from_int(i64::from(py)))
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
    fn new(gradient: &'a RadialGradient, draw: Transform, bbox: Rect) -> Option<Self> {
        if !valid_stops(&gradient.stops) {
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

    fn sample(&self, px: i32, py: i32) -> Color {
        let x = Fixed64::from_int(i64::from(px));
        let y = Fixed64::from_int(i64::from(py));
        let qx =
            self.inverse.m00.mul_wide(x) + self.inverse.m01.mul_wide(y) + self.inverse.tx - self.fx;
        let qy =
            self.inverse.m10.mul_wide(x) + self.inverse.m11.mul_wide(y) + self.inverse.ty - self.fy;
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
        let Some(t) = t.filter(|candidate| {
            self.inner_radius + self.radius_delta.mul_wide(*candidate) >= Fixed64::ZERO
        }) else {
            return Color::rgba(0, 0, 0, 0);
        };
        sample_stops(self.stops, spread(t, self.spread))
    }
}

pub(crate) fn valid_stops(stops: &[GradientStop]) -> bool {
    let mut previous = mirx::types::Fixed::ZERO;
    if stops.is_empty() {
        return false;
    }
    for stop in stops {
        if stop.offset < previous || stop.offset > mirx::types::Fixed::ONE {
            return false;
        }
        previous = stop.offset;
    }
    true
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

fn sample_stops(stops: &[GradientStop], t: Fixed64) -> Color {
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
        assert!(valid_stops(&stops));
        assert_eq!(sample_stops(&stops, Fixed64::from_ratio(1, 2)).b, 255);
        assert!(!valid_stops(&[]));
        assert!(!valid_stops(&[stops[2], stops[1], stops[0]]));
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
