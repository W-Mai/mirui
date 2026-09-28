use super::runtime::LumenNodes;
use super::style::{ACCENT, BACKGROUND, BOARD, LIGHT, SUCCESS, TILE};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::lumen::{LumenModel, MirrorOrientation, Trace};
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::ComputedRect;
use crate::ui::view::{View, ViewCtx};

#[crate::component]
#[derive(Default)]
pub(super) struct LumenBoard;

pub(super) fn cell_center(point: crate::gallery::play::lumen::GridPoint) -> Point {
    Point::new(27 + i32::from(point.x) * 38, 26 + i32::from(point.y) * 38)
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

fn paint_board(painter: &mut PlayPainter<'_, '_>, model: &LumenModel) {
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
            painter.fill(
                Rect::new(
                    center.x - Fixed::from_int(17),
                    center.y - Fixed::from_int(17),
                    34,
                    34,
                ),
                TILE,
                Fixed::from_int(5),
            );
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

    for wall in model.level().walls {
        let center = cell_center(*wall);
        painter.fill(
            Rect::new(
                center.x - Fixed::from_int(13),
                center.y - Fixed::from_int(13),
                26,
                26,
            ),
            Color::rgb(59, 68, 68),
            Fixed::from_int(4),
        );
        painter.border(
            Rect::new(
                center.x - Fixed::from_int(13),
                center.y - Fixed::from_int(13),
                26,
                26,
            ),
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

    for (index, mirror) in model.level().mirrors.iter().enumerate() {
        let center = cell_center(mirror.position);
        let selected = model.selected() == index;
        let hinted = model.hint() == Some(index);
        painter.fill(
            Rect::new(
                center.x - Fixed::from_int(15),
                center.y - Fixed::from_int(15),
                30,
                30,
            ),
            if selected {
                Color::rgb(56, 68, 71)
            } else {
                Color::rgb(44, 57, 61)
            },
            Fixed::from_int(6),
        );
        painter.border(
            Rect::new(
                center.x - Fixed::from_int(15),
                center.y - Fixed::from_int(15),
                30,
                30,
            ),
            if hinted {
                Color::rgb(237, 178, 151)
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
        let slash = model.orientations()[index] == MirrorOrientation::Slash;
        let first = Point::new(
            center.x - Fixed::from_int(9),
            center.y + Fixed::from_int(if slash { 9 } else { -9 }),
        );
        let second = Point::new(
            center.x + Fixed::from_int(9),
            center.y + Fixed::from_int(if slash { -9 } else { 9 }),
        );
        painter.line(first, second, Color::rgb(220, 234, 221), Fixed::from_int(2));
        painter.circle(first, Fixed::from_int(2), ACCENT);
        painter.circle(second, Fixed::from_int(2), ACCENT);
    }

    let target = cell_center(model.level().target);
    painter.circle(target, Fixed::from_int(12), Color::rgb(161, 195, 176));
    painter.circle(target, Fixed::from_int(10), Color::rgb(26, 41, 39));
    painter.circle(
        target,
        Fixed::from_int(6),
        if model.trace().solved {
            SUCCESS
        } else {
            Color::rgb(26, 41, 39)
        },
    );
    if model.trace().solved {
        painter.line(
            Point::new(target.x - Fixed::from_int(4), target.y),
            Point::new(target.x - Fixed::ONE, target.y + Fixed::from_int(3)),
            BACKGROUND,
            Fixed::from_int(2),
        );
        painter.line(
            Point::new(target.x - Fixed::ONE, target.y + Fixed::from_int(3)),
            Point::new(target.x + Fixed::from_int(5), target.y - Fixed::from_int(4)),
            BACKGROUND,
            Fixed::from_int(2),
        );
    }

    let source_y = cell_center(crate::gallery::play::lumen::GridPoint {
        x: 0,
        y: model.level().source_row,
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

fn board_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<LumenModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 282, 204);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_board(&mut painter, model);
}

pub(super) fn board_view() -> View {
    View::new("LumenBoard", 60, board_render).with_filter::<LumenBoard>()
}

pub(super) fn board_tap(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(rect) = world.get::<ComputedRect>(entity).map(|rect| rect.0) else {
        return false;
    };
    if rect.w.is_zero() || rect.h.is_zero() {
        return false;
    }
    let local_x = (*x - rect.x) * Fixed::from_int(282) / rect.w;
    let local_y = (*y - rect.y) * Fixed::from_int(204) / rect.h;
    let column = ((local_x.to_int() - 8) / 38) as i8;
    let row = ((local_y.to_int() - 7) / 38) as i8;
    if !(0..7).contains(&column) || !(0..5).contains(&row) {
        return false;
    }
    LumenNodes::update(world, |model| model.rotate_cell(column, row));
    true
}
