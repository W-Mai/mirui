//! A vintage tri-compax chronograph drawn from vector primitives.
//!
//! Everything is laid out in a 200 unit radius and scaled once, so the face
//! stays crisp at any size. The watch keeps its own printed colours, as a real
//! dial does, and does not follow the theme.

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::raster::FillRule;
use crate::render::scene::Paint;
use crate::types::Transform;

/// Time of day the watch starts at, the one every advertisement shows.
const START_MS: u64 = (10 * 3600 + 8 * 60 + 36) * 1000;
/// The chronograph is already running when the page opens.
const CHRONO_HEAD_START_MS: u64 = 7_000;
const HEADER_HEIGHT: i32 = 38;

#[crate::component]
#[derive(Clone, Copy, Default)]
pub struct Shapes {
    pub start_ms: u32,
    pub dial: u8,
}

impl Shapes {
    pub(super) fn next_dial(self) -> Self {
        Self {
            dial: (self.dial + 1) % DIALS.len() as u8,
            ..self
        }
    }
}

/// Printed colours of one dial.
#[derive(Clone, Copy)]
struct Dial {
    edge: Color,
    bezel: Color,
    groove: Color,
    face: Color,
    inset: Color,
    ink: Color,
    lume: Color,
    hand: Color,
    sweep: Color,
}

const DIALS: [Dial; 3] = [
    // Ivory dial with blued hands.
    Dial {
        edge: Color::rgb(56, 52, 48),
        bezel: Color::rgb(200, 196, 186),
        groove: Color::rgb(92, 88, 80),
        face: Color::rgb(236, 227, 204),
        inset: Color::rgb(223, 211, 184),
        ink: Color::rgb(52, 44, 38),
        lume: Color::rgb(214, 196, 138),
        hand: Color::rgb(46, 62, 98),
        sweep: Color::rgb(176, 48, 36),
    },
    // Black dial with gold hands.
    Dial {
        edge: Color::rgb(20, 20, 22),
        bezel: Color::rgb(158, 160, 166),
        groove: Color::rgb(34, 34, 38),
        face: Color::rgb(26, 27, 30),
        inset: Color::rgb(14, 14, 16),
        ink: Color::rgb(226, 220, 200),
        lume: Color::rgb(236, 226, 180),
        hand: Color::rgb(214, 176, 92),
        sweep: Color::rgb(232, 86, 60),
    },
    // Deep green dial with brass hands.
    Dial {
        edge: Color::rgb(14, 34, 32),
        bezel: Color::rgb(170, 178, 174),
        groove: Color::rgb(22, 52, 48),
        face: Color::rgb(30, 84, 76),
        inset: Color::rgb(22, 64, 58),
        ink: Color::rgb(232, 226, 206),
        lume: Color::rgb(238, 228, 182),
        hand: Color::rgb(224, 198, 130),
        sweep: Color::rgb(242, 152, 62),
    },
];

static CIRCLE: Path = path!(
    "M 100 0 C 100 55.228 55.228 100 0 100 C -55.228 100 -100 55.228 -100 0 C -100 -55.228 -55.228 -100 0 -100 C 55.228 -100 100 -55.228 100 0 Z"
);
static HOUR_HAND: Path = path!("M 0 -104 L 8 -30 L 5 20 L -5 20 L -8 -30 Z");
static HOUR_LUME: Path = path!("M 0 -92 L 4 -36 L -4 -36 Z");
static MINUTE_HAND: Path = path!("M 0 -152 L 6 -32 L 4 24 L -4 24 L -6 -32 Z");
static MINUTE_LUME: Path = path!("M 0 -140 L 2.6 -38 L -2.6 -38 Z");
static TRIANGLE: Path = path!("M -13 -154 L 13 -154 L 0 -118 Z");
static TRIANGLE_LUME: Path = path!("M -7 -148 L 7 -148 L 0 -128 Z");

/// Sub-dial centres: small seconds, 30 minute counter, 12 hour counter.
const SUBDIALS: [(i32, i32, i32); 3] = [(-68, 0, 12), (68, 0, 10), (0, 72, 12)];
const SUBDIAL_RADIUS: i32 = 40;

fn fx(v: i32) -> Fixed {
    Fixed::from_int(v)
}

fn fr(n: i32, d: i32) -> Fixed {
    Fixed::from_ratio(n, d)
}

/// The point `r` units from the origin along `angle` degrees clockwise from 12.
fn toward(angle: Fixed, r: i32) -> Point {
    Point {
        x: Fixed::sin_deg(angle) * fx(r),
        y: Fixed::ZERO - Fixed::cos_deg(angle) * fx(r),
    }
}

/// `value` within `period`, as tenths of a degree turned.
fn turn(value: u64, period: u64) -> Fixed {
    fr(((value % period) * 3_600 / period) as i32, 10)
}

