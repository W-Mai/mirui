use alloc::borrow::Cow;

use super::state::IrisState;
use super::style::{SEE, is_dark};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::render::scene::{
    GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode,
};
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::view::PaintCtx;

/// Lens geometry is authored on a radius of 64 units. Draw scales stay near or
/// above 1, where 24.8 transform entries keep their precision.
pub(super) const UNIT: i32 = 64;
pub(super) const BLADES: i32 = 9;
/// Blade pivot pins, offset against the closing direction, in lens units.
const PIVOT_X: Fixed = Fixed::from_ratio(-2688, 100);
const PIVOT_Y: Fixed = Fixed::from_ratio(-512, 10);
/// Distance of the blade's inner edge from the axis before it swings in.
const EDGE_REST: Fixed = Fixed::from_ratio(4864, 100);
/// Glass radius as a fraction of the lens radius, in thousandths.
const GLASS_PER_MILLE: i32 = 740;
const KNURL_TEETH: i32 = 120;

pub(super) static CIRCLE: Path = path!(
    "M 64 0 C 64 35.346 35.346 64 0 64 C -35.346 64 -64 35.346 -64 0 C -64 -35.346 -35.346 -64 0 -64 C 35.346 -64 64 -35.346 64 0 Z"
);
static GLASS_DISC: Path = path!(
    "M 47.36 0 C 47.36 26.156 26.156 47.36 0 47.36 C -26.156 47.36 -47.36 26.156 -47.36 0 C -47.36 -26.156 -26.156 -47.36 0 -47.36 C 26.156 -47.36 47.36 -26.156 47.36 0 Z"
);
static SCALE_RING: Path = path!(
    "M 55.68 0 C 55.68 30.751 30.751 55.68 0 55.68 C -30.751 55.68 -55.68 30.751 -55.68 0 C -55.68 -30.751 -30.751 -55.68 0 -55.68 C 30.751 -55.68 55.68 -30.751 55.68 0 Z M 47.36 0 C 47.36 26.156 26.156 47.36 0 47.36 C -26.156 47.36 -47.36 26.156 -47.36 0 C -47.36 -26.156 -26.156 -47.36 0 -47.36 C 26.156 -47.36 47.36 -26.156 47.36 0 Z"
);
static BLADE: Path = path!(
    "M -12.8 -48.64 L 83.2 -48.64 L 83.2 -121.6 L -44.8 -121.6 C -44.8 -83.2 -26.88 -62.72 -12.8 -48.64 Z"
);
/// The leading strip of the first blade, painted again on top so the overlap
/// stays cyclic: every blade covers the one before it.
static BLADE_LEAD: Path = path!(
    "M -12.8 -48.64 L 16.64 -48.64 L 0.64 -121.6 L -44.8 -121.6 C -44.8 -83.2 -26.88 -62.72 -12.8 -48.64 Z"
);
static BLADE_EDGE: Path = path!(
    "M -12.8 -48.64 C -26.88 -62.72 -44.8 -83.2 -44.8 -121.6 L -43.776 -121.6 C -43.776 -82.944 -25.856 -62.208 -12.032 -49.408 Z"
);
static INDEX: Path = path!("M 0 -53.632 L -1.664 -56.576 L 1.664 -56.576 Z");
/// Catchlight on the front glass, the white dot of the mark.
static GLINT: Path = path!(
    "M 5.44 0 C 5.44 2.545 3.004 4.608 0 4.608 C -3.004 4.608 -5.44 2.545 -5.44 0 C -5.44 -2.545 -3.004 -4.608 0 -4.608 C 3.004 -4.608 5.44 -2.545 5.44 0 Z"
);

fn stop(offset: Fixed, color: Color) -> GradientStop {
    GradientStop {
        offset: offset.into(),
        color: color.into(),
    }
}

fn linear(start: (i32, i32), end: (i32, i32), stops: [(i32, Color); 3]) -> Paint {
    Paint::LinearGradient(LinearGradient {
        start: Point::new(
            Fixed::from_ratio(start.0, 100),
            Fixed::from_ratio(start.1, 100),
        )
        .into(),
        end: Point::new(Fixed::from_ratio(end.0, 100), Fixed::from_ratio(end.1, 100)).into(),
        stops: Cow::Owned(
            stops
                .into_iter()
                .map(|(offset, color)| stop(Fixed::from_ratio(offset, 100), color))
                .collect(),
        ),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    })
}

