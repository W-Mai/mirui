use super::state::{OrbitModalSurface, OrbitSurface};
use super::style::{
    ANGLE_BOX, BACKGROUND, COAST, CYAN, DEEP, DV_BOX, FOOTER_TOP, INK, LINE, ORANGE, PAGE_TOP,
    PANEL, PLANET, RED, SPACE, VIOLET,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::orbit::{
    CRASH_RADIUS, MAX_NODES, MAX_TELEMETRY, OrbitCue, OrbitGoal, OrbitModal, OrbitModel, OrbitPage,
    START_RADIUS, TrackPoint,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::types::Transform;
use crate::ui::view::PaintCtx;

/// The space window of the map page, between the header and the footer.
const MAP: Rect = Rect {
    x: Fixed::ZERO,
    y: Fixed::from_int(PAGE_TOP),
    w: Fixed::from_int(480),
    h: Fixed::from_int(FOOTER_TOP - PAGE_TOP),
};
/// Map pixels per unit of distance.
const MAP_SCALE: Fixed = Fixed::from_ratio(9, 10);
/// The readouts at the right edge of the map.
const GAUGE_X: i32 = 386;
const GAUGE_W: i32 = 88;

fn map_center() -> Point {
    Point::new(226, 157)
}

fn map_length(units: i64) -> Fixed {
    Fixed::from_int(units as i32) * MAP_SCALE
}

fn track_point(point: TrackPoint) -> Point {
    let center = map_center();
    let scale = MAP_SCALE / 4;
    Point {
        x: center.x + Fixed::from_int(i32::from(point.x)) * scale,
        y: center.y + Fixed::from_int(i32::from(point.y)) * scale,
    }
}

fn goal_point(x: i16, y: i16) -> Point {
    track_point(TrackPoint { x: x * 4, y: y * 4 })
}

fn faded(color: Color, alpha: i32) -> Color {
    Color {
        a: alpha.clamp(0, 255) as u8,
        ..color
    }
}

/// How a goal is drawn: done, the one to reach now, or still ahead.
#[derive(Clone, Copy, Eq, PartialEq)]
enum GoalState {
    Done,
    Current,
    Ahead,
}

fn goal_states(model: &OrbitModel) -> impl Iterator<Item = (OrbitGoal, GoalState)> + '_ {
    let mission = model.mission();
    (0..mission.goal_count()).filter_map(move |index| {
        let state = match index.cmp(&model.completed()) {
            core::cmp::Ordering::Less => GoalState::Done,
            core::cmp::Ordering::Equal => GoalState::Current,
            core::cmp::Ordering::Greater => GoalState::Ahead,
        };
        Some((mission.goal(index)?, state))
    })
}

fn paint_shell(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, PAGE_TOP), INK, Fixed::ZERO);
    painter.fill(
        Rect::new(0, FOOTER_TOP, 480, 320 - FOOTER_TOP),
        INK,
        Fixed::ZERO,
    );
    for (left, width) in [DV_BOX, ANGLE_BOX] {
        painter.fill(
            Rect::new(left, FOOTER_TOP + 4, width, 28),
            DEEP,
            Fixed::ZERO,
        );
    }
}

fn paint_goal(painter: &mut PlayPainter<'_, '_>, goal: OrbitGoal, state: GoalState) {
    let center = map_center();
    let full = Fixed::from_int(360);
    match goal {
        OrbitGoal::Band { min, max } => {
            let inner = map_length(i64::from(min));
            let outer = map_length(i64::from(max));
            let (fill, edge) = match state {
                GoalState::Current => (70, 210),
                GoalState::Done => (24, 0),
                GoalState::Ahead => (0, 90),
            };
            if fill > 0 {
                painter.arc(
                    center,
                    (inner + outer) / 2,
                    Fixed::ZERO,
                    full,
                    faded(ORANGE, fill),
                    outer - inner,
                );
            }
            if edge > 0 {
                for radius in [inner, outer] {
                    painter.arc(
                        center,
                        radius,
                        Fixed::ZERO,
                        full,
                        faded(ORANGE, edge),
                        Fixed::ONE,
                    );
                }
            }
        }
        OrbitGoal::Beacon { x, y, range, .. } => {
            let point = goal_point(x, y);
            let (alpha, width) = match state {
                GoalState::Current => (255, Fixed::from_int(2)),
                GoalState::Done => (70, Fixed::ONE),
                GoalState::Ahead => (110, Fixed::ONE),
            };
            let color = faded(VIOLET, alpha);
            painter.arc(
                point,
                map_length(i64::from(range)),
                Fixed::ZERO,
                full,
                color,
                width,
            );
            painter.disc(point, Fixed::from_int(2), color);
        }
    }
}