fn disc(paint: &mut PaintCtx<'_>, t: Transform, (cx, cy): (i32, i32), r: i32, color: Color) {
    disc_at(
        paint,
        t,
        Point {
            x: fx(cx),
            y: fx(cy),
        },
        r,
        color,
        255,
    );
}

/// A filled circle, drawn as four curves so that no radius is too large for
/// the rounded rectangle fill.
fn disc_at(paint: &mut PaintCtx<'_>, t: Transform, c: Point, r: i32, color: Color, opa: u8) {
    let placed = t
        .compose(&Transform::translate(c.x, c.y))
        .compose(&Transform::scale(fr(r, 100), fr(r, 100)));
    paint.draw(&DrawCommand::FillPath {
        path: &CIRCLE,
        transform: placed,
        paint: &Paint::Color(color.into()),
        opa,
        fill_rule: FillRule::NonZero,
    });
}

fn block(paint: &mut PaintCtx<'_>, t: Transform, area: Rect, radius: i32, color: Color) {
    paint.draw(&DrawCommand::Fill {
        area,
        transform: t,
        quad: None,
        color,
        radius: fx(radius),
        opa: 255,
    });
}

fn ring(paint: &mut PaintCtx<'_>, t: Transform, centre: Point, r: i32, width: Fixed, color: Color) {
    paint.draw(&DrawCommand::Arc {
        center: centre,
        transform: t,
        radius: fx(r),
        start_angle: Fixed::ZERO,
        end_angle: fx(360),
        color,
        width,
        opa: 255,
    });
}

fn stroke(paint: &mut PaintCtx<'_>, t: Transform, p1: Point, p2: Point, width: Fixed, c: Color) {
    paint.draw(&DrawCommand::Line {
        p1,
        p2,
        transform: t,
        color: c,
        width,
        opa: 255,
    });
}

fn shape(paint: &mut PaintCtx<'_>, path: &Path, t: Transform, color: Color) {
    paint.draw(&DrawCommand::FillPath {
        path,
        transform: t,
        paint: &Paint::Color(color.into()),
        opa: 255,
        fill_rule: FillRule::NonZero,
    });
}

fn offset(p: Point, (x, y): (i32, i32)) -> Point {
    Point {
        x: p.x + fx(x),
        y: p.y + fx(y),
    }
}

/// A needle on a sub-dial: a short tail behind the pivot and the blade.
fn needle(paint: &mut PaintCtx<'_>, t: Transform, at: (i32, i32), angle: Fixed, color: Color) {
    let tail = offset(toward(angle, -9), at);
    let tip = offset(toward(angle, SUBDIAL_RADIUS - 8), at);
    stroke(paint, t, tail, tip, fr(5, 2), color);
    disc(paint, t, at, 3, color);
}

fn case(paint: &mut PaintCtx<'_>, t: Transform, d: &Dial) {
    let origin = Point::ZERO;
    // Soft contact shadow under the whole watch.
    disc_at(
        paint,
        t,
        Point {
            x: Fixed::ZERO,
            y: fx(6),
        },
        204,
        Color::rgb(0, 0, 0),
        56,
    );

    // Crown and the two chronograph pushers sit behind the case.
    block(paint, t, Rect::new(194, -18, 22, 36), 6, d.bezel);
    block(paint, t, Rect::new(194, -12, 4, 24), 1, d.groove);
    for side in [-30, 30] {
        let rotated = t.compose(&Transform::rotate_deg(fx(side)));
        block(paint, rotated, Rect::new(188, -6, 18, 12), 2, d.groove);
        block(paint, rotated, Rect::new(204, -11, 14, 22), 5, d.bezel);
    }

    disc(paint, t, (0, 0), 200, d.edge);
    disc(paint, t, (0, 0), 195, d.bezel);
    ring(paint, t, origin, 187, fr(6, 5), d.groove);
    disc(paint, t, (0, 0), 179, d.groove);
    disc(paint, t, (0, 0), 175, d.face);
}

fn railway(paint: &mut PaintCtx<'_>, t: Transform, d: &Dial) {
    let origin = Point::ZERO;
    ring(paint, t, origin, 166, fr(8, 5), d.ink);
    ring(paint, t, origin, 154, fr(8, 5), d.ink);
    for i in 0..60 {
        let angle = fx(i * 6);
        if i % 5 == 0 {
            if i == 0 {
                continue;
            }
            stroke(
                paint,
                t,
                toward(angle, 118),
                toward(angle, 150),
                fx(7),
                d.ink,
            );
            stroke(
                paint,
                t,
                toward(angle, 124),
                toward(angle, 144),
                fr(7, 2),
                d.lume,
            );
        } else {
            stroke(
                paint,
                t,
                toward(angle, 154),
                toward(angle, 166),
                fr(8, 5),
                d.ink,
            );
        }
    }
    shape(paint, &TRIANGLE, t, d.ink);
    shape(paint, &TRIANGLE_LUME, t, d.lume);
}