fn radial(center: (i32, i32), radius: i32, focal: (i32, i32), stops: &[(i32, Color)]) -> Paint {
    Paint::RadialGradient(RadialGradient {
        center: Point::new(
            Fixed::from_ratio(center.0, 100),
            Fixed::from_ratio(center.1, 100),
        )
        .into(),
        radius: Fixed::from_ratio(radius, 100).into(),
        focal: Point::new(
            Fixed::from_ratio(focal.0, 100),
            Fixed::from_ratio(focal.1, 100),
        )
        .into(),
        focal_radius: Fixed::ZERO.into(),
        stops: Cow::Owned(
            stops
                .iter()
                .map(|(offset, color)| stop(Fixed::from_ratio(*offset, 100), *color))
                .collect(),
        ),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    })
}

struct Finish {
    barrel: Paint,
    blade: Paint,
    knurl: Color,
    knurl_opa: u8,
    rim: Color,
    rim_opa: u8,
    ring: Color,
    ring_edge: Color,
}

impl Finish {
    fn black() -> Self {
        Self {
            barrel: linear(
                (100, 0),
                (0, 100),
                [
                    (0, Color::rgb(74, 81, 92)),
                    (50, Color::rgb(29, 33, 41)),
                    (100, Color::rgb(10, 12, 16)),
                ],
            ),
            blade: linear(
                (0, 0),
                (100, 0),
                [
                    (0, Color::rgb(62, 68, 78)),
                    (35, Color::rgb(34, 38, 45)),
                    (100, Color::rgb(13, 15, 18)),
                ],
            ),
            knurl: Color::rgb(5, 6, 8),
            knurl_opa: 230,
            rim: Color::rgb(255, 255, 255),
            rim_opa: 46,
            ring: Color::rgb(11, 13, 17),
            ring_edge: Color::rgb(43, 49, 58),
        }
    }

    fn silver() -> Self {
        Self {
            barrel: linear(
                (100, 0),
                (0, 100),
                [
                    (0, Color::rgb(251, 252, 253)),
                    (50, Color::rgb(211, 217, 224)),
                    (100, Color::rgb(163, 173, 185)),
                ],
            ),
            blade: linear(
                (0, 0),
                (100, 0),
                [
                    (0, Color::rgb(75, 82, 92)),
                    (35, Color::rgb(43, 48, 56)),
                    (100, Color::rgb(21, 24, 28)),
                ],
            ),
            knurl: Color::rgb(127, 137, 150),
            knurl_opa: 150,
            rim: Color::rgb(255, 255, 255),
            rim_opa: 230,
            ring: Color::rgb(233, 237, 241),
            ring_edge: Color::rgb(195, 202, 211),
        }
    }
}

#[crate::component]
pub(super) struct IrisLens {
    pub(super) model: model!(IrisState),
    knurl: Path,
    coatings: [Path; 2],
    finishes: [Finish; 2],
    pupil: Paint,
    sheen: Paint,
    coat: [Paint; 2],
    glint: Paint,
}

/// A point at `per_mille` thousandths of the lens radius, in lens units.
fn polar(per_mille: i32, degrees: Fixed) -> Point {
    let radius = Fixed::from_ratio(per_mille * UNIT, 1000);
    Point::new(
        Fixed::cos_deg(degrees) * radius,
        Fixed::sin_deg(degrees) * radius,
    )
}

fn knurl_path() -> Path {
    let mut path = Path::try_with_capacity(KNURL_TEETH as usize * 5).expect("knurl storage");
    let pitch = Fixed::from_ratio(360, KNURL_TEETH);
    let half = pitch / 4;
    for tooth in 0..KNURL_TEETH {
        let angle = pitch * tooth;
        path.move_to(polar(905, angle - half))
            .line_to(polar(968, angle - half))
            .line_to(polar(968, angle + half))
            .line_to(polar(905, angle + half))
            .close();
    }
    path
}

/// A reflection crescent: an arc of the glass rim closed by a flatter chord.
fn crescent(radius: i32, inner: i32, from: i32, to: i32) -> Path {
    let mut path = Path::try_with_capacity(4).expect("crescent storage");
    let a0 = Fixed::from_int(from);
    let a1 = Fixed::from_int(to);
    let mid = (a0 + a1) / 2;
    // Cubic control distance for a circular arc: 4/3 * tan(span / 4) * r.
    let quarter = (a1 - a0) / 4;
    let reach = Fixed::from_ratio(4, 3) * Fixed::sin_deg(quarter) / Fixed::cos_deg(quarter)
        * Fixed::from_ratio(radius * UNIT, 1000);
    let p0 = polar(radius, a0);
    let p1 = polar(radius, a1);
    let t0 = Point::new(Fixed::ZERO - Fixed::sin_deg(a0), Fixed::cos_deg(a0));
    let t1 = Point::new(Fixed::ZERO - Fixed::sin_deg(a1), Fixed::cos_deg(a1));
    path.move_to(p0)
        .cubic_to(
            Point::new(p0.x + t0.x * reach, p0.y + t0.y * reach),
            Point::new(p1.x - t1.x * reach, p1.y - t1.y * reach),
            p1,
        )
        .quad_to(polar(inner, mid), p0)
        .close();
    path
}

