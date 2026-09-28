use super::state::{TideModalSurface, TideSurface};
use super::style::{ACCENT, BACKGROUND, BOARD, HEADER, LINE, PANEL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::tidal::{BOARD_SIZE, TideModal, TideModel, Tile};
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};

fn tile_color(tile: Tile) -> Color {
    match tile {
        Tile::Sea => BOARD,
        Tile::Grove => Color::rgb(120, 165, 140),
        Tile::Field => Color::rgb(214, 182, 110),
        Tile::Hamlet => Color::rgb(204, 135, 115),
        Tile::Harbor => Color::rgb(115, 165, 181),
        Tile::Lens => Color::rgb(169, 154, 195),
        Tile::Lagoon => Color::rgb(95, 147, 155),
        Tile::Beacon => Color::rgb(210, 203, 166),
        Tile::Dike => Color::rgb(149, 157, 154),
    }
}

fn paint_tile(painter: &mut PlayPainter<'_, '_>, tile: Tile, center: Point) {
    let color = tile_color(tile);
    let x = center.x.to_int();
    let y = center.y.to_int();
    match tile {
        Tile::Sea => {}
        Tile::Grove => {
            painter.line(
                Point::new(x, y + 8),
                Point::new(x, y - 2),
                color,
                Fixed::ONE,
            );
            painter.fill(Rect::new(x - 7, y - 6, 14, 11), color, Fixed::from_int(7));
        }
        Tile::Field => {
            for offset in [-6, -1, 4] {
                painter.line(
                    Point::new(x - 8 + offset / 2, y + 7),
                    Point::new(x + offset, y - 7),
                    color,
                    Fixed::ONE,
                );
            }
            painter.line(
                Point::new(x - 9, y + 8),
                Point::new(x + 9, y + 8),
                color,
                Fixed::ONE,
            );
        }
        Tile::Hamlet => {
            painter.fill(Rect::new(x - 7, y - 1, 14, 11), color, Fixed::ONE);
            painter.line(
                Point::new(x - 9, y - 1),
                Point::new(x, y - 10),
                color,
                Fixed::from_int(2),
            );
            painter.line(
                Point::new(x, y - 10),
                Point::new(x + 9, y - 1),
                color,
                Fixed::from_int(2),
            );
        }
        Tile::Harbor => {
            painter.line(
                Point::new(x - 9, y + 5),
                Point::new(x + 9, y + 5),
                color,
                Fixed::from_int(2),
            );
            painter.line(
                Point::new(x, y + 5),
                Point::new(x, y - 10),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x, y - 9),
                Point::new(x + 8, y - 1),
                color,
                Fixed::ONE,
            );
        }
        Tile::Lens => {
            painter.arc(
                center,
                Fixed::from_int(9),
                Fixed::ZERO,
                Fixed::from_int(360),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x - 7, y + 7),
                Point::new(x + 7, y - 7),
                color,
                Fixed::ONE,
            );
            painter.circle(center, Fixed::from_int(2), color);
        }
        Tile::Lagoon => {
            for offset in [-6, 0, 6] {
                painter.line(
                    Point::new(x - 9, y + offset),
                    Point::new(x - 3, y + offset - 2),
                    color,
                    Fixed::ONE,
                );
                painter.line(
                    Point::new(x - 3, y + offset - 2),
                    Point::new(x + 3, y + offset + 2),
                    color,
                    Fixed::ONE,
                );
                painter.line(
                    Point::new(x + 3, y + offset + 2),
                    Point::new(x + 9, y + offset),
                    color,
                    Fixed::ONE,
                );
            }
        }
        Tile::Beacon => {
            painter.fill(Rect::new(x - 4, y - 5, 8, 14), color, Fixed::ONE);
            painter.fill(Rect::new(x - 7, y - 10, 14, 5), color, Fixed::from_int(2));
        }
        Tile::Dike => {
            for row in 0..3 {
                painter.line(
                    Point::new(x - 9 + row * 2, y - 7 + row * 6),
                    Point::new(x + 8, y - 7 + row * 6),
                    color,
                    Fixed::from_int(2),
                );
            }
        }
    }
}

