use super::style::{ACCENT, BACKGROUND, BOARD, HINT, LIGHT, SUCCESS, TILE};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::lumen::{BeamStatus, GridPoint, LumenModel, MirrorOrientation};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::path::Path;
use crate::types::Transform;

/// The board's own size; the view scales it to its box.
pub(super) const BOARD_WIDTH: i32 = 378;
pub(super) const BOARD_HEIGHT: i32 = 272;
/// Distance between cell centres.
const PITCH: i32 = 52;
/// Centre of the top-left cell.
const ORIGIN: (i32, i32) = (33, 32);

/// A lamp, centred on its cell.
static LAMP: Path = crate::path!("M 0 -12 L 12 0 L 0 12 L -12 0 Z");

#[crate::component]
pub(super) struct LumenBoard {
    pub(super) model: model!(LumenModel),
}

pub(super) fn cell_center(point: GridPoint) -> Point {
    Point::new(
        ORIGIN.0 + i32::from(point.x) * PITCH,
        ORIGIN.1 + i32::from(point.y) * PITCH,
    )
}

/// A square of `half` pixels each way around `center`.
fn square(center: Point, half: i32) -> Rect {
    Rect::new(
        center.x - Fixed::from_int(half),
        center.y - Fixed::from_int(half),
        half * 2,
        half * 2,
    )
}

/// A circle outline of `radius` around `center`.
fn ring(center: Point, radius: Fixed) -> Rect {
    Rect {
        x: center.x - radius,
        y: center.y - radius,
        w: radius * 2,
        h: radius * 2,
    }
}

fn between(from: Point, to: Point, fraction: Fixed) -> Point {
    Point {
        x: from.x + (to.x - from.x) * fraction,
        y: from.y + (to.y - from.y) * fraction,
    }
}

/// Opacity that fades out as `progress` runs from 0 to 1.
fn fading(progress: Fixed, full: i32) -> u8 {
    (Fixed::from_int(full) * (Fixed::ONE - progress))
        .to_int()
        .clamp(0, 255) as u8
}

/// The beam up to `front` segments along its path, with a bright head while
/// it is still being drawn.
fn paint_beam(
    painter: &mut PlayPainter<'_, '_>,
    points: &[GridPoint],
    front: Fixed,
    drawing: bool,
) {
    let mut head = None;
    for (index, pair) in points.windows(2).enumerate() {
        let start = Fixed::from_int(index as i32);
        if front <= start {
            break;
        }
        let from = cell_center(pair[0]);
        let to = between(from, cell_center(pair[1]), (front - start).min(Fixed::ONE));
        painter.line(from, to, ACCENT, Fixed::from_int(3));
        painter.line(from, to, LIGHT, Fixed::from_ratio(3, 2));
        head = Some(to);
    }
    if drawing && let Some(head) = head {
        painter.circle(head, Fixed::from_int(4), LIGHT);
    }
}

/// A point of light `phase` of the way along the whole beam.
fn scan_point(points: &[GridPoint], phase: u16) -> Option<Point> {
    let segments = points.len().checked_sub(1)?;
    let scaled = u32::from(phase) * segments as u32;
    let segment = ((scaled >> 16) as usize).min(segments - 1);
    let fraction = Fixed::from_ratio((scaled & 0xffff) as i32, 65_536);
    Some(between(
        cell_center(points[segment]),
        cell_center(points[segment + 1]),
        fraction,
    ))
}

/// How far the beam front has run past `cell`, in segments; `None` while it
/// has not reached it.
fn past(points: &[GridPoint], cell: GridPoint, front: Fixed) -> Option<Fixed> {
    let reached = Fixed::from_int(points.iter().position(|point| *point == cell)? as i32);
    (front >= reached).then_some(front - reached)
}

/// The angle of a mirror line in degrees; `/` points up to the right.
fn mirror_angle(orientation: MirrorOrientation) -> Fixed {
    match orientation {
        MirrorOrientation::Slash => Fixed::from_int(-45),
        MirrorOrientation::Backslash => Fixed::from_int(45),
    }
}

/// The two ends of a mirror line `reach` pixels from `center`.
fn mirror_ends(center: Point, angle: Fixed, reach: i32) -> (Point, Point) {
    let end = Transform::rotate_deg(angle).apply_point(Point::new(reach, 0));
    (
        Point {
            x: center.x - end.x,
            y: center.y - end.y,
        },
        Point {
            x: center.x + end.x,
            y: center.y + end.y,
        },
    )
}