impl IrisLens {
    pub(super) fn new(model: <IrisState as crate::core::model::BindType>::Shared) -> Self {
        let clear = |r: u8, g: u8, b: u8| Color::rgba(r, g, b, 0);
        Self {
            model,
            knurl: knurl_path(),
            coatings: [crescent(690, 600, 196, 252), crescent(688, 622, 22, 64)],
            finishes: [Finish::black(), Finish::silver()],
            pupil: radial(
                (50, 50),
                50,
                (58, 40),
                &[
                    (0, Color::rgb(255, 122, 92)),
                    (30, Color::rgb(239, 74, 53)),
                    (70, Color::rgb(122, 18, 24)),
                    (100, Color::rgb(26, 4, 7)),
                ],
            ),
            sheen: radial(
                (64, 28),
                80,
                (64, 28),
                &[
                    (0, Color::rgba(255, 255, 255, 34)),
                    (55, Color::rgba(159, 195, 255, 6)),
                    (100, Color::rgba(0, 0, 0, 120)),
                ],
            ),
            coat: [
                linear(
                    (0, 100),
                    (100, 0),
                    [
                        (0, clear(180, 76, 255)),
                        (55, Color::rgba(155, 92, 255, 150)),
                        (100, clear(79, 155, 255)),
                    ],
                ),
                linear(
                    (100, 0),
                    (0, 100),
                    [
                        (0, clear(61, 255, 176)),
                        (50, Color::rgba(61, 255, 192, 92)),
                        (100, clear(61, 255, 176)),
                    ],
                ),
            ],
            glint: radial(
                (50, 50),
                50,
                (50, 50),
                &[
                    (0, Color::rgba(255, 255, 255, 255)),
                    (45, Color::rgba(255, 255, 255, 190)),
                    (100, Color::rgba(255, 255, 255, 0)),
                ],
            ),
        }
    }
}

/// Swing of one blade about its pivot that leaves its inner edge `edge`
/// units from the axis. The edge distance falls monotonically with the swing.
pub(super) fn blade_reach(swing: Fixed) -> Fixed {
    EDGE_REST + Fixed::sin_deg(swing) * PIVOT_X
        - (Fixed::ONE - Fixed::cos_deg(swing)) * (Fixed::ZERO - PIVOT_Y)
}

pub(super) fn blade_swing(edge: Fixed) -> Fixed {
    let mut low = Fixed::ZERO;
    let mut high = Fixed::from_int(80);
    for _ in 0..18 {
        let mid = (low + high) / 2;
        if blade_reach(mid) > edge {
            low = mid;
        } else {
            high = mid;
        }
    }
    (low + high) / 2
}

pub(super) fn blade_transform(index: i32, swing: Fixed) -> Transform {
    let pivot = Transform::translate(PIVOT_X, PIVOT_Y);
    let back = Transform::translate(Fixed::ZERO - PIVOT_X, Fixed::ZERO - PIVOT_Y);
    Transform::rotate_deg(Fixed::from_ratio(360 * index, BLADES))
        .compose(&pivot)
        .compose(&Transform::rotate_deg(swing))
        .compose(&back)
}

struct Lens<'paint, 'frame> {
    paint: &'paint mut PaintCtx<'frame>,
    unit: Transform,
    center: Point,
    radius: Fixed,
}

impl Lens<'_, '_> {
    fn fill(&mut self, path: &Path, local: Transform, paint: &Paint, opa: u8) {
        self.paint.draw(&DrawCommand::FillPath {
            path,
            transform: self.unit.compose(&local),
            paint,
            opa,
            fill_rule: FillRule::EvenOdd,
        });
    }

    fn ring(&mut self, per_mille: i32, color: Color, width: Fixed, opa: u8) {
        self.paint.draw(&DrawCommand::Arc {
            center: self.center,
            transform: self.paint.transform(),
            radius: self.radius * Fixed::from_ratio(per_mille, 1000),
            start_angle: Fixed::ZERO,
            end_angle: Fixed::from_int(360),
            color,
            width,
            opa,
        });
    }
}