fn subdials(paint: &mut PaintCtx<'_>, t: Transform, d: &Dial) {
    for (x, y, ticks) in SUBDIALS {
        let centre = Point { x: fx(x), y: fx(y) };
        disc(paint, t, (x, y), SUBDIAL_RADIUS, d.inset);
        ring(paint, t, centre, SUBDIAL_RADIUS, fr(8, 5), d.ink);
        ring(paint, t, centre, SUBDIAL_RADIUS - 5, fr(4, 5), d.ink);
        for i in 0..ticks {
            let angle = fr(i * 3_600, ticks * 10);
            let major = i % 3 == 0;
            let inner = if major { 27 } else { 31 };
            stroke(
                paint,
                t,
                offset(toward(angle, inner), (x, y)),
                offset(toward(angle, SUBDIAL_RADIUS - 6), (x, y)),
                if major { fx(2) } else { fr(6, 5) },
                d.ink,
            );
        }
    }
}

#[crate::view(component = Shapes, name = "Shapes", priority = 60)]
fn shapes_render(state: component!(), clock: res!(optional MonoClock), paint: paint!()) {
    let dial = DIALS[usize::from(state.dial) % DIALS.len()];
    let now_ms = clock.map(|c| c.now_ms()).unwrap_or(0);
    let elapsed = u64::from(now_ms.wrapping_sub(state.start_ms));
    let wall = START_MS + elapsed;
    let chrono = CHRONO_HEAD_START_MS + elapsed;

    let rect = paint.rect();
    let content_height = (rect.h - fx(HEADER_HEIGHT)).max(Fixed::ZERO);
    let cx = rect.x + rect.w / fx(2);
    let cy = rect.y + fx(HEADER_HEIGHT) + content_height / fx(2);
    // The crown reaches 1.1 radii to the right.
    let radius = ((rect.w / fx(2) - fx(10)) * fx(10) / fx(11)).min(content_height / fx(2) - fx(10));
    let unit = radius / fx(200);
    let t = paint
        .transform()
        .compose(&Transform::translate(cx, cy))
        .compose(&Transform::scale(unit, unit));

    case(paint, t, &dial);
    railway(paint, t, &dial);
    subdials(paint, t, &dial);

    // Maker's mark under the 12.
    crate::gallery::brand::draw_eye(
        paint,
        t.compose(&Transform::translate(Fixed::ZERO, fx(-70)))
            .compose(&Transform::scale(fr(7, 5), fr(7, 5))),
        dial.ink,
        dial.sweep,
    );

    // Sub-dial needles: seconds of the day, chronograph minutes and hours.
    needle(
        paint,
        t,
        (SUBDIALS[0].0, SUBDIALS[0].1),
        turn(wall, 60_000),
        dial.ink,
    );
    needle(
        paint,
        t,
        (SUBDIALS[1].0, SUBDIALS[1].1),
        turn(chrono, 1_800_000),
        dial.ink,
    );
    needle(
        paint,
        t,
        (SUBDIALS[2].0, SUBDIALS[2].1),
        turn(chrono, 43_200_000),
        dial.ink,
    );

    let hour = t.compose(&Transform::rotate_deg(turn(wall, 43_200_000)));
    shape(paint, &HOUR_HAND, hour, dial.hand);
    shape(paint, &HOUR_LUME, hour, dial.lume);
    let minute = t.compose(&Transform::rotate_deg(turn(wall, 3_600_000)));
    shape(paint, &MINUTE_HAND, minute, dial.hand);
    shape(paint, &MINUTE_LUME, minute, dial.lume);

    // Chronograph seconds hand with its counterweight, then the hub.
    let sweep = turn(chrono, 60_000);
    stroke(
        paint,
        t,
        toward(sweep, -36),
        toward(sweep, 160),
        fr(13, 5),
        dial.sweep,
    );
    let weight = toward(sweep, -30);
    disc_at(paint, t, weight, 7, dial.sweep, 255);
    disc(paint, t, (0, 0), 10, dial.hand);
    disc(paint, t, (0, 0), 4, dial.sweep);

    // Glints on the crystal.
    let origin = Point::ZERO;
    paint.draw(&DrawCommand::Arc {
        center: origin,
        transform: t,
        radius: fx(168),
        start_angle: fx(200),
        end_angle: fx(250),
        color: Color::rgb(255, 255, 255),
        width: fx(5),
        opa: 40,
    });
    paint.draw(&DrawCommand::Arc {
        center: origin,
        transform: t,
        radius: fx(168),
        start_angle: fx(20),
        end_angle: fx(50),
        color: Color::rgb(255, 255, 255),
        width: fx(3),
        opa: 22,
    });
}

pub use shapes_render::view as shapes_view;

#[mirui_macros::system(order = Animation)]
pub fn shapes_anim_system(shapes: query!(mut Shapes)) {
    for _shape in shapes {}
}
