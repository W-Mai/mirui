use super::geometry::board_geometry;
use super::state::PictureSurface;
use super::style::{APRICOT, BG, CELL, GRID, LAVENDER, MINT, MUTED, PANEL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::picture::{PictureCell, PictureModel};
use crate::prelude::{Color, Entity, Fixed, Point, Rect, World};
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};

fn paint_board(painter: &mut PlayPainter<'_, '_>, model: &PictureModel) {
    let level = model.level();
    let size = level.size();
    let geometry = board_geometry(model);
    let cell_size = geometry.cell;
    painter.fill(
        Rect::new(
            geometry.x - 6,
            geometry.y - 6,
            geometry.size + 12,
            geometry.size + 12,
        ),
        PANEL,
        Fixed::from_int(7),
    );
    painter.border(
        Rect::new(
            geometry.x - 6,
            geometry.y - 6,
            geometry.size + 12,
            geometry.size + 12,
        ),
        GRID,
        Fixed::ONE,
        Fixed::from_int(7),
    );
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
        if model.cursor() == cell {
            painter.border(
                Rect::new(x - 1, y - 1, cell_size + 1, cell_size + 1),
                LAVENDER,
                Fixed::from_int(2),
                Fixed::from_int(2),
            );
        }
    }
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<PictureModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(Rect::new(0, 0, 480, 320), BG, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 39), PANEL, Fixed::ZERO);
    painter.line(Point::new(14, 39), Point::new(466, 39), GRID, Fixed::ONE);
    paint_board(&mut painter, model);
    painter.fill(Rect::new(324, 53, 142, 202), PANEL, Fixed::from_int(9));
    painter.border(
        Rect::new(324, 53, 142, 202),
        GRID,
        Fixed::ONE,
        Fixed::from_int(9),
    );
}

pub(super) fn surface_view() -> View {
    View::new("PictureSurface", 60, surface_render).with_filter::<PictureSurface>()
}