/// A gauge bar under a readout at the right edge of the map.
fn paint_gauge(painter: &mut PlayPainter<'_, '_>, top: i32, value: i32, full: i32, color: Color) {
    painter.fill(Rect::new(GAUGE_X, top, GAUGE_W, 3), DEEP, Fixed::ZERO);
    let width = GAUGE_W * value.clamp(0, full) / full.max(1);
    painter.fill(Rect::new(GAUGE_X, top, width, 3), color, Fixed::ZERO);
}

fn paint_map(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    painter.fill(MAP, SPACE, Fixed::ZERO);
    let center = map_center();
    for (radius, color) in [
        (CRASH_RADIUS, faded(RED, 90)),
        (START_RADIUS, Color::rgb(41, 60, 77)),
    ] {
        painter.arc(
            center,
            map_length(radius),
            Fixed::ZERO,
            Fixed::from_int(360),
            color,
            Fixed::ONE,
        );
    }
    for (goal, state) in goal_states(model) {
        paint_goal(painter, goal, state);
    }
    painter.circle(center, Fixed::from_int(20), Color::rgb(244, 173, 74));
    painter.circle(center, Fixed::from_int(13), PLANET);
    let fuel = model.fuel().to_fixed().to_int();
    paint_gauge(painter, 50, fuel, model.mission().fuel.into(), CYAN);
    let heat = model.heat().to_fixed().to_int();
    paint_gauge(painter, 71, heat, 100, ORANGE);
}

/// Draws `points` as one line, starting from `from`, inside `clip`.
fn polyline(
    paint: &mut PaintCtx<'_>,
    transform: Transform,
    clip: Rect,
    from: Option<Point>,
    points: impl Iterator<Item = TrackPoint>,
    color: Color,
) {
    let mut previous = from;
    for point in points.map(track_point) {
        if let Some(from) = previous {
            paint.draw_clipped(
                &DrawCommand::Line {
                    p1: from,
                    p2: point,
                    transform,
                    color,
                    width: Fixed::ONE,
                    opa: 255,
                },
                clip,
            );
        }
        previous = Some(point);
    }
}

/// The past trail, the planned coast and the burn preview, cut to the
/// map window.
fn paint_tracks(paint: &mut PaintCtx<'_>, transform: Transform, model: &OrbitModel) {
    let Some(clip) = transform.apply_rect_bbox(MAP).intersect(&paint.clip()) else {
        return;
    };
    let craft = Some(track_point(TrackPoint::from_point(model.body().position)));
    let trail = (0..model.trail_len()).filter_map(|index| model.trail(index));
    polyline(paint, transform, clip, None, trail, CYAN);
    let coast = model.coast().iter().copied();
    polyline(paint, transform, clip, craft, coast, COAST);
    if model.preview() {
        let aim = model.aim().iter().copied();
        polyline(paint, transform, clip, craft, aim, ORANGE);
    }
}

fn paint_craft(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    for index in 0..MAX_NODES {
        if let Some(mark) = model.node_mark(index) {
            painter.arc(
                track_point(mark),
                Fixed::from_int(4),
                Fixed::ZERO,
                Fixed::from_int(360),
                ORANGE,
                Fixed::from_ratio(3, 2),
            );
        }
    }
    let body = model.body();
    let craft = track_point(TrackPoint::from_point(body.position));
    if !model.status().terminal() {
        let speed = body.speed().max(crate::types::Fixed64::ONE);
        let ux = (body.velocity.x / speed).to_fixed();
        let uy = (body.velocity.y / speed).to_fixed();
        let angle = Fixed::from_int(i32::from(model.angle_degrees()));
        let (cosine, sine) = (Fixed::cos_deg(angle), Fixed::sin_deg(angle));
        let length = Fixed::from_int(8) + Fixed::from_int(i32::from(model.dv_tenths())) / 4;
        let tip = Point {
            x: craft.x + (ux * cosine - uy * sine) * length,
            y: craft.y + (ux * sine + uy * cosine) * length,
        };
        painter.line(craft, tip, ORANGE, Fixed::from_ratio(3, 2));
    }
    painter.circle(craft, Fixed::from_int(5), CYAN);
    painter.circle(craft, Fixed::from_int(2), PANEL);
}

