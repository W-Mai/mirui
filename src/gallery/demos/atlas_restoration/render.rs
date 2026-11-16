use super::geometry::board_geometry;
use super::state::PictureSurface;
use super::style::{APRICOT, BG, CELL, ERROR, GRID, LAVENDER, MINT, MUTED, PANEL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::picture::{PictureCell, PictureMessage, PictureModel};
use crate::prelude::{Color, Fixed, Point, Rect};

fn paint_board(painter: &mut PlayPainter<'_, '_>, model: &PictureModel) {
    let level = model.level();
    let size = level.size();
    let geometry = board_geometry(model);
    let cell_size = geometry.cell;
    let frame = Rect::new(
        geometry.x - 6,
        geometry.y - 6,
        geometry.size + 12,
        geometry.size + 12,
    );
    painter.fill(frame, PANEL, Fixed::from_int(7));
    painter.border(frame, GRID, Fixed::ONE, Fixed::from_int(7));
    let checked = matches!(model.message(), PictureMessage::Checked(errors) if errors != 0);
    for cell in 0..size * size {
        let x = geometry.x + i32::from(cell % size) * cell_size;
        let y = geometry.y + i32::from(cell / size) * cell_size;
        let inner = cell_size - 1;
        let value = model.cell(cell);
        let fill = match value {
            PictureCell::Unknown => CELL,
            PictureCell::Filled => MINT,
            PictureCell::EmptyMark => Color::rgb(26, 40, 53),
        };
        painter.fill(Rect::new(x, y, inner, inner), fill, Fixed::from_int(2));
        painter.border(
            Rect::new(x, y, inner, inner),
            GRID,
            Fixed::ONE,
            Fixed::from_int(2),
        );
        if value == PictureCell::EmptyMark {
            let inset = (cell_size / 3).max(3);
            painter.line(
                Point::new(x + inset, y + inset),
                Point::new(x + cell_size - inset, y + cell_size - inset),
                MUTED,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x + cell_size - inset, y + inset),
                Point::new(x + inset, y + cell_size - inset),
                MUTED,
                Fixed::ONE,
            );
        }
        if level.is_given(cell) {
            painter.border(
                Rect::new(x + 2, y + 2, cell_size - 5, cell_size - 5),
                APRICOT,
                Fixed::ONE,
                Fixed::from_int(2),
            );
        }
        // After a check, the wrong cells keep a red frame until the next stroke.
        let wrong =
            (value == PictureCell::Filled) != level.target(cell) && value != PictureCell::Unknown;
        if checked && wrong {
            painter.border(
                Rect::new(x + 1, y + 1, cell_size - 3, cell_size - 3),
                ERROR,
                Fixed::from_int(2),
                Fixed::from_int(2),
            );
        }
        if model.message() == PictureMessage::Hint(cell) {
            painter.border(
                Rect::new(x - 1, y - 1, cell_size + 1, cell_size + 1),
                APRICOT,
                Fixed::from_int(2),
                Fixed::from_int(3),
            );
        }
        if model.cursor() == cell {
            painter.border(
                Rect::new(x - 1, y - 1, cell_size + 1, cell_size + 1),
                LAVENDER,
                Fixed::from_int(2),
                Fixed::from_int(2),
            );
        }
    }
    // Every fifth line is heavier on the big boards, to help counting.
    if size >= 8 {
        let at = 5 * cell_size;
        let (left, top) = (geometry.x, geometry.y);
        let end = geometry.size + 1;
        painter.line(
            Point::new(left + at - 1, top - 2),
            Point::new(left + at - 1, top + end),
            MUTED,
            Fixed::from_int(2),
        );
        painter.line(
            Point::new(left - 2, top + at - 1),
            Point::new(left + end, top + at - 1),
            MUTED,
            Fixed::from_int(2),
        );
    }
}

#[crate::view(component = PictureSurface, name = "PictureSurface", priority = 60)]
pub(super) fn surface_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(Rect::new(0, 0, 480, 320), BG, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 28), PANEL, Fixed::ZERO);
    painter.line(Point::new(0, 28), Point::new(480, 28), GRID, Fixed::ONE);
    paint_board(&mut painter, model);
}