impl IrisLens {
    fn render(&self, paint: &mut PaintCtx<'_>, state: &IrisState, theme: &Theme) {
        let rect = paint.rect();
        let size = rect.w.min(rect.h);
        if size < Fixed::from_int(24) {
            return;
        }
        let radius = size / 2;
        let center = Point::new(rect.x + rect.w / 2, rect.y + rect.h / 2);
        let scale = radius / UNIT;
        let unit = paint
            .transform()
            .compose(&Transform::translate(center.x, center.y))
            .compose(&Transform::scale(scale, scale));
        let finish = &self.finishes[usize::from(!is_dark(theme))];
        let fine = Fixed::ONE.max(radius / 160);
        let mut lens = Lens {
            paint,
            unit,
            center,
            radius,
        };

        lens.fill(&CIRCLE, Transform::IDENTITY, &finish.barrel, 255);
        if radius >= Fixed::from_int(40) {
            let knurl = Paint::Color(finish.knurl.into());
            lens.fill(&self.knurl, Transform::IDENTITY, &knurl, finish.knurl_opa);
        }
        lens.ring(995, finish.rim, Fixed::ONE, finish.rim_opa);
        let ring = Paint::Color(finish.ring.into());
        lens.fill(&SCALE_RING, Transform::IDENTITY, &ring, 255);
        lens.ring(870, finish.ring_edge, Fixed::ONE, 255);

        let phase = state.phase();
        // The iris drifts a little, like an eye that keeps looking around.
        let glance = Point::new(
            Fixed::cos_deg(phase) * Fixed::from_ratio(166, 100),
            Fixed::sin_deg(phase * 2) * Fixed::from_ratio(115, 100) - Fixed::from_ratio(64, 100),
        );
        // Overshoot slightly past zero so a closed shutter leaves no pinhole.
        let swing = blade_swing(
            (state.edge() * UNIT - Fixed::from_ratio(192, 100) * state.closure())
                .max(Fixed::from_ratio(-256, 100)),
        );
        let blade = &finish.blade;
        let edge = Paint::Color(Color::rgb(201, 214, 230).into());
        let glass = Paint::Color(Color::rgb(5, 5, 6).into());
        let unit_transform = lens.unit;
        let pupil = &self.pupil;
        let sheen = &self.sheen;
        let coat = &self.coat;
        let coatings = &self.coatings;
        lens.paint
            .with_clip_path(&GLASS_DISC, unit_transform, FillRule::EvenOdd, |paint| {
                let mut inner = Lens {
                    paint,
                    unit: unit_transform,
                    center,
                    radius,
                };
                inner.fill(&GLASS_DISC, Transform::IDENTITY, &glass, 255);
                inner.fill(
                    &CIRCLE,
                    Transform::translate(glance.x, glance.y).compose(&Transform::scale(
                        Fixed::from_ratio(64, 100),
                        Fixed::from_ratio(64, 100),
                    )),
                    pupil,
                    255,
                );
                for index in 0..BLADES {
                    let pose = blade_transform(index, swing);
                    inner.fill(&BLADE, pose, blade, 255);
                    inner.fill(&BLADE_EDGE, pose, &edge, 58);
                }
                let first = blade_transform(0, swing);
                inner.fill(&BLADE_LEAD, first, blade, 255);
                inner.fill(&BLADE_EDGE, first, &edge, 58);
                inner.fill(&GLASS_DISC, Transform::IDENTITY, sheen, 255);
                let drift = Fixed::sin_deg(phase) * 4;
                inner.fill(&coatings[0], Transform::rotate_deg(drift), &coat[0], 255);
                inner.fill(&coatings[1], Transform::rotate_deg(drift), &coat[1], 255);
            });

        lens.ring(GLASS_PER_MILLE + 8, Color::rgb(0, 0, 0), fine * 2, 170);
        lens.ring(
            GLASS_PER_MILLE + 22,
            if is_dark(theme) {
                Color::rgb(90, 100, 114)
            } else {
                Color::rgb(255, 255, 255)
            },
            fine,
            120,
        );
        let see = Paint::Color(theme.resolve(SEE).into());
        lens.fill(&INDEX, Transform::IDENTITY, &see, 255);
        let glint = Transform::translate(
            Fixed::from_ratio(1024, 100) + glance.x * 3 / 10,
            Fixed::from_ratio(-1216, 100) + glance.y * 3 / 10,
        );
        lens.fill(&GLINT, glint, &self.glint, 255);
    }
}

#[crate::view(component = IrisLens, name = "IrisLens", priority = 60)]
pub(super) fn lens_render(
    component: component!(),
    model: model!(component.model, watch(edge, closure, phase)),
    paint: paint!(),
    theme: res!(Theme),
) {
    component.render(paint, model, &theme);
}

#[cfg(test)]
pub(super) fn glass_radius() -> Fixed {
    Fixed::from_ratio(GLASS_PER_MILLE * UNIT, 1000)
}