/// A ring that grows from `from` by `grow` pixels as a flash plays and fades.
fn flash_ring(
    painter: &mut PlayPainter<'_, '_>,
    center: Point,
    played: Fixed,
    (from, grow): (i32, i32),
    color: Color,
) {
    let alpha = ((Fixed::ONE - played) * 255).to_int();
    painter.arc(
        center,
        Fixed::from_int(from) + played * grow,
        Fixed::ZERO,
        Fixed::from_int(360),
        faded(color, alpha),
        Fixed::from_int(2),
    );
}

/// The flash of the last burn, arrival, sample or crash.
fn paint_cue(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    let Some((cue, at, played)) = model.cue_flash() else {
        return;
    };
    let alpha = ((Fixed::ONE - played) * 255).to_int();
    let craft = track_point(at);
    match cue {
        OrbitCue::Burn => {
            painter.disc(
                craft,
                Fixed::from_int(4) + played * 4,
                faded(ORANGE, alpha * 3 / 4),
            );
            flash_ring(painter, craft, played, (6, 18), ORANGE);
        }
        OrbitCue::Sample => {
            flash_ring(painter, craft, played, (6, 22), VIOLET);
            flash_ring(painter, craft, played, (3, 10), BACKGROUND);
        }
        OrbitCue::Crash => {
            painter.disc(craft, Fixed::from_int(6) + played * 10, faded(RED, alpha));
            flash_ring(painter, craft, played, (8, 24), RED);
        }
        OrbitCue::Arrive => {
            flash_ring(painter, craft, played, (6, 16), CYAN);
            match model.mission().goal(model.completed()) {
                Some(OrbitGoal::Band { min, max }) => {
                    for (radius, sign) in [(min, -1), (max, 1)] {
                        let radius = map_length(i64::from(radius)) + played * 6 * sign;
                        painter.arc(
                            map_center(),
                            radius,
                            Fixed::ZERO,
                            Fixed::from_int(360),
                            faded(ORANGE, alpha),
                            Fixed::from_int(2),
                        );
                    }
                }
                Some(OrbitGoal::Beacon { x, y, range, .. }) => {
                    let reach = map_length(i64::from(range)).to_int();
                    flash_ring(painter, goal_point(x, y), played, (reach, 12), VIOLET);
                }
                None => {}
            }
        }
    }
}

fn paint_plan(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    painter.fill(Rect::new(316, PAGE_TOP, 164, 254), SPACE, Fixed::ZERO);
    for index in 0..MAX_NODES {
        let y = 54 + index as i32 * 44;
        let planned = model.queue(index).is_some();
        painter.fill(
            Rect::new(8, y, 300, 38),
            if planned {
                Color::rgb(231, 233, 226)
            } else {
                PANEL
            },
            Fixed::ZERO,
        );
        painter.border(Rect::new(8, y, 300, 38), LINE, Fixed::ONE, Fixed::ZERO);
        if planned {
            painter.fill(Rect::new(8, y, 4, 38), ORANGE, Fixed::ZERO);
        }
    }
    // Fuel now, with the part the planned nodes will burn in orange.
    let full = i32::from(model.mission().fuel).max(1);
    let width = |fuel: crate::types::Fixed64| 146 * fuel.to_fixed().to_int().clamp(0, full) / full;
    let now = width(model.fuel());
    let planned = crate::types::Fixed64::from_ratio(model.planned_fuel().into(), 10);
    let after = width(model.fuel() - planned);
    painter.fill(Rect::new(326, 146, 146, 4), DEEP, Fixed::ZERO);
    painter.fill(Rect::new(326, 146, after, 4), CYAN, Fixed::ZERO);
    painter.fill(
        Rect::new(326 + after, 146, now - after, 4),
        ORANGE,
        Fixed::ZERO,
    );
}

