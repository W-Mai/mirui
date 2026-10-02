use super::style::{BOARD_HEIGHT, BOARD_WIDTH, color, mix};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::marble::{MarbleModel, Page, Theme, Vec2};
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::types::{Fixed64, Transform};
use crate::ui::ComputedRect;

const BOARD_MODEL_Y_ORIGIN: i32 = 52;

#[crate::component]
pub(super) struct MarbleBoard {
    pub(super) model: model!(MarbleModel),
}

fn point(value: Vec2, y_offset: i32) -> Point {
    Point {
        x: value.x.to_fixed(),
        y: value.y.to_fixed() - Fixed::from_int(y_offset),
    }
}

pub(super) fn paint_play_board(
    painter: &mut PlayPainter<'_, '_>,
    model: &MarbleModel,
    theme: Theme,
) {
    let board = Rect::new(10, 0, 460, 199);
    painter.fill(board, color(theme.bg), Fixed::from_int(10));
    painter.border(
        board,
        mix(theme.bg, 0x90a679, Fixed::from_ratio(15, 100)),
        Fixed::ONE,
        Fixed::from_int(10),
    );
    for slot in 0..model.ball_count() {
        painter.circle(
            Point::new(18 + slot as i32 * 7, 10),
            Fixed::from_int(2),
            color(theme.accent).scale_alpha(150),
        );
    }
    for x in (23..463).step_by(16) {
        for y in (62..247).step_by(16) {
            painter.circle(
                Point::new(x, y - BOARD_MODEL_Y_ORIGIN),
                Fixed::from_ratio(1, 2),
                mix(
                    theme.bg,
                    0x93a278,
                    if model.page == Page::Edit {
                        Fixed::from_ratio(25, 100)
                    } else {
                        Fixed::from_ratio(13, 100)
                    },
                ),
            );
        }
    }
    paint_gravity_direction(painter, model, theme);
    paint_transport(painter, model, theme);
    for (ax, ay, bx, by) in [
        (36, 145, 78, 165),
        (215, 80, 264, 70),
        (269, 213, 308, 234),
        (404, 144, 437, 122),
    ] {
        painter.line(
            Point::new(ax, ay - BOARD_MODEL_Y_ORIGIN + 2),
            Point::new(bx, by - BOARD_MODEL_Y_ORIGIN + 2),
            mix(theme.bg, 0x05110a, Fixed::from_ratio(65, 100)),
            Fixed::from_int(6),
        );
        painter.line(
            Point::new(ax, ay - BOARD_MODEL_Y_ORIGIN),
            Point::new(bx, by - BOARD_MODEL_Y_ORIGIN),
            Color::rgb(110, 131, 94),
            Fixed::from_int(5),
        );
        painter.line(
            Point::new(ax, ay - BOARD_MODEL_Y_ORIGIN),
            Point::new(bx, by - BOARD_MODEL_Y_ORIGIN),
            Color::rgb(189, 204, 164),
            Fixed::from_int(2),
        );
    }
    painter.fill(
        Rect::new(30, 194, 420, 2),
        mix(
            theme.bg,
            theme.accent,
            Fixed::from_ratio(22, 100) + model.flash.to_fixed() * Fixed::from_ratio(4, 10),
        ),
        Fixed::ONE,
    );
    for ring in model.rings.iter().flatten() {
        painter.arc(
            point(ring.pos, BOARD_MODEL_Y_ORIGIN),
            ring.radius.to_fixed(),
            Fixed::ZERO,
            Fixed::from_int(360),
            color(ring.color).scale_alpha(110),
            Fixed::ONE,
        );
    }
    for (slot, pad) in model.pads.iter().enumerate() {
        let Some(pad) = pad else { continue };
        let pulse = if model.feedback {
            Fixed64::ONE + pad.pulse * Fixed64::from_ratio(75, 1_000)
        } else {
            Fixed64::ONE
        };
        let radius = (pad.radius * pulse).to_fixed();
        let center = point(pad.pos, BOARD_MODEL_Y_ORIGIN);
        painter.circle(
            Point {
                y: center.y + Fixed::from_int(2),
                ..center
            },
            radius,
            mix(theme.bg, 0x05110a, Fixed::from_ratio(65, 100)),
        );
        painter.circle(
            center,
            radius,
            mix(theme.bg, pad.color, Fixed::from_ratio(65, 100)),
        );
        painter.circle(
            center,
            radius - Fixed::ONE,
            mix(theme.bg, pad.color, Fixed::from_ratio(12, 100)),
        );
        painter.arc(
            center,
            radius - Fixed::from_int(4),
            Fixed::ZERO,
            Fixed::from_int(360),
            color(pad.color).scale_alpha(80),
            Fixed::ONE,
        );
        painter.arc(
            center,
            radius - Fixed::ONE,
            Fixed::from_int(204),
            Fixed::from_int(281),
            color(pad.color),
            Fixed::ONE,
        );
        if slot == model.selected {
            let id_width = 4 + model.selected_pad().id.min(12) as i32;
            painter.fill(
                Rect::new(
                    center.x - Fixed::from_int(id_width / 2),
                    center.y - Fixed::ONE,
                    id_width,
                    2,
                ),
                color(pad.color),
                Fixed::ONE,
            );
            painter.arc(
                center,
                radius + Fixed::from_int(4),
                Fixed::from_int(340),
                Fixed::from_int(380),
                color(pad.color).scale_alpha(180),
                Fixed::ONE,
            );
            painter.arc(
                center,
                radius + Fixed::from_int(4),
                Fixed::from_int(160),
                Fixed::from_int(200),
                color(pad.color).scale_alpha(180),
                Fixed::ONE,
            );
        }
    }
    for ball in model.balls.iter().flatten() {
        if model.trails {
            for index in 1..ball.trail_len {
                painter.line(
                    point(ball.trail[index - 1], BOARD_MODEL_Y_ORIGIN),
                    point(ball.trail[index], BOARD_MODEL_Y_ORIGIN),
                    color(ball.color).scale_alpha((24 + index * 27) as u8),
                    Fixed::from_ratio(5 + index as i32, 4),
                );
            }
        }
        let center = point(ball.pos, BOARD_MODEL_Y_ORIGIN);
        painter.circle(center, Fixed::from_int(4), color(ball.color));
        painter.circle(
            Point {
                x: center.x - Fixed::ONE,
                y: center.y - Fixed::ONE,
            },
            Fixed::ONE,
            Color::rgb(251, 255, 239),
        );
    }
    for particle in model.particles.iter().flatten() {
        painter.circle(
            point(particle.pos, BOARD_MODEL_Y_ORIGIN),
            Fixed::ONE,
            color(particle.color).scale_alpha(170),
        );
    }
}

