use super::geometry::{HEADER, SURFACE, cell_center, cell_rect, panel};
use super::state::{TwinSurface, TwinTrack};
use super::style::{APRICOT, BG, FLOOR, GRID, LAVENDER, MINT, PANEL, WALL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::expeditions::{BoardMotion, TwinCell};
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::twin::{TwinModel, mapped};
use crate::prelude::*;

const EDGE: Color = Color::rgb(34, 68, 80);

/// Where `station`'s beacon is drawn this frame: on its way from the cell
/// the track recorded, pushed toward a refused move.
fn beacon_point(
    model: &TwinModel,
    station: usize,
    shown: Option<(TwinTrack, BoardMotion)>,
) -> Point {
    let cell = model.position(station);
    let rest = cell_center(station, cell);
    let Some((track, motion)) = shown else {
        return rest;
    };
    // The director has not seen this move yet: draw it landed.
    if track.board.cells[station] != cell {
        return rest;
    }
    let point = motion.along(cell_center(station, track.from[station]), rest);
    let bump = if station == 0 {
        track.board.bump
    } else {
        mapped(track.board.bump, model.level().mode())
    };
    motion.bumped(point, bump)
}

fn paint_board(
    painter: &mut PlayPainter<'_, '_>,
    model: &TwinModel,
    station: usize,
    accent: Color,
    beacon: Point,
) {
    let level = model.level();
    let area = panel(station);
    painter.fill(area, PANEL, Fixed::from_int(9));
    painter.border(area, EDGE, Fixed::ONE, Fixed::from_int(9));
    for cell in 0..36_u8 {
        let rect = cell_rect(station, cell);
        let fill = match level.cell(station, cell) {
            TwinCell::Floor => FLOOR,
            TwinCell::Wall => WALL,
            TwinCell::Door if model.has_key() => FLOOR,
            TwinCell::Door => APRICOT,
        };
        painter.fill(rect, fill, Fixed::from_int(5));
        painter.border(rect, GRID, Fixed::ONE, Fixed::from_int(5));
        if level.goal(station) == cell {
            let inset = Fixed::from_int(7);
            let ring = Rect {
                x: rect.x + inset,
                y: rect.y + inset,
                w: rect.w - inset - inset,
                h: rect.h - inset - inset,
            };
            painter.border(
                ring,
                accent,
                Fixed::from_int(2),
                ring.w / Fixed::from_int(2),
            );
        }
        // Only A can take the key, so it lies on A's board.
        if station == 0 && level.key() == Some(cell) && !model.has_key() {
            painter.disc(cell_center(station, cell), Fixed::from_int(5), APRICOT);
        }
    }
    painter.disc(beacon, Fixed::from_int(11), accent);
    painter.disc(beacon, Fixed::from_int(4), BG);
}

#[crate::view(component = TwinSurface, name = "TwinSurface", priority = 60)]
pub(super) fn surface_render(
    game: model!(component.game, watch(visual_revision)),
    entity: entity!(),
    boards: query!(TwinTrack, BoardMotion),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let shown = boards.get(entity).map(|(track, motion)| (*track, *motion));
    let (width, height) = SURFACE;
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), width, height);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(Rect::new(0, 0, width, height), BG, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, width, HEADER), PANEL, Fixed::ZERO);
    painter.line(
        Point::new(0, HEADER),
        Point::new(width, HEADER),
        GRID,
        Fixed::ONE,
    );
    for (station, accent) in [(0, MINT), (1, LAVENDER)] {
        let beacon = beacon_point(game, station, shown);
        paint_board(&mut painter, game, station, accent, beacon);
    }
}
