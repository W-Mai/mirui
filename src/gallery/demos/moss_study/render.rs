use super::state::{MossModalSurface, MossSurface};
use super::style::{ACTIVE, BACKGROUND, BOARD, HEADER, PANEL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::moss::{GRID_HEIGHT, GRID_WIDTH, MossCells, MossModal, MossModel};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};

fn paint_cells(painter: &mut PlayPainter<'_, '_>, cells: &MossCells, previous: &MossCells) {
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let age = cells.get(x, y);
            let old = previous.get(x, y);
            let left = 17 + i32::from(x) * 17;
            let top = 67 + i32::from(y) * 17;
            let fill = if age == 0 {
                Color::rgb(27, 44, 33)
            } else if age == 1 {
                Color::rgb(208, 236, 157)
            } else if age < 5 {
                Color::rgb(157, 207, 132)
            } else {
                Color::rgb(120, 171, 112)
            };
            painter.fill(Rect::new(left, top, 15, 15), fill, Fixed::from_int(3));
            if age != 0 {
                painter.line(
                    Point::new(left + 4, top + 11),
                    Point::new(left + 10, top + 5),
                    if age == 1 {
                        Color::rgb(138, 184, 101)
                    } else {
                        Color::rgb(84, 137, 77)
                    },
                    Fixed::ONE,
                );
                painter.circle(
                    Point::new(left + 5, top + 4),
                    Fixed::ONE,
                    if age == 1 {
                        Color::rgb(237, 245, 205)
                    } else {
                        Color::rgb(182, 220, 150)
                    },
                );
            } else if old != 0 {
                painter.fill(
                    Rect::new(left + 5, top + 5, 5, 5),
                    Color::rgb(67, 88, 65),
                    Fixed::ONE,
                );
            } else {
                painter.fill(
                    Rect::new(left + 7, top + 7, 1, 1),
                    Color::rgb(64, 80, 62),
                    Fixed::ZERO,
                );
            }
        }
    }
}

fn paint_moss_surface(painter: &mut PlayPainter<'_, '_>, model: &MossModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(
        Point::new(12, 34),
        Point::new(468, 34),
        Color::rgb(57, 73, 58),
        Fixed::ONE,
    );
    painter.line(
        Point::new(18, 22),
        Point::new(24, 12),
        ACTIVE,
        Fixed::from_int(2),
    );
    painter.circle(Point::new(17, 14), Fixed::from_int(3), ACTIVE);
    painter.circle(Point::new(23, 10), Fixed::from_int(2), ACTIVE);
    painter.fill(Rect::new(12, 64, 350, 211), BOARD, Fixed::from_int(7));
    painter.border(
        Rect::new(12, 64, 350, 211),
        Color::rgb(60, 81, 60),
        Fixed::ONE,
        Fixed::from_int(7),
    );
    paint_cells(painter, model.cells(), model.previous());
    painter.fill(Rect::new(371, 42, 97, 116), PANEL, Fixed::from_int(8));
    painter.border(
        Rect::new(371, 42, 97, 116),
        Color::rgb(67, 86, 64),
        Fixed::ONE,
        Fixed::from_int(8),
    );
    painter.line(
        Point::new(382, 100),
        Point::new(457, 100),
        Color::rgb(64, 88, 60),
        Fixed::ONE,
    );
    painter.fill(
        Rect::new(382, 136, 74, 4),
        Color::rgb(58, 80, 53),
        Fixed::from_int(2),
    );
    let density = Fixed::from_int(74) * Fixed::from_ratio(i32::from(model.live_count()), 240);
    painter.fill(
        Rect {
            x: Fixed::from_int(382),
            y: Fixed::from_int(136),
            w: density,
            h: Fixed::from_int(4),
        },
        ACTIVE,
        Fixed::from_int(2),
    );
    painter.fill(
        Rect::new(0, 282, 480, 38),
        Color::rgb(24, 36, 27),
        Fixed::ZERO,
    );
}

#[crate::view(
    component = MossSurface,
    read(model),
    watch(model.visual_revision()),
    name = "MossSurface",
    priority = 60
)]
pub(super) fn surface_render(
    renderer: &mut dyn Renderer,
    model: &MossModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_moss_surface(&mut painter, model);
}

fn paint_seed_preview(
    painter: &mut PlayPainter<'_, '_>,
    cells: &MossCells,
    origin_x: i32,
    origin_y: i32,
) {
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            if cells.get(x, y) != 0 {
                painter.fill(
                    Rect::new(
                        origin_x + i32::from(x) * 6,
                        origin_y + i32::from(y) * 6,
                        5,
                        5,
                    ),
                    ACTIVE,
                    Fixed::ONE,
                );
            }
        }
    }
}

#[crate::view(
    component = MossModalSurface,
    read(model),
    watch(model.modal(), model.seed_id()),
    name = "MossModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    renderer: &mut dyn Renderer,
    model: &MossModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if model.modal() == MossModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(8, 13, 9, 224),
        Fixed::ZERO,
    );
    painter.fill(
        Rect::new(17, 65, 446, 206),
        Color::rgb(32, 50, 37),
        Fixed::from_int(10),
    );
    painter.border(
        Rect::new(17, 65, 446, 206),
        Color::rgb(72, 96, 67),
        Fixed::ONE,
        Fixed::from_int(10),
    );
    if model.modal() == MossModal::Seeds {
        for seed_id in 0..3_u8 {
            let left = 29 + i32::from(seed_id) * 142;
            painter.fill(
                Rect::new(left, 104, 135, 111),
                Color::rgb(32, 50, 37),
                Fixed::from_int(7),
            );
            painter.border(
                Rect::new(left, 104, 135, 111),
                if model.seed_id() == seed_id {
                    ACTIVE
                } else {
                    Color::rgb(72, 96, 67)
                },
                Fixed::ONE,
                Fixed::from_int(7),
            );
            let cells = MossCells::seed(seed_id).expect("built-in seed");
            paint_seed_preview(&mut painter, &cells, left + 8, 122);
        }
    } else {
        painter.fill(
            Rect::new(57, 116, 84, 84),
            Color::rgb(23, 37, 28),
            Fixed::from_int(42),
        );
        painter.line(
            Point::new(83, 177),
            Point::new(111, 135),
            Color::rgb(64, 91, 59),
            Fixed::from_int(4),
        );
        painter.circle(Point::new(83, 143), Fixed::from_int(12), ACTIVE);
        painter.circle(Point::new(111, 133), Fixed::from_int(9), ACTIVE);
    }
}

pub(super) fn surface_view() -> View {
    surface_render::view()
}

pub(super) fn modal_view() -> View {
    modal_render::view()
}
