use super::style::{ACCENT, BACKGROUND, BOARD, HINT, LIGHT, SUCCESS, TILE};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::lumen::{BeamStatus, GridPoint, LumenModel, MirrorOrientation, Trace};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::path::Path;
use crate::types::Transform;

/// A lamp, centred on its cell.
static LAMP: Path = crate::path!("M 0 -9 L 9 0 L 0 9 L -9 0 Z");

#[crate::component]
pub(super) struct LumenBoard {
    pub(super) model: model!(LumenModel),
}

pub(super) fn cell_center(point: GridPoint) -> Point {
    Point::new(27 + i32::from(point.x) * 38, 26 + i32::from(point.y) * 38)
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

fn paint_trace(painter: &mut PlayPainter<'_, '_>, trace: &Trace) {
    let points = trace.points();
    for pair in points.windows(2) {
        let from = cell_center(pair[0]);
        let to = cell_center(pair[1]);
        painter.line(from, to, ACCENT, Fixed::from_ratio(12, 5));
        painter.line(from, to, LIGHT, Fixed::ONE);
    }
}

fn scan_point(trace: &Trace, phase: u16) -> Option<Point> {
    let points = trace.points();
    let segments = points.len().checked_sub(1)?;
    let scaled = u32::from(phase) * segments as u32;
    let segment = ((scaled >> 16) as usize).min(segments - 1);
    let fraction = Fixed::from_ratio((scaled & 0xffff) as i32, 65_536);
    let from = cell_center(points[segment]);
    let to = cell_center(points[segment + 1]);
    Some(Point {
        x: from.x + (to.x - from.x) * fraction,
        y: from.y + (to.y - from.y) * fraction,
    })
}

/// The two ends of a mirror line `half` pixels from `center`.
fn mirror_ends(center: Point, orientation: MirrorOrientation, half: i32) -> (Point, Point) {
    let rise = if orientation == MirrorOrientation::Slash {
        half
    } else {
        -half
    };
    (
        Point::new(
            center.x - Fixed::from_int(half),
            center.y + Fixed::from_int(rise),
        ),
        Point::new(
            center.x + Fixed::from_int(half),
            center.y - Fixed::from_int(rise),
        ),
    )
}

fn paint_wall(painter: &mut PlayPainter<'_, '_>, center: Point) {
    painter.fill(
        square(center, 13),
        Color::rgb(59, 68, 68),
        Fixed::from_int(4),
    );
    painter.border(
        square(center, 13),
        Color::rgb(89, 97, 92),
        Fixed::ONE,
        Fixed::from_int(4),
    );
    for offset in [-6, 0, 6] {
        painter.line(
            Point::new(
                center.x - Fixed::from_int(8),
                center.y + Fixed::from_int(offset),
            ),
            Point::new(
                center.x + Fixed::from_int(8),
                center.y + Fixed::from_int(offset),
            ),
            Color::rgb(98, 107, 96),
            Fixed::ONE,
        );
    }
}

/// A lamp: a diamond that fills with light once the beam has crossed it.
fn paint_lamp(painter: &mut PlayPainter<'_, '_>, center: Point, lit: bool) {
    if lit {
        painter.circle(center, Fixed::from_int(12), Color::rgb(72, 68, 48));
    }
    let mut lamp = painter.transformed(Transform::translate(center.x, center.y));
    lamp.fill_path(&LAMP, if lit { LIGHT } else { Color::rgb(30, 41, 43) });
    lamp.stroke_path(
        &LAMP,
        if lit {
            ACCENT
        } else {
            Color::rgb(138, 156, 146)
        },
        Fixed::from_ratio(3, 2),
    );
}

/// A rotatable mirror: a framed plate with gold pivots at both ends.
fn paint_mirror(
    painter: &mut PlayPainter<'_, '_>,
    center: Point,
    orientation: MirrorOrientation,
    selected: bool,
    hinted: bool,
) {
    painter.fill(
        square(center, 15),
        if hinted {
            Color::rgb(72, 60, 54)
        } else if selected {
            Color::rgb(56, 68, 71)
        } else {
            Color::rgb(44, 57, 61)
        },
        Fixed::from_int(6),
    );
    painter.border(
        square(center, 15),
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
        Fixed::from_int(6),
    );
    let (first, second) = mirror_ends(center, orientation, 9);
    painter.line(first, second, Color::rgb(220, 234, 221), Fixed::from_int(2));
    painter.circle(first, Fixed::from_int(2), ACCENT);
    painter.circle(second, Fixed::from_int(2), ACCENT);
}

/// A fixed mirror: a square, riveted plate with a plain steel line.
fn paint_fixed_mirror(
    painter: &mut PlayPainter<'_, '_>,
    center: Point,
    orientation: MirrorOrientation,
) {
    painter.fill(
        square(center, 15),
        Color::rgb(36, 45, 48),
        Fixed::from_int(2),
    );
    painter.border(
        square(center, 15),
        Color::rgb(76, 88, 89),
        Fixed::ONE,
        Fixed::from_int(2),
    );
    for (dx, dy) in [(-11, -11), (11, -11), (-11, 11), (11, 11)] {
        painter.circle(
            Point::new(
                center.x + Fixed::from_int(dx),
                center.y + Fixed::from_int(dy),
            ),
            Fixed::from_ratio(3, 2),
            Color::rgb(112, 124, 120),
        );
    }
    let (first, second) = mirror_ends(center, orientation, 10);
    painter.line(first, second, Color::rgb(170, 182, 177), Fixed::from_int(3));
}

fn paint_receiver(painter: &mut PlayPainter<'_, '_>, center: Point, status: BeamStatus) {
    let ring = match status {
        BeamStatus::Lit => SUCCESS,
        BeamStatus::LampsDark => HINT,
        _ => Color::rgb(161, 195, 176),
    };
    painter.circle(center, Fixed::from_int(12), ring);
    painter.circle(center, Fixed::from_int(10), Color::rgb(26, 41, 39));
    if status != BeamStatus::Lit {
        return;
    }
    painter.circle(center, Fixed::from_int(6), SUCCESS);
    painter.line(
        Point::new(center.x - Fixed::from_int(4), center.y),
        Point::new(center.x - Fixed::ONE, center.y + Fixed::from_int(3)),
        BACKGROUND,
        Fixed::from_int(2),
    );
    painter.line(
        Point::new(center.x - Fixed::ONE, center.y + Fixed::from_int(3)),
        Point::new(center.x + Fixed::from_int(5), center.y - Fixed::from_int(4)),
        BACKGROUND,
        Fixed::from_int(2),
    );
}

fn paint_board(painter: &mut PlayPainter<'_, '_>, model: &LumenModel) {
    let level = model.level();
    painter.fill(Rect::new(0, 0, 282, 204), BOARD, Fixed::from_int(9));
    painter.border(
        Rect::new(0, 0, 282, 204),
        Color::rgb(54, 68, 67),
        Fixed::ONE,
        Fixed::from_int(9),
    );
    for row in 0..5 {
        for column in 0..7 {
            let center = Point::new(27 + column * 38, 26 + row * 38);
            painter.fill(square(center, 17), TILE, Fixed::from_int(5));
            painter.circle(center, Fixed::ONE, Color::rgb(82, 96, 91));
        }
    }

    paint_trace(painter, model.trace());
    if model.scan() {
        for offset in [0_u16, 21_845, 43_690] {
            if let Some(point) = scan_point(model.trace(), model.scan_phase().wrapping_add(offset))
            {
                painter.circle(point, Fixed::from_int(2), LIGHT);
            }
        }
    }

    for wall in level.walls {
        paint_wall(painter, cell_center(*wall));
    }
    for (index, lamp) in level.lamps.iter().enumerate() {
        let lit = model.trace().lit_lamps & (1 << index) != 0;
        paint_lamp(painter, cell_center(*lamp), lit);
    }
    for (index, mirror) in level.mirrors.iter().enumerate() {
        paint_mirror(
            painter,
            cell_center(mirror.position),
            model.orientations()[index],
            model.selected() == Some(index),
            model.hint() == Some(index),
        );
    }
    for mirror in level.fixed {
        paint_fixed_mirror(painter, cell_center(mirror.position), mirror.orientation);
    }
    paint_receiver(painter, cell_center(level.target), model.beam_status());

    let source_y = cell_center(GridPoint {
        x: 0,
        y: level.source_row,
    })
    .y;
    painter.line(
        Point::new(2, source_y - Fixed::from_int(5)),
        Point::new(8, source_y),
        ACCENT,
        Fixed::from_int(2),
    );
    painter.line(
        Point::new(8, source_y),
        Point::new(2, source_y + Fixed::from_int(5)),
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
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 282, 204);
    let mut painter = PlayPainter::new(paint, transform);
    paint_board(&mut painter, model);
}

pub(super) fn board_tap(board: &LumenBoard, rect: Rect, x: Fixed, y: Fixed) -> bool {
    if rect.w.is_zero() || rect.h.is_zero() {
        return false;
    }
    let local_x = (x - rect.x) * Fixed::from_int(282) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(204) / rect.h;
    let column = ((local_x.to_int() - 8) / 38) as i8;
    let row = ((local_y.to_int() - 7) / 38) as i8;
    if !(0..7).contains(&column) || !(0..5).contains(&row) {
        return false;
    }
    board.model.rotate_cell(column, row);
    true
}
