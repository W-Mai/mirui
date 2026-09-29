use super::state::TwinSurface;
use super::style::{APRICOT, BG, FLOOR, GRID, LAVENDER, MINT, PANEL, WALL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::expeditions::TwinCell;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::twin::TwinModel;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};

fn paint_board(
    painter: &mut PlayPainter<'_, '_>,
    model: &TwinModel,
    station: usize,
    origin_x: i32,
    accent: Color,
) {
    let level = model.level();
    painter.fill(
        Rect::new(origin_x - 5, 54, 139, 151),
        PANEL,
        Fixed::from_int(8),
    );
    painter.border(
        Rect::new(origin_x - 5, 54, 139, 151),
        Color::rgb(34, 68, 80),
        Fixed::ONE,
        Fixed::from_int(8),
    );
    for cell in 0..36_u8 {
        let x = origin_x + i32::from(cell % 6) * 21;
        let y = 67 + i32::from(cell / 6) * 21;
        let fill = match level.cell(station, cell) {
            TwinCell::Floor => FLOOR,
            TwinCell::Wall => WALL,
            TwinCell::Door => {
                if model.has_key() {
                    FLOOR
                } else {
                    APRICOT
                }
            }
        };
        painter.fill(Rect::new(x, y, 19, 19), fill, Fixed::from_int(3));
        painter.border(
            Rect::new(x, y, 19, 19),
            GRID,
            Fixed::ONE,
            Fixed::from_int(3),
        );
        if level.goal(station) == cell {
            painter.border(
                Rect::new(x + 4, y + 4, 11, 11),
                accent,
                Fixed::ONE,
                Fixed::from_int(5),
            );
        }
        if level.key() == Some(cell) && !model.has_key() {
            painter.circle(Point::new(x + 9, y + 9), Fixed::from_int(3), APRICOT);
        }
    }
    let position = model.position(station);
    let x = origin_x + i32::from(position % 6) * 21 + 9;
    let y = 67 + i32::from(position / 6) * 21 + 9;
    painter.circle(Point::new(x, y), Fixed::from_int(7), accent);
    painter.circle(Point::new(x, y), Fixed::from_int(3), BG);
}

#[crate::view(
    component = TwinSurface,
    read(game),
    watch(game.visual_revision()),
    name = "TwinSurface",
    priority = 60
)]
pub(super) fn surface_render(
    renderer: &mut dyn Renderer,
    game: &TwinModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(Rect::new(0, 0, 480, 320), BG, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 39), PANEL, Fixed::ZERO);
    painter.line(Point::new(14, 39), Point::new(466, 39), GRID, Fixed::ONE);
    paint_board(&mut painter, game, 0, 18, MINT);
    paint_board(&mut painter, game, 1, 165, LAVENDER);
    painter.fill(Rect::new(316, 54, 150, 205), PANEL, Fixed::from_int(8));
    painter.border(
        Rect::new(316, 54, 150, 205),
        Color::rgb(34, 68, 80),
        Fixed::ONE,
        Fixed::from_int(8),
    );
    for y in [89, 128, 167, 206] {
        painter.line(Point::new(327, y), Point::new(455, y), GRID, Fixed::ONE);
    }
    painter.circle(Point::new(338, 73), Fixed::from_int(5), MINT);
    painter.circle(Point::new(355, 73), Fixed::from_int(5), LAVENDER);
}

pub(super) fn surface_view() -> View {
    surface_render::view()
}
