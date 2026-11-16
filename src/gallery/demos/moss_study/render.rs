use super::state::{MossModalSurface, MossSurface};
use super::style::{ACTIVE, BACKGROUND, BOARD, HEADER};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::moss::{GRID_HEIGHT, GRID_WIDTH, MossCells, MossModal, MossModel};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::{Color, Fixed, Point, Rect};

/// The top-left cell of the garden and the distance between cells.
pub(super) const CELL_ORIGIN: (i32, i32) = (10, 38);
pub(super) const CELL_PITCH: i32 = 23;
const CELL_SIZE: i32 = 21;
/// Where the seed cards sit in their dialog.
pub(super) const SEED_CARD_TOP: i32 = 88;
pub(super) const SEED_CARD_LEFT: [i32; 3] = [29, 171, 313];

fn paint_cells(painter: &mut PlayPainter<'_, '_>, cells: &MossCells, previous: &MossCells) {
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let age = cells.get(x, y);
            let old = previous.get(x, y);
            let left = CELL_ORIGIN.0 + i32::from(x) * CELL_PITCH;
            let top = CELL_ORIGIN.1 + i32::from(y) * CELL_PITCH;
            let fill = if age == 0 {
                Color::rgb(27, 44, 33)
            } else if age == 1 {
                Color::rgb(208, 236, 157)
            } else if age < 5 {
                Color::rgb(157, 207, 132)
            } else {
                Color::rgb(120, 171, 112)
            };
            painter.fill(
                Rect::new(left, top, CELL_SIZE, CELL_SIZE),
                fill,
                Fixed::from_int(4),
            );
            if age != 0 {
                painter.line(
                    Point::new(left + 6, top + 15),
                    Point::new(left + 14, top + 7),
                    if age == 1 {
                        Color::rgb(138, 184, 101)
                    } else {
                        Color::rgb(84, 137, 77)
                    },
                    Fixed::from_ratio(3, 2),
                );
                painter.circle(
                    Point::new(left + 7, top + 6),
                    Fixed::from_ratio(3, 2),
                    if age == 1 {
                        Color::rgb(237, 245, 205)
                    } else {
                        Color::rgb(182, 220, 150)
                    },
                );
            } else if old != 0 {
                painter.fill(
                    Rect::new(left + 7, top + 7, 7, 7),
                    Color::rgb(67, 88, 65),
                    Fixed::from_int(2),
                );
            } else {
                painter.fill(
                    Rect::new(left + 10, top + 10, 1, 1),
                    Color::rgb(64, 80, 62),
                    Fixed::ZERO,
                );
            }
        }
    }
}

fn paint_moss_surface(painter: &mut PlayPainter<'_, '_>, model: &MossModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 32), HEADER, Fixed::ZERO);
    painter.line(
        Point::new(12, 32),
        Point::new(468, 32),
        Color::rgb(57, 73, 58),
        Fixed::ONE,
    );
    painter.line(
        Point::new(18, 24),
        Point::new(24, 14),
        ACTIVE,
        Fixed::from_int(2),
    );
    painter.circle(Point::new(17, 16), Fixed::from_int(3), ACTIVE);
    painter.circle(Point::new(23, 12), Fixed::from_int(2), ACTIVE);
    let frame = Rect::new(
        CELL_ORIGIN.0 - 2,
        CELL_ORIGIN.1 - 2,
        CELL_PITCH * i32::from(GRID_WIDTH) + 4,
        CELL_PITCH * i32::from(GRID_HEIGHT) + 4,
    );
    painter.fill(frame, BOARD, Fixed::from_int(7));
    painter.border(
        frame,
        Color::rgb(60, 81, 60),
        Fixed::ONE,
        Fixed::from_int(7),
    );
    paint_cells(painter, model.cells(), model.previous());
}

#[crate::view(component = MossSurface, name = "MossSurface", priority = 60)]
pub(super) fn surface_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    paint_moss_surface(&mut painter, model);
}

/// The panel a dialog sits on: the seeds need more room than a question.
pub(super) fn modal_frame(modal: MossModal) -> Rect {
    if modal == MossModal::Seeds {
        Rect::new(17, 40, 446, 258)
    } else {
        Rect::new(17, 65, 446, 206)
    }
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
    name = "MossModalSurface",
    priority = 70
)]
pub(super) fn modal_render(model: model!(component.model, watch(modal, seed_id)), paint: paint!()) {
    if model.modal() == MossModal::None {
        return;
    }
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(8, 13, 9, 224),
        Fixed::ZERO,
    );
    let frame = modal_frame(model.modal());
    painter.fill(frame, Color::rgb(32, 50, 37), Fixed::from_int(10));
    painter.border(
        frame,
        Color::rgb(72, 96, 67),
        Fixed::ONE,
        Fixed::from_int(10),
    );
    if model.modal() == MossModal::Seeds {
        for (seed_id, left) in SEED_CARD_LEFT.into_iter().enumerate() {
            let card = Rect::new(left, SEED_CARD_TOP, 135, 96);
            painter.fill(card, Color::rgb(32, 50, 37), Fixed::from_int(7));
            painter.border(
                card,
                if usize::from(model.seed_id()) == seed_id {
                    ACTIVE
                } else {
                    Color::rgb(72, 96, 67)
                },
                Fixed::ONE,
                Fixed::from_int(7),
            );
            let cells = MossCells::seed(seed_id as u8).expect("built-in seed");
            paint_seed_preview(&mut painter, &cells, left + 8, SEED_CARD_TOP + 12);
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