fn paint_chart(
    painter: &mut PlayPainter<'_, '_>,
    model: &OrbitModel,
    rect: Rect,
    radius: bool,
    color: Color,
    max: i32,
) {
    painter.fill(rect, PANEL, Fixed::ZERO);
    let level = |tenths: i32| {
        rect.y + rect.h - rect.h * Fixed::from_int(tenths.clamp(0, max * 10)) / (max * 10)
    };
    let rule = |painter: &mut PlayPainter<'_, '_>, tenths: i32, color: Color| {
        let line = Rect {
            x: rect.x,
            y: level(tenths),
            w: rect.w,
            h: Fixed::ONE,
        };
        painter.fill(line, color, Fixed::ZERO);
    };
    for (goal, state) in goal_states(model) {
        let alpha = if state == GoalState::Current { 60 } else { 22 };
        match goal {
            OrbitGoal::Band { min, max } if radius => {
                let (top, bottom) = (level(i32::from(max) * 10), level(i32::from(min) * 10));
                painter.fill(
                    Rect {
                        x: rect.x,
                        y: top,
                        w: rect.w,
                        h: bottom - top,
                    },
                    faded(ORANGE, alpha),
                    Fixed::ZERO,
                );
            }
            OrbitGoal::Beacon { x, y, .. } if radius => {
                let distance = crate::types::Fixed64::from_int(
                    i64::from(x) * i64::from(x) + i64::from(y) * i64::from(y),
                )
                .sqrt();
                let tenths = (distance * 10).to_int() as i32;
                rule(painter, tenths, faded(VIOLET, alpha * 3));
            }
            OrbitGoal::Beacon { max_speed, .. } => {
                let tenths = i32::from(max_speed) * 10;
                rule(painter, tenths, faded(VIOLET, alpha * 3));
            }
            OrbitGoal::Band { .. } => {}
        }
    }
    painter.border(rect, LINE, Fixed::ONE, Fixed::ZERO);
    let len = model.telemetry_len();
    let mut previous = None;
    for index in 0..len {
        let Some(sample) = model.telemetry(index) else {
            continue;
        };
        let tenths = if radius {
            sample.radius_tenths
        } else {
            sample.speed_tenths
        };
        let point = Point {
            x: rect.x + rect.w * Fixed::from_int(index as i32) / (MAX_TELEMETRY as i32 - 1),
            y: level(i32::from(tenths)),
        };
        if let Some(from) = previous {
            painter.line(from, point, color, Fixed::from_ratio(3, 2));
        }
        previous = Some(point);
    }
}

fn paint_record(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    paint_chart(
        painter,
        model,
        Rect::new(10, 52, 320, 96),
        true,
        ORANGE,
        160,
    );
    paint_chart(
        painter,
        model,
        Rect::new(10, 172, 320, 104),
        false,
        CYAN,
        60,
    );
    painter.fill(Rect::new(340, 34, 134, 244), PANEL, Fixed::ZERO);
    painter.border(Rect::new(340, 34, 134, 244), LINE, Fixed::ONE, Fixed::ZERO);
}

#[crate::view(component = OrbitSurface, name = "OrbitSurface", priority = 60)]
pub(super) fn surface_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    paint_shell(&mut painter);
    match model.page() {
        OrbitPage::Map => paint_map(&mut painter, model),
        OrbitPage::Plan => paint_plan(&mut painter, model),
        OrbitPage::Record => paint_record(&mut painter, model),
    }
    if model.page() == OrbitPage::Map {
        paint_tracks(paint, transform, model);
        let mut painter = PlayPainter::new(paint, transform);
        paint_craft(&mut painter, model);
        paint_cue(&mut painter, model);
    }
}

#[crate::view(
    component = OrbitModalSurface,
    name = "OrbitModalSurface",
    priority = 70
)]
pub(super) fn modal_render(model: model!(component.model, watch(modal)), paint: paint!()) {
    if model.modal() == OrbitModal::None {
        return;
    }
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(12, 20, 30, 220),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(38, 69, 404, 194), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(38, 69, 404, 194),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(42, 73, 396, 32), INK, Fixed::ZERO);
    if model.modal() == OrbitModal::Result {
        painter.fill(Rect::new(42, 105, 396, 3), VIOLET, Fixed::ZERO);
    }
}
