use super::state::{TideModalSurface, TideSurface};
use super::style::{
    ACCENT, BACKGROUND, BOARD, BOARD_X, BOARD_Y, CELL, GAIN_TOP, HEADER, LAND, LINE, PANEL,
    PANEL_W, PANEL_X, PITCH, RACK_PITCH, RACK_Y, cell_origin,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::tidal::{BOARD_SIZE, TideModal, TideModel, Tile};
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::types::Transform;

/// Tile art is drawn for a 32 px cell; board and rack cells are 48 px.
const ART_SCALE: Fixed = Fixed::from_ratio(3, 2);
/// How far a placed tile falls before it lands, in pixels.
const DROP_HEIGHT: i32 = 18;
/// The pill behind a harvest score.
const GAIN_SHADE: Color = Color::rgb(16, 38, 45);

fn faded(color: Color, alpha: i32) -> Color {
    Color {
        a: alpha.clamp(0, 255) as u8,
        ..color
    }
}

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

/// Draws `tile`'s art around the painter's origin.
fn paint_art(painter: &mut PlayPainter<'_, '_>, tile: Tile) {
    let color = tile_color(tile);
    let point = Point::new;
    match tile {
        Tile::Sea => {}
        Tile::Grove => {
            painter.line(point(0, 8), point(0, -2), color, Fixed::ONE);
            painter.fill(Rect::new(-7, -6, 14, 11), color, Fixed::from_int(7));
        }
        Tile::Field => {
            for offset in [-6, -1, 4] {
                painter.line(
                    point(-8 + offset / 2, 7),
                    point(offset, -7),
                    color,
                    Fixed::ONE,
                );
            }
            painter.line(point(-9, 8), point(9, 8), color, Fixed::ONE);
        }
        Tile::Hamlet => {
            painter.fill(Rect::new(-7, -1, 14, 11), color, Fixed::ONE);
            painter.line(point(-9, -1), point(0, -10), color, Fixed::from_int(2));
            painter.line(point(0, -10), point(9, -1), color, Fixed::from_int(2));
        }
        Tile::Harbor => {
            painter.line(point(-9, 5), point(9, 5), color, Fixed::from_int(2));
            painter.line(point(0, 5), point(0, -10), color, Fixed::ONE);
            painter.line(point(0, -9), point(8, -1), color, Fixed::ONE);
        }
        Tile::Lens => {
            painter.arc(
                point(0, 0),
                Fixed::from_int(9),
                Fixed::ZERO,
                Fixed::from_int(360),
                color,
                Fixed::ONE,
            );
            painter.line(point(-7, 7), point(7, -7), color, Fixed::ONE);
            painter.circle(point(0, 0), Fixed::from_int(2), color);
        }
        Tile::Lagoon => {
            for offset in [-6, 0, 6] {
                painter.line(point(-9, offset), point(-3, offset - 2), color, Fixed::ONE);
                painter.line(
                    point(-3, offset - 2),
                    point(3, offset + 2),
                    color,
                    Fixed::ONE,
                );
                painter.line(point(3, offset + 2), point(9, offset), color, Fixed::ONE);
            }
        }
        Tile::Beacon => {
            painter.fill(Rect::new(-4, -5, 8, 14), color, Fixed::ONE);
            painter.fill(Rect::new(-7, -10, 14, 5), color, Fixed::from_int(2));
        }
        Tile::Dike => {
            for row in 0..3 {
                painter.line(
                    point(-9 + row * 2, -7 + row * 6),
                    point(8, -7 + row * 6),
                    color,
                    Fixed::from_int(2),
                );
            }
        }
    }
}

/// Draws `tile`'s art centred on `center`, raised by `lift` pixels.
fn paint_art_at(painter: &mut PlayPainter<'_, '_>, tile: Tile, center: Point, lift: Fixed) {
    let place = Transform::translate(center.x, center.y - lift);
    let mut art = painter.transformed(place.compose(&Transform::scale(ART_SCALE, ART_SCALE)));
    paint_art(&mut art, tile);
}

fn paint_cell(painter: &mut PlayPainter<'_, '_>, model: &TideModel, index: usize) {
    let (x, y) = cell_origin(index);
    let cell = Rect::new(x, y, CELL, CELL);
    let center = Point::new(x + CELL / 2, y + CELL / 2);
    let radius = Fixed::from_int(4);
    let tile = model.tile(index);
    let legal = model.valid(index) && !model.settled() && !model.complete();
    let fill = match (tile, model.terrain(index)) {
        (Tile::Sea, 0) => Color::rgb(32, 74, 84),
        (Tile::Sea, _) => Color::rgb(28, 64, 73),
        _ => LAND,
    };
    painter.fill(cell, fill, radius);
    painter.border(
        cell,
        if legal {
            Color::rgb(82, 119, 117)
        } else {
            LINE
        },
        Fixed::ONE,
        radius,
    );
    let drop = model
        .dropping()
        .filter(|(dropping, _)| *dropping == index)
        .map(|(_, fallen)| fallen);
    if tile != Tile::Sea {
        let lift = drop.map_or(Fixed::ZERO, |fallen| (Fixed::ONE - fallen) * DROP_HEIGHT);
        paint_art_at(painter, tile, center, lift);
        if matches!(tile, Tile::Field | Tile::Hamlet)
            && model.tile_score(index, model.forecast()) == 0
        {
            painter.line(
                Point::new(x + 6, y + 42),
                Point::new(x + 42, y + 42),
                Color::rgb(142, 186, 202),
                Fixed::from_int(2),
            );
        }
    } else if model.terrain(index) == 2 {
        let peak = Color::rgb(82, 115, 112);
        painter.line(
            Point::new(x + 16, y + 31),
            Point::new(x + 24, y + 17),
            peak,
            Fixed::ONE,
        );
        painter.line(
            Point::new(x + 24, y + 17),
            Point::new(x + 32, y + 31),
            peak,
            Fixed::ONE,
        );
    }
    for mark in 0..=i32::from(model.terrain(index)) {
        painter.fill(
            Rect::new(x + 4 + mark * 5, y + 4, 3, 3),
            Color::rgb(165, 181, 160),
            Fixed::ZERO,
        );
    }
    let marked = model.selected() == Some(index as u8) || model.pending() == Some(index as u8);
    if marked {
        let inner = Rect::new(x + 1, y + 1, CELL - 2, CELL - 2);
        painter.border(inner, ACCENT, Fixed::from_int(2), radius);
        if tile == Tile::Sea {
            let offer = model.offer(usize::from(model.choice()));
            paint_art_at(painter, offer, center, Fixed::ZERO);
        }
    }
    if let Some(fallen) = drop {
        // The cell rings as the tile lands.
        let alpha = ((Fixed::ONE - fallen.clamp(Fixed::ZERO, Fixed::ONE)) * 255).to_int();
        painter.border(
            Rect::new(x - 2, y - 2, CELL + 4, CELL + 4),
            faded(ACCENT, alpha),
            Fixed::from_int(2),
            Fixed::from_int(5),
        );
    }
    let glow = model.gain_glow();
    if model.gain(index) > 0 && glow > Fixed::ZERO {
        let alpha = (glow * 255).to_int();
        let inner = Rect::new(x + 1, y + 1, CELL - 2, CELL - 2);
        painter.border(inner, faded(ACCENT, alpha), Fixed::from_int(2), radius);
        let top = y + GAIN_TOP - i32::from(model.gain_lift());
        painter.fill(
            Rect::new(x + 9, top, CELL - 18, 14),
            faded(GAIN_SHADE, alpha * 7 / 8),
            radius,
        );
    }
}

/// The three tiles in hand; the chosen one stands raised.
fn paint_rack(painter: &mut PlayPainter<'_, '_>, model: &TideModel) {
    let open = !model.settled() && !model.complete();
    for slot in 0..3 {
        let chosen = open && usize::from(model.choice()) == slot;
        let lift = if chosen { 4 } else { 0 };
        let x = PANEL_X + slot as i32 * RACK_PITCH;
        let rect = Rect::new(x, RACK_Y - lift, CELL, CELL);
        let radius = Fixed::from_int(4);
        painter.fill(
            rect,
            if open { LAND } else { Color::rgb(29, 57, 63) },
            radius,
        );
        if chosen {
            painter.border(rect, ACCENT, Fixed::from_int(2), radius);
        } else {
            painter.border(rect, LINE, Fixed::ONE, radius);
        }
        if open {
            let center = Point::new(x + CELL / 2, RACK_Y - lift + CELL / 2);
            paint_art_at(painter, model.offer(slot), center, Fixed::ZERO);
        }
    }
}

fn paint_surface(painter: &mut PlayPainter<'_, '_>, model: &TideModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    let span = 5 * PITCH + CELL;
    painter.fill(
        Rect::new(BOARD_X - 4, BOARD_Y - 4, span + 8, span + 8),
        HEADER,
        Fixed::from_int(6),
    );
    for index in 0..BOARD_SIZE {
        paint_cell(painter, model, index);
    }
    let radius = Fixed::from_int(4);
    painter.fill(Rect::new(PANEL_X, 78, PANEL_W, 34), PANEL, radius);
    painter.border(
        Rect::new(PANEL_X, 78, PANEL_W, 34),
        LINE,
        Fixed::ONE,
        radius,
    );
    paint_rack(painter, model);
    let goal = model.goal();
    let done = i32::from(model.goal_progress().min(goal.count));
    painter.fill(Rect::new(PANEL_X, 282, PANEL_W, 3), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(
            PANEL_X,
            282,
            PANEL_W * done / i32::from(goal.count.max(1)),
            3,
        ),
        ACCENT,
        Fixed::ZERO,
    );
}

#[crate::view(component = TideSurface, name = "TideSurface", priority = 60)]
pub(super) fn surface_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    paint_surface(&mut painter, model);
}

#[crate::view(
    component = TideModalSurface,
    name = "TideModalSurface",
    priority = 70
)]
pub(super) fn modal_render(model: model!(component.model, watch(modal)), paint: paint!()) {
    if model.modal() == TideModal::None {
        return;
    }
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(
        Rect::new(0, 0, 480, 320),
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