fn paint_transport(painter: &mut PlayPainter<'_, '_>, model: &MarbleModel, theme: Theme) {
    let active = model.transport_beat();
    for beat in 0..16 {
        let emphasized = beat % 4 == 0;
        let color = if beat == active {
            if model.recording {
                Color::rgb(238, 172, 139)
            } else {
                color(theme.accent)
            }
        } else if model.looping {
            color(theme.accent).scale_alpha(if emphasized { 90 } else { 55 })
        } else {
            mix(
                theme.bg,
                theme.accent,
                Fixed::from_ratio(if emphasized { 30 } else { 16 }, 100),
            )
        };
        painter.circle(
            Point::new(170 + beat as i32 * 7, 10),
            if beat == active {
                Fixed::from_ratio(3, 2)
            } else {
                Fixed::from_ratio(3, 4)
            },
            color,
        );
    }
}

fn paint_gravity_direction(painter: &mut PlayPainter<'_, '_>, model: &MarbleModel, theme: Theme) {
    let center = Point::new(435, 18);
    painter.circle(
        center,
        Fixed::from_int(15),
        mix(theme.bg, theme.panel, Fixed::from_ratio(55, 100)),
    );
    painter.arc(
        center,
        Fixed::from_int(15),
        Fixed::ZERO,
        Fixed::from_int(360),
        mix(theme.bg, theme.accent, Fixed::from_ratio(35, 100)),
        Fixed::ONE,
    );
    let target = Vec2 {
        x: model.target.x * Fixed64::from_int(190),
        y: model.gravity * Fixed64::from_int(150) + model.target.y * Fixed64::from_int(200),
    };
    let actual = Vec2 {
        x: model.tilt.x * Fixed64::from_int(190),
        y: model.gravity * Fixed64::from_int(150) + model.tilt.y * Fixed64::from_int(200),
    };
    paint_gravity_arrow(
        painter,
        center,
        target,
        Fixed::from_int(12),
        color(theme.accent).scale_alpha(110),
        Fixed::ONE,
        false,
    );
    paint_gravity_arrow(
        painter,
        center,
        actual,
        Fixed::from_int(9),
        color(theme.accent),
        Fixed::from_ratio(3, 2),
        true,
    );
    painter.circle(center, Fixed::from_int(2), color(theme.accent));
}