fn paint_wall(painter: &mut PlayPainter<'_, '_>, center: Point) {
    painter.fill(
        square(center, 18),
        Color::rgb(59, 68, 68),
        Fixed::from_int(5),
    );
    painter.border(
        square(center, 18),
        Color::rgb(89, 97, 92),
        Fixed::ONE,
        Fixed::from_int(5),
    );
    for offset in [-8, 0, 8] {
        painter.line(
            Point::new(
                center.x - Fixed::from_int(11),
                center.y + Fixed::from_int(offset),
            ),
            Point::new(
                center.x + Fixed::from_int(11),
                center.y + Fixed::from_int(offset),
            ),
            Color::rgb(98, 107, 96),
            Fixed::ONE,
        );
    }
}

/// A lamp: a diamond that fills with light once the beam front has crossed
/// it, sending out one ring as it does. `light` is how far the front has run
/// past it, in cells.
fn paint_lamp(painter: &mut PlayPainter<'_, '_>, center: Point, light: Option<Fixed>) {
    if let Some(light) = light {
        let spread = light.min(Fixed::ONE);
        painter.circle(
            center,
            Fixed::from_int(11) + Fixed::from_int(5) * spread,
            Color::rgb(72, 68, 48),
        );
        if spread < Fixed::ONE {
            let radius = Fixed::from_int(14) + Fixed::from_int(12) * spread;
            painter.border(
                ring(center, radius),
                Color::rgba(247, 239, 196, fading(spread, 220)),
                Fixed::from_int(2),
                radius,
            );
        }
    }
    let mut lamp = painter.transformed(Transform::translate(center.x, center.y));
    lamp.fill_path(
        &LAMP,
        if light.is_some() {
            LIGHT
        } else {
            Color::rgb(30, 41, 43)
        },
    );
    lamp.stroke_path(
        &LAMP,
        if light.is_some() {
            ACCENT
        } else {
            Color::rgb(138, 156, 146)
        },
        Fixed::from_int(2),
    );
}

/// A rotatable mirror: a framed plate with gold pivots at both ends.
fn paint_mirror(
    painter: &mut PlayPainter<'_, '_>,
    center: Point,
    angle: Fixed,
    selected: bool,
    hinted: bool,
) {
    painter.fill(
        square(center, 20),
        if hinted {
            Color::rgb(72, 60, 54)
        } else if selected {
            Color::rgb(56, 68, 71)
        } else {
            Color::rgb(44, 57, 61)
        },
        Fixed::from_int(8),
    );
    painter.border(
        square(center, 20),
        if hinted {
            HINT
        } else if selected {
            ACCENT
        } else {
            Color::rgb(115, 134, 133)
        },
        if hinted {
            Fixed::from_int(2)
        } else {
            Fixed::ONE
        },
        Fixed::from_int(8),
    );
    let (first, second) = mirror_ends(center, angle, 17);
    painter.line(
        first,
        second,
        Color::rgb(220, 234, 221),
        Fixed::from_ratio(5, 2),
    );
    painter.circle(first, Fixed::from_ratio(5, 2), ACCENT);
    painter.circle(second, Fixed::from_ratio(5, 2), ACCENT);
}

/// A fixed mirror: a square, riveted plate with a plain steel line.
fn paint_fixed_mirror(
    painter: &mut PlayPainter<'_, '_>,
    center: Point,
    orientation: MirrorOrientation,
) {
    painter.fill(
        square(center, 20),
        Color::rgb(36, 45, 48),
        Fixed::from_int(3),
    );
    painter.border(
        square(center, 20),
        Color::rgb(76, 88, 89),
        Fixed::ONE,
        Fixed::from_int(3),
    );
    for (dx, dy) in [(-15, -15), (15, -15), (-15, 15), (15, 15)] {
        painter.circle(
            Point::new(
                center.x + Fixed::from_int(dx),
                center.y + Fixed::from_int(dy),
            ),
            Fixed::from_int(2),
            Color::rgb(112, 124, 120),
        );
    }
    let (first, second) = mirror_ends(center, mirror_angle(orientation), 18);
    painter.line(first, second, Color::rgb(170, 182, 177), Fixed::from_int(4));
}

/// The receiver: a ring that turns green with a check once it is lit, and
/// sends one ring of light outwards as it does.
fn paint_receiver(
    painter: &mut PlayPainter<'_, '_>,
    center: Point,
    status: Option<BeamStatus>,
    glow: Option<Fixed>,
) {
    if let Some(glow) = glow {
        let radius = Fixed::from_int(17) + Fixed::from_int(16) * glow;
        painter.border(
            ring(center, radius),
            Color::rgba(189, 225, 154, fading(glow, 230)),
            Fixed::from_int(3),
            radius,
        );
    }
    let edge = match status {
        Some(BeamStatus::Lit) => SUCCESS,
        Some(BeamStatus::LampsDark) => HINT,
        _ => Color::rgb(161, 195, 176),
    };
    painter.circle(center, Fixed::from_int(16), edge);
    painter.circle(center, Fixed::from_int(13), Color::rgb(26, 41, 39));
    if status != Some(BeamStatus::Lit) {
        return;
    }
    painter.circle(center, Fixed::from_int(8), SUCCESS);
    painter.line(
        Point::new(center.x - Fixed::from_int(5), center.y),
        Point::new(center.x - Fixed::ONE, center.y + Fixed::from_int(4)),
        BACKGROUND,
        Fixed::from_ratio(5, 2),
    );
    painter.line(
        Point::new(center.x - Fixed::ONE, center.y + Fixed::from_int(4)),
        Point::new(center.x + Fixed::from_int(6), center.y - Fixed::from_int(5)),
        BACKGROUND,
        Fixed::from_ratio(5, 2),
    );
}

