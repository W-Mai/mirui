use super::geometry::{HEADER, SURFACE, cell_center, cell_rect, hull_rect, panel};
use super::state::{FoldSurface, FoldTrack};
use super::style::{APRICOT, BG, BLUE, FLOOR, GRID, LAVENDER, MINT, PANEL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::expeditions::{BoardMotion, FoldCell};
use crate::gallery::play::fold::{FoldModel, HullPose};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;

const EDGE: Color = Color::rgb(42, 68, 81);

/// Where the hull is drawn this frame: rolling over from the cells the
/// track recorded, pushed toward a refused roll. Also whether it is at
/// rest, which is when its upright or lying mark shows.
fn hull_area(model: &FoldModel, shown: Option<(FoldTrack, BoardMotion)>) -> (Rect, bool) {
    let (cells, len) = model.occupied();
    let rest = hull_rect(cells, len);
    let Some((track, motion)) = shown else {
        return (rest, true);
    };
    // The director has not seen this roll yet: draw it landed.
    if (track.board.cells, track.board.len) != (cells, len) {
        return (rest, true);
    }
    let from = hull_rect(track.from.0, track.from.1);
    let corner = motion.along(Point::new(from.x, from.y), Point::new(rest.x, rest.y));
    let size = motion.along(Point::new(from.w, from.h), Point::new(rest.w, rest.h));
    let corner = motion.bumped(corner, track.board.bump);
    let area = Rect {
        x: corner.x,
        y: corner.y,
        w: size.x,
        h: size.y,
    };
    (
        area,
        motion.travel == Fixed::ONE && motion.bump == Fixed::ZERO,
    )
}

fn inset(area: Rect, by: i32) -> Rect {
    let by = Fixed::from_int(by);
    Rect {
        x: area.x + by,
        y: area.y + by,
        w: area.w - by - by,
        h: area.h - by - by,
    }
}

fn paint_hull(painter: &mut PlayPainter<'_, '_>, model: &FoldModel, area: Rect, at_rest: bool) {
    painter.fill(inset(area, 2), MINT, Fixed::from_int(6));
    if !at_rest {
        return;
    }
    if model.pose() == HullPose::Upright {
        painter.border(inset(area, 9), BG, Fixed::from_int(2), Fixed::from_int(3));
        return;
    }
    // A lying hull shows the seam between its two halves.
    let (half, edge) = (Fixed::from_int(2), Fixed::from_int(8));
    let (from, to) = if area.w > area.h {
        let x = area.x + area.w / half;
        (
            Point::new(x, area.y + edge),
            Point::new(x, area.y + area.h - edge),
        )
    } else {
        let y = area.y + area.h / half;
        (
            Point::new(area.x + edge, y),
            Point::new(area.x + area.w - edge, y),
        )
    };
    painter.line(from, to, BG, Fixed::from_int(2));
}

fn paint_map(painter: &mut PlayPainter<'_, '_>, model: &FoldModel, hull: (Rect, bool)) {
    let level = model.level();
    let area = panel();
    painter.fill(area, PANEL, Fixed::from_int(9));
    painter.border(area, EDGE, Fixed::ONE, Fixed::from_int(9));
    for cell in 0..70_u8 {
        let kind = level.cell(cell);
        if kind == FoldCell::Void {
            continue;
        }
        let rect = cell_rect(cell);
        let fill = match kind {
            FoldCell::Solid => FLOOR,
            FoldCell::Fragile => Color::rgb(62, 55, 76),
            FoldCell::Bridge if model.bridge_on() => BLUE,
            FoldCell::Bridge => Color::rgb(20, 35, 47),
            FoldCell::Switch => Color::rgb(45, 81, 74),
            FoldCell::Void => BG,
        };
        painter.fill(rect, fill, Fixed::from_int(5));
        painter.border(rect, GRID, Fixed::ONE, Fixed::from_int(5));
        if level.goal() == cell {
            painter.border(inset(rect, 7), MINT, Fixed::from_int(2), Fixed::from_int(4));
        }
        for seal in 0..usize::from(level.seal_count()) {
            if level.seal(seal) == Some(cell) && model.seal_bits() & (1 << seal) == 0 {
                painter.disc(cell_center(cell), Fixed::from_int(6), APRICOT);
            }
        }
        if kind == FoldCell::Switch {
            painter.disc(cell_center(cell), Fixed::from_int(5), LAVENDER);
        }
    }
    paint_hull(painter, model, hull.0, hull.1);
}

#[crate::view(component = FoldSurface, name = "FoldSurface", priority = 60)]
pub(super) fn surface_render(
    game: model!(component.game, watch(visual_revision)),
    entity: entity!(),
    maps: query!(FoldTrack, BoardMotion),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let shown = maps.get(entity).map(|(track, motion)| (*track, *motion));
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
    let hull = hull_area(game, shown);
    paint_map(&mut painter, game, hull);
}