fn paint_surface(painter: &mut PlayPainter<'_, '_>, model: &TideModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(Point::new(14, 34), Point::new(466, 34), LINE, Fixed::ONE);
    painter.fill(Rect::new(237, 60, 229, 51), PANEL, Fixed::from_int(4));
    painter.border(
        Rect::new(237, 60, 229, 51),
        LINE,
        Fixed::ONE,
        Fixed::from_int(4),
    );
    let forecast = model.forecast();
    for index in 0..BOARD_SIZE {
        let x = 14 + (index % 6) as i32 * 35;
        let y = 60 + (index / 6) as i32 * 35;
        let tile = model.tile(index);
        let legal = model.valid(index) && !model.settled() && !model.complete();
        painter.fill(
            Rect::new(x, y, 32, 32),
            if tile == Tile::Sea {
                if model.terrain(index) == 0 {
                    Color::rgb(32, 74, 84)
                } else {
                    Color::rgb(28, 64, 73)
                }
            } else {
                Color::rgb(51, 86, 89)
            },
            Fixed::from_int(3),
        );
        painter.border(
            Rect::new(x, y, 32, 32),
            if legal {
                Color::rgb(82, 119, 117)
            } else {
                LINE
            },
            Fixed::ONE,
            Fixed::from_int(3),
        );
        if tile != Tile::Sea {
            paint_tile(painter, tile, Point::new(x + 16, y + 16));
            if matches!(tile, Tile::Field | Tile::Hamlet) && model.tile_score(index, forecast) == 0
            {
                painter.line(
                    Point::new(x + 4, y + 27),
                    Point::new(x + 28, y + 27),
                    Color::rgb(142, 186, 202),
                    Fixed::from_int(2),
                );
            }
        } else if model.terrain(index) == 2 {
            painter.line(
                Point::new(x + 11, y + 21),
                Point::new(x + 17, y + 11),
                Color::rgb(82, 115, 112),
                Fixed::ONE,
            );
            painter.line(
                Point::new(x + 17, y + 11),
                Point::new(x + 23, y + 21),
                Color::rgb(82, 115, 112),
                Fixed::ONE,
            );
        }
        for mark in 0..=model.terrain(index) {
            painter.fill(
                Rect::new(x + 3 + i32::from(mark) * 4, y + 3, 2, 2),
                Color::rgb(165, 181, 160),
                Fixed::ZERO,
            );
        }
        if model.selected() == Some(index as u8) || model.pending() == Some(index as u8) {
            painter.border(
                Rect::new(x + 1, y + 1, 30, 30),
                ACCENT,
                Fixed::from_int(2),
                Fixed::from_int(3),
            );
            if tile == Tile::Sea {
                paint_tile(
                    painter,
                    model.offer(usize::from(model.choice())),
                    Point::new(x + 16, y + 16),
                );
            }
        }
    }
    painter.line(Point::new(14, 296), Point::new(466, 296), LINE, Fixed::ONE);
}

#[crate::view(
    component = TideSurface,
    read(model),
    watch(model.visual_revision()),
    name = "TideSurface",
    priority = 60
)]
pub(super) fn surface_render(
    renderer: &mut dyn Renderer,
    model: &TideModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_surface(&mut painter, model);
}

#[crate::view(
    component = TideModalSurface,
    read(model),
    watch(model.modal()),
    name = "TideModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    renderer: &mut dyn Renderer,
    model: &TideModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if model.modal() == TideModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 34, 480, 264),
        Color::rgba(16, 38, 45, 236),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(20, 43, 440, 247), PANEL, Fixed::from_int(7));
    painter.border(
        Rect::new(20, 43, 440, 247),
        LINE,
        Fixed::ONE,
        Fixed::from_int(7),
    );
    painter.line(Point::new(35, 96), Point::new(445, 96), LINE, Fixed::ONE);
}

pub(super) fn surface_view() -> View {
    surface_render::view()
}

pub(super) fn modal_view() -> View {
    modal_render::view()
}