fn paint_board(painter: &mut PlayPainter<'_, '_>, model: &LumenModel) {
    let level = model.level();
    let motion = model.motion();
    let trace = model.trace();
    let points = trace.points();
    let front = motion.front();
    let complete = motion.beam_complete();
    painter.fill(
        Rect::new(0, 0, BOARD_WIDTH, BOARD_HEIGHT),
        BOARD,
        Fixed::from_int(10),
    );
    painter.border(
        Rect::new(0, 0, BOARD_WIDTH, BOARD_HEIGHT),
        Color::rgb(54, 68, 67),
        Fixed::ONE,
        Fixed::from_int(10),
    );
    for row in 0..5 {
        for column in 0..7 {
            let center = cell_center(GridPoint { x: column, y: row });
            painter.fill(square(center, 23), TILE, Fixed::from_int(6));
            painter.circle(center, Fixed::ONE, Color::rgb(82, 96, 91));
        }
    }

    paint_beam(painter, points, front, !complete);
    if complete {
        for offset in [0_u16, 21_845, 43_690] {
            if let Some(point) = scan_point(points, model.scan_phase().wrapping_add(offset)) {
                painter.circle(point, Fixed::from_ratio(5, 2), LIGHT);
            }
        }
    }

    for wall in level.walls {
        paint_wall(painter, cell_center(*wall));
    }
    for (index, lamp) in level.lamps.iter().enumerate() {
        let lit = trace.lit_lamps & (1 << index) != 0;
        let light = if lit {
            past(points, *lamp, front)
        } else {
            None
        };
        paint_lamp(painter, cell_center(*lamp), light);
    }
    for (index, mirror) in level.mirrors.iter().enumerate() {
        let resting = mirror_angle(model.orientations()[index]);
        let angle = motion.turn_of(index).map_or(resting, |turn| {
            resting - Fixed::from_int(90) + Fixed::from_int(90) * turn
        });
        paint_mirror(
            painter,
            cell_center(mirror.position),
            angle,
            model.selected() == Some(index),
            model.hint() == Some(index),
        );
    }
    for (index, mirror) in level.fixed.iter().enumerate() {
        let center = cell_center(mirror.position);
        let shaken = Point {
            x: center.x + motion.shake_of(index),
            y: center.y,
        };
        paint_fixed_mirror(painter, shaken, mirror.orientation);
    }
    paint_receiver(
        painter,
        cell_center(level.target),
        complete.then(|| model.beam_status()),
        motion.glow(),
    );

    let source_y = cell_center(GridPoint {
        x: 0,
        y: level.source_row,
    })
    .y;
    painter.line(
        Point::new(1, source_y - Fixed::from_int(6)),
        Point::new(7, source_y),
        ACCENT,
        Fixed::from_int(2),
    );
    painter.line(
        Point::new(7, source_y),
        Point::new(1, source_y + Fixed::from_int(6)),
        ACCENT,
        Fixed::from_int(2),
    );
}

#[crate::view(component = LumenBoard, name = "LumenBoard", priority = 60)]
pub(super) fn board_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), BOARD_WIDTH, BOARD_HEIGHT);
    let mut painter = PlayPainter::new(paint, transform);
    paint_board(&mut painter, model);
}

/// The cell under a tap, in board columns and rows.
fn tapped_cell(rect: Rect, x: Fixed, y: Fixed) -> Option<(i8, i8)> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = ((x - rect.x) * Fixed::from_int(BOARD_WIDTH) / rect.w).to_int();
    let local_y = ((y - rect.y) * Fixed::from_int(BOARD_HEIGHT) / rect.h).to_int();
    let column = (local_x - (ORIGIN.0 - PITCH / 2)).div_euclid(PITCH);
    let row = (local_y - (ORIGIN.1 - PITCH / 2)).div_euclid(PITCH);
    ((0..7).contains(&column) && (0..5).contains(&row)).then_some((column as i8, row as i8))
}

pub(super) fn board_tap(board: &LumenBoard, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some((column, row)) = tapped_cell(rect, x, y) else {
        return false;
    };
    board.model.rotate_cell(column, row);
    true
}
