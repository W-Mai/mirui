use super::state::{OrbitModalSurface, OrbitSurface};
use super::style::{
    BACKGROUND, COAST, CYAN, DEEP, INK, LINE, ORANGE, PANEL, PLANET, SPACE, VIOLET,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::orbit::{
    CRASH_RADIUS, MAX_NODES, MAX_TELEMETRY, OrbitGoal, OrbitModal, OrbitModel, OrbitPage,
    START_RADIUS, TrackPoint,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::types::Transform;
use crate::ui::view::PaintCtx;

/// The space window of the map page.
const MAP: Rect = Rect {
    x: Fixed::from_int(11),
    y: Fixed::from_int(65),
    w: Fixed::from_int(287),
    h: Fixed::from_int(188),
};
/// Map pixels per unit of distance.
const MAP_SCALE: Fixed = Fixed::from_ratio(7, 10);

fn map_center() -> Point {
    Point::new(154, 159)
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
    painter.fill(Rect::new(0, 0, 480, 31), INK, Fixed::ZERO);
    painter.fill(
        Rect::new(0, 31, 480, 28),
        Color::rgb(226, 231, 228),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(0, 282, 480, 38), INK, Fixed::ZERO);
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
                    Color::rgba(244, 139, 54, fill),
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
                        Color::rgba(244, 139, 54, edge),
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
            let color = Color::rgba(147, 112, 222, alpha);
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

fn paint_map(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    painter.fill(MAP, SPACE, Fixed::ZERO);
    painter.fill(Rect::new(307, 65, 162, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(307, 65, 162, 188), LINE, Fixed::ONE, Fixed::ZERO);
    let center = map_center();
    for (radius, color) in [
        (CRASH_RADIUS, Color::rgba(232, 92, 72, 90)),
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
    painter.circle(center, Fixed::from_int(17), Color::rgb(244, 173, 74));
    painter.circle(center, Fixed::from_int(11), PLANET);
    let fuel = model.fuel().to_fixed().to_int();
    let full = i32::from(model.mission().fuel).max(1);
    painter.fill(Rect::new(322, 88, 132, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(322, 88, 132 * fuel.clamp(0, full) / full, 4),
        CYAN,
        Fixed::ZERO,
    );
    painter.fill(Rect::new(322, 112, 132, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(
            322,
            112,
            132 * model.heat().to_fixed().to_int().min(100) / 100,
            4,
        ),
        ORANGE,
        Fixed::ZERO,
    );
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
        let length = Fixed::from_int(7) + Fixed::from_int(i32::from(model.dv_tenths())) / 5;
        let tip = Point {
            x: craft.x + (ux * cosine - uy * sine) * length,
            y: craft.y + (ux * sine + uy * cosine) * length,
        };
        painter.line(craft, tip, ORANGE, Fixed::from_ratio(3, 2));
    }
    painter.circle(craft, Fixed::from_int(5), CYAN);
    painter.circle(craft, Fixed::from_int(2), PANEL);
}

fn paint_plan(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    painter.fill(Rect::new(11, 65, 292, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(11, 65, 292, 188), LINE, Fixed::ONE, Fixed::ZERO);
    painter.fill(Rect::new(313, 65, 156, 188), SPACE, Fixed::ZERO);
    for index in 0..MAX_NODES {
        let y = 91 + index as i32 * 48;
        let planned = model.queue(index).is_some();
        painter.fill(
            Rect::new(24, y, 262, 38),
            if planned {
                Color::rgb(231, 233, 226)
            } else {
                BACKGROUND
            },
            Fixed::ZERO,
        );
        painter.border(Rect::new(24, y, 262, 38), LINE, Fixed::ONE, Fixed::ZERO);
        if planned {
            painter.fill(Rect::new(24, y, 4, 38), ORANGE, Fixed::ZERO);
        }
    }
    // Fuel now, with the part the planned nodes will burn in orange.
    let full = i32::from(model.mission().fuel).max(1);
    let width = |fuel: crate::types::Fixed64| 132 * fuel.to_fixed().to_int().clamp(0, full) / full;
    let now = width(model.fuel());
    let planned = crate::types::Fixed64::from_ratio(model.planned_fuel().into(), 10);
    let after = width(model.fuel() - planned);
    painter.fill(Rect::new(325, 245, 132, 4), DEEP, Fixed::ZERO);
    painter.fill(Rect::new(325, 245, after, 4), CYAN, Fixed::ZERO);
    painter.fill(
        Rect::new(325 + after, 245, now - after, 4),
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
                    Color::rgba(244, 139, 54, alpha),
                    Fixed::ZERO,
                );
            }
            OrbitGoal::Beacon { x, y, .. } if radius => {
                let distance = crate::types::Fixed64::from_int(
                    i64::from(x) * i64::from(x) + i64::from(y) * i64::from(y),
                )
                .sqrt();
                let tenths = (distance * 10).to_int() as i32;
                rule(painter, tenths, Color::rgba(147, 112, 222, alpha * 3));
            }
            OrbitGoal::Beacon { max_speed, .. } => {
                let tenths = i32::from(max_speed) * 10;
                rule(painter, tenths, Color::rgba(147, 112, 222, alpha * 3));
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
        Rect::new(15, 88, 286, 74),
        true,
        ORANGE,
        160,
    );
    paint_chart(painter, model, Rect::new(15, 190, 286, 58), false, CYAN, 60);
    painter.fill(Rect::new(313, 65, 156, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(313, 65, 156, 188), LINE, Fixed::ONE, Fixed::ZERO);
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
        paint_craft(&mut PlayPainter::new(paint, transform), model);
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
    painter.fill(Rect::new(38, 69, 404, 190), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(38, 69, 404, 190),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(42, 73, 396, 32), INK, Fixed::ZERO);
    if model.modal() == OrbitModal::Result {
        painter.fill(Rect::new(42, 105, 396, 3), VIOLET, Fixed::ZERO);
    }
}