fn paint_gravity_arrow(
    painter: &mut PlayPainter<'_, '_>,
    center: Point,
    vector: Vec2,
    length: Fixed,
    color: Color,
    width: Fixed,
    arrowhead: bool,
) {
    let magnitude = (vector.x * vector.x + vector.y * vector.y).sqrt();
    if magnitude <= Fixed64::from_ratio(1, 1_000) {
        return;
    }
    let dx = (vector.x / magnitude).to_fixed();
    let dy = (vector.y / magnitude).to_fixed();
    let end = Point {
        x: center.x + dx * length,
        y: center.y + dy * length,
    };
    painter.line(center, end, color, width);
    if arrowhead {
        let back = Fixed::from_int(3);
        let side = Fixed::from_int(2);
        let base = Point {
            x: end.x - dx * back,
            y: end.y - dy * back,
        };
        painter.line(
            end,
            Point {
                x: base.x - dy * side,
                y: base.y + dx * side,
            },
            color,
            width,
        );
        painter.line(
            end,
            Point {
                x: base.x + dy * side,
                y: base.y - dx * side,
            },
            color,
            width,
        );
    } else {
        painter.circle(end, Fixed::from_ratio(3, 2), color);
    }
}

#[crate::view(component = MarbleBoard, name = "MarbleBoard", priority = 60)]
pub(super) fn board_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), BOARD_WIDTH, BOARD_HEIGHT);
    let mut painter = PlayPainter::new(paint, transform);
    paint_play_board(&mut painter, model, model.theme());
}

pub(super) fn event_point(rect: Rect, x: Fixed, y: Fixed) -> Option<Vec2> {
    let fit = fit_logical_canvas(rect, Transform::IDENTITY, BOARD_WIDTH, BOARD_HEIGHT);
    if fit.m00 <= Fixed::ZERO {
        return None;
    }
    let local_x = (x - fit.tx) / fit.m00;
    let local_y = (y - fit.ty) / fit.m11 + Fixed::from_int(BOARD_MODEL_Y_ORIGIN);
    Some(Vec2 {
        x: Fixed64::from_fixed(local_x),
        y: Fixed64::from_fixed(local_y),
    })
}

pub(super) fn board_tap(board: &MarbleBoard, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(point) = event_point(rect, x, y) else {
        return false;
    };
    board.model.tap_board(point);
    true
}

pub(super) fn board_drag_start(board: &MarbleBoard, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(point) = event_point(rect, x, y) else {
        return false;
    };
    board.model.begin_board_drag(point);
    true
}

pub(super) fn board_drag_move(board: &MarbleBoard, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(point) = event_point(rect, x, y) else {
        return false;
    };
    board.model.move_board_drag(point);
    true
}

pub(super) fn board_drag_end(board: &MarbleBoard, cancelled: bool) -> bool {
    board.model.end_board_drag(cancelled);
    true
}

#[compose]
pub(super) fn marble_play_page(model: model!(MarbleModel)) -> Entity {
    ui! {
        View (
            id: "marble_play_board",
            width: BOARD_WIDTH,
            height: BOARD_HEIGHT,
            clip_children: true
        ) [
            MarbleBoard {
                model: model.clone(),
            },
            TouchAction::None,
        ] on Tap {
            let Some(board) = com!(target, MarbleBoard) else {
                return false;
            };
            let Some(rect) = com!(target, ComputedRect).map(|rect| rect.0) else {
                return false;
            };
            board_tap(board, rect, x, y)
        } on DragStart {
            let Some(board) = com!(target, MarbleBoard) else {
                return false;
            };
            let Some(rect) = com!(target, ComputedRect).map(|rect| rect.0) else {
                return false;
            };
            board_drag_start(board, rect, x, y)
        } on DragMove {
            let Some(board) = com!(target, MarbleBoard) else {
                return false;
            };
            let Some(rect) = com!(target, ComputedRect).map(|rect| rect.0) else {
                return false;
            };
            board_drag_move(board, rect, x, y)
        } on DragEnd {
            let Some(board) = com!(target, MarbleBoard) else {
                return false;
            };
            board_drag_end(board, false)
        } on DragCancel {
            let Some(board) = com!(target, MarbleBoard) else {
                return false;
            };
            board_drag_end(board, true)
        }
    }
}
