use super::state::{EchoModalSurface, EchoSurface};
use super::style::{
    ACCENT, BACKGROUND, BOARD, ECHO_COLORS, HEADER, LINE, MUTED, PANEL, RUST, TEAL, TEXT, VIOLET,
    WALL,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::echo::{
    BEAT_LIMIT, BOARD_WIDTH, Direction, EchoModal, EchoModel, EchoRoom, MAX_GHOSTS,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::types::Transform;

/// The area the room is centred in, left of the control column.
const REGION: Rect = Rect {
    x: Fixed::from_int(6),
    y: Fixed::from_int(34),
    w: Fixed::from_int(376),
    h: Fixed::from_int(262),
};
/// Thickness of the room's outer wall around the floor cells.
const FRAME: i32 = 6;
/// The beat strip under the room.
const TIMELINE_LEFT: i32 = 12;
const TIMELINE_WIDTH: i32 = 370;
const TIMELINE_TOP: i32 = 300;

/// Where the room's cells are drawn: the columns and rows they cover, the
/// cell size and the top-left corner of the first cell.
#[derive(Clone, Copy)]
pub(super) struct MapBox {
    pub(super) min_x: usize,
    pub(super) max_x: usize,
    pub(super) min_y: usize,
    pub(super) max_y: usize,
    pub(super) size: i32,
    pub(super) x: i32,
    pub(super) y: i32,
}

impl MapBox {
    const fn columns(self) -> i32 {
        (self.max_x - self.min_x + 1) as i32
    }

    const fn rows(self) -> i32 {
        (self.max_y - self.min_y + 1) as i32
    }

    pub(super) const fn contains(self, x: usize, y: usize) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    fn frame(self) -> Rect {
        Rect::new(
            self.x - FRAME,
            self.y - FRAME,
            self.columns() * self.size + 2 * FRAME,
            self.rows() * self.size + 2 * FRAME,
        )
    }
}

pub(super) fn map_box_from_span(span: (usize, usize, usize, usize)) -> MapBox {
    let (min_x, max_x, min_y, max_y) = span;
    let columns = (max_x - min_x + 1) as i32;
    let rows = (max_y - min_y + 1) as i32;
    let region_w = REGION.w.to_int();
    let region_h = REGION.h.to_int();
    let size = ((region_w - 2 * FRAME) / columns).min((region_h - 2 * FRAME) / rows);
    MapBox {
        min_x,
        max_x,
        min_y,
        max_y,
        size,
        x: REGION.x.to_int() + (region_w - columns * size) / 2,
        y: REGION.y.to_int() + (region_h - rows * size) / 2,
    }
}

pub(super) fn map_box(model: &EchoModel) -> MapBox {
    map_box_from_span(model.room_span())
}

pub(super) fn cell_origin(cell: u8, map: MapBox) -> (i32, i32) {
    let x = usize::from(cell) % BOARD_WIDTH;
    let y = usize::from(cell) / BOARD_WIDTH;
    (
        map.x + (x as i32 - map.min_x as i32) * map.size,
        map.y + (y as i32 - map.min_y as i32) * map.size,
    )
}

fn cell_center(cell: u8, map: MapBox) -> Point {
    let (left, top) = cell_origin(cell, map);
    Point::new(left + map.size / 2, top + map.size / 2)
}

/// The point `progress` of the way from `from` to `to`.
fn between(from: Point, to: Point, progress: Fixed) -> Point {
    Point {
        x: from.x + (to.x - from.x) * progress,
        y: from.y + (to.y - from.y) * progress,
    }
}

/// The point on a route at a fractional beat.
fn route_point(map: MapBox, beat: Fixed, cell_at: impl Fn(u8) -> u8) -> Point {
    let whole = beat.to_int().max(0) as u8;
    let fraction = beat - Fixed::from_int(i32::from(whole));
    between(
        cell_center(cell_at(whole), map),
        cell_center(cell_at(whole.saturating_add(1)), map),
        fraction,
    )
}

fn ratio(size: i32, numerator: i32, denominator: i32) -> Fixed {
    Fixed::from_int(size * numerator) / Fixed::from_int(denominator)
}

fn paint_cell(
    painter: &mut PlayPainter<'_, '_>,
    model: &EchoModel,
    room: &EchoRoom,
    cell: u8,
    map: MapBox,
) {
    let s = map.size;
    let (left, top) = cell_origin(cell, map);
    let area = Rect::new(left + 1, top + 1, s - 2, s - 2);
    if room.is_wall(usize::from(cell)) {
        painter.fill(area, WALL, Fixed::from_int(2));
        painter.line(
            Point::new(left + 4, top + s - 5),
            Point::new(left + s - 4, top + s - 5),
            LINE,
            Fixed::ONE,
        );
        return;
    }
    painter.fill(area, BOARD, Fixed::from_int(2));
    painter.border(area, LINE, Fixed::ONE, Fixed::from_int(2));
    let center = Point::new(left + s / 2, top + s / 2);
    let beat = model.tick();
    if let Some(door) = room.door_at(cell) {
        let open = model.door_open(door, beat, model.position());
        painter.fill(
            Rect::new(left + 4, top + 4, s - 8, s - 8),
            if open {
                Color::rgb(43, 77, 83)
            } else {
                Color::rgb(112, 93, 88)
            },
            Fixed::from_int(2),
        );
        let color = if open { TEAL } else { RUST };
        for bar in 1..4 {
            let x = left + s * bar / 4;
            painter.line(
                Point::new(x, top + 7),
                Point::new(x, top + s - 7),
                color,
                Fixed::from_int(2),
            );
        }
    }
    if let Some(plate) = room.plate_at(cell) {
        painter.circle(center, ratio(s, 1, 3), VIOLET);
        painter.circle(center, ratio(s, 1, 5), BOARD);
        if model.door_open(plate, beat, model.position()) {
            painter.arc(
                center,
                ratio(s, 1, 2) - Fixed::from_int(3),
                Fixed::ZERO,
                Fixed::from_int(360),
                TEAL,
                Fixed::ONE,
            );
        }
    }
    if cell == room.start {
        painter.circle(center, ratio(s, 1, 10), MUTED);
    }
    if cell == room.end {
        let inset = s / 5;
        let exit = Rect::new(left + inset, top + inset, s - 2 * inset, s - 2 * inset);
        if model.collected_count() == model.gem_count() {
            painter.fill(exit, ACCENT.scale_alpha(70), Fixed::from_int(2));
            painter.border(exit, ACCENT, Fixed::from_int(2), Fixed::from_int(2));
        } else {
            painter.border(
                exit,
                ACCENT.scale_alpha(150),
                Fixed::ONE,
                Fixed::from_int(2),
            );
        }
    }
    if let Some(gem) = room.gem_at(cell)
        && !model.collected(gem)
    {
        painter.circle(center, ratio(s, 1, 7), ACCENT);
        painter.circle(center, ratio(s, 1, 16), BACKGROUND);
    }
    if room.clock_at(cell).is_some() {
        let color = if model.clock_open(cell, beat + 1) {
            TEAL
        } else {
            Color::rgb(177, 121, 131)
        };
        let radius = ratio(s, 1, 5);
        painter.circle(center, radius, BOARD);
        painter.arc(
            center,
            radius,
            Fixed::ZERO,
            Fixed::from_int(360),
            color,
            Fixed::ONE,
        );
        painter.line(
            center,
            Point::new(center.x, center.y - radius + Fixed::from_int(2)),
            color,
            Fixed::ONE,
        );
        painter.line(
            center,
            Point::new(center.x + radius - Fixed::from_int(3), center.y),
            color,
            Fixed::ONE,
        );
    }
}

fn paint_echoes(painter: &mut PlayPainter<'_, '_>, model: &EchoModel, map: MapBox) {
    let s = map.size;
    let (_, slide_tick, slide) = model.slide();
    let soloed = model.peek_ghost().map(usize::from);
    for (ghost, color) in ECHO_COLORS
        .iter()
        .copied()
        .enumerate()
        .take(usize::from(model.ghost_count()))
    {
        if soloed.is_some_and(|peek| peek != ghost) {
            continue;
        }
        let len = model.ghost_route_len(ghost);
        for beat in 1..=len {
            painter.line(
                cell_center(model.route_cell(ghost + 1, beat - 1), map),
                cell_center(model.route_cell(ghost + 1, beat), map),
                color.scale_alpha(if soloed.is_some() { 230 } else { 120 }),
                Fixed::from_int(if soloed.is_some() { 2 } else { 1 }),
            );
        }
        if let Some(beat) = model.replay_beat() {
            // A soloed tape walks its route again on the board, in a loop.
            let point = route_point(map, beat, |beat| model.ghost_position(ghost, beat));
            painter.arc(
                point,
                ratio(s, 1, 4),
                Fixed::ZERO,
                Fixed::from_int(360),
                color,
                Fixed::from_int(2),
            );
        }
        let center = between(
            cell_center(model.ghost_position(ghost, slide_tick), map),
            cell_center(model.ghost_position(ghost, model.tick()), map),
            slide,
        );
        painter.circle(center, ratio(s, 3, 10), color);
        painter.circle(center, ratio(s, 2, 10), BOARD);
    }
}

fn paint_walker(painter: &mut PlayPainter<'_, '_>, model: &EchoModel, map: MapBox) {
    let s = map.size;
    let center = if let Some((progress, len, echo)) = model.rewinding() {
        // The route runs back to the start, leaving its line behind.
        let color = echo.map_or(MUTED, |echo| ECHO_COLORS[usize::from(echo)]);
        let beat = Fixed::from_int(i32::from(len)) * (Fixed::ONE - progress);
        for step in 1..=len {
            painter.line(
                cell_center(model.rewind_cell(step - 1), map),
                cell_center(model.rewind_cell(step), map),
                color.scale_alpha(200),
                Fixed::from_int(2),
            );
        }
        route_point(map, beat, |beat| model.rewind_cell(beat.min(len)))
    } else {
        let (from, _, slide) = model.slide();
        let mut center = between(
            cell_center(from, map),
            cell_center(model.position(), map),
            slide,
        );
        let (direction, lean) = model.nudge();
        let (dx, dy) = match direction {
            Direction::Up => (0, -1),
            Direction::Right => (1, 0),
            Direction::Down => (0, 1),
            Direction::Left => (-1, 0),
            Direction::Wait => (0, 0),
        };
        let reach = lean * ratio(s, 1, 6);
        center.x += reach * Fixed::from_int(dx);
        center.y += reach * Fixed::from_int(dy);
        center
    };
    painter.circle(center, ratio(s, 22, 100), TEXT);
    painter.circle(center, ratio(s, 9, 100), BACKGROUND);
}

fn paint_timeline(painter: &mut PlayPainter<'_, '_>, model: &EchoModel) {
    let x = |beat: i32| TIMELINE_LEFT + beat * TIMELINE_WIDTH / i32::from(BEAT_LIMIT);
    for beat in 0..i32::from(BEAT_LIMIT) {
        let color = if beat < i32::from(model.tick()) {
            ACCENT
        } else {
            Color::rgb(71, 81, 105)
        };
        painter.fill(Rect::new(x(beat), TIMELINE_TOP, 6, 4), color, Fixed::ONE);
    }
    // Each echo stops on the beat its route ends and holds that cell.
    for (ghost, length) in model
        .ghost_route_lengths()
        .into_iter()
        .take(usize::from(model.ghost_count()))
        .enumerate()
        .take(MAX_GHOSTS)
    {
        let beat = i32::from(length.max(1)) - 1;
        painter.fill(
            Rect::new(x(beat), TIMELINE_TOP - 3, 6, 2),
            ECHO_COLORS[ghost],
            Fixed::ZERO,
        );
    }
}

fn paint_surface(painter: &mut PlayPainter<'_, '_>, model: &EchoModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 28), HEADER, Fixed::ZERO);
    painter.line(Point::new(0, 28), Point::new(480, 28), LINE, Fixed::ONE);
    painter.line(Point::new(386, 34), Point::new(386, 316), LINE, Fixed::ONE);
    let map = map_box(model);
    let intro = model.intro();
    let mut board = painter.transformed(Transform::translate(
        Fixed::ZERO,
        (Fixed::ONE - intro) * Fixed::from_int(12),
    ));
    board.fill(map.frame(), WALL, Fixed::from_int(6));
    let room = model.room();
    for y in map.min_y..=map.max_y {
        for x in map.min_x..=map.max_x {
            paint_cell(&mut board, model, room, (y * BOARD_WIDTH + x) as u8, map);
        }
    }
    paint_echoes(&mut board, model, map);
    paint_walker(&mut board, model, map);
    if intro < Fixed::ONE {
        // The room fades in over the background as it settles.
        let veil = (Fixed::ONE - intro) * Fixed::from_int(255);
        board.fill(
            map.frame(),
            BACKGROUND.scale_alpha(veil.to_int().clamp(0, 255) as u8),
            Fixed::from_int(6),
        );
    }
    paint_timeline(painter, model);
}

#[crate::view(component = EchoSurface, name = "EchoSurface", priority = 60)]
pub(super) fn surface_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    paint_surface(&mut painter, model);
}

/// The panel every dialog sits on.
const PANEL_RECT: Rect = Rect {
    x: Fixed::from_int(36),
    y: Fixed::from_int(40),
    w: Fixed::from_int(408),
    h: Fixed::from_int(256),
};
/// Left edges of the five legend icons in the rules.
pub(super) const LEGEND_X: [i32; 5] = [56, 134, 212, 290, 368];
const LEGEND_Y: i32 = 98;

fn paint_legend(painter: &mut PlayPainter<'_, '_>) {
    let center = |index: usize| Point::new(LEGEND_X[index] + 28, LEGEND_Y);
    let square = |center: Point, half: i32| Rect {
        x: center.x - Fixed::from_int(half),
        y: center.y - Fixed::from_int(half),
        w: Fixed::from_int(2 * half),
        h: Fixed::from_int(2 * half),
    };
    painter.circle(center(0), Fixed::from_int(9), VIOLET);
    painter.circle(center(0), Fixed::from_int(5), BOARD);
    let door = center(1);
    painter.fill(
        square(door, 10),
        Color::rgb(112, 93, 88),
        Fixed::from_int(2),
    );
    for bar in [-5, 0, 5] {
        painter.line(
            Point {
                x: door.x + Fixed::from_int(bar),
                y: door.y - Fixed::from_int(7),
            },
            Point {
                x: door.x + Fixed::from_int(bar),
                y: door.y + Fixed::from_int(7),
            },
            RUST,
            Fixed::from_int(2),
        );
    }
    let clock = center(2);
    painter.arc(
        clock,
        Fixed::from_int(8),
        Fixed::ZERO,
        Fixed::from_int(360),
        TEAL,
        Fixed::ONE,
    );
    painter.line(
        clock,
        Point {
            y: clock.y - Fixed::from_int(6),
            ..clock
        },
        TEAL,
        Fixed::ONE,
    );
    painter.line(
        clock,
        Point {
            x: clock.x + Fixed::from_int(5),
            ..clock
        },
        TEAL,
        Fixed::ONE,
    );
    painter.circle(center(3), Fixed::from_int(6), ACCENT);
    painter.circle(center(3), Fixed::from_int(2), BACKGROUND);
    painter.border(
        square(center(4), 8),
        ACCENT,
        Fixed::from_int(2),
        Fixed::from_int(2),
    );
}

/// One pip per room, filled for the rooms already solved.
fn paint_progress(painter: &mut PlayPainter<'_, '_>, solved: u8) {
    for room in 0..12_i32 {
        let area = Rect::new(56 + room * 31, 178, 25, 8);
        if room < i32::from(solved) {
            painter.fill(area, ACCENT, Fixed::from_int(2));
        } else {
            painter.fill(area, HEADER, Fixed::from_int(2));
            painter.border(area, LINE, Fixed::ONE, Fixed::from_int(2));
        }
    }
}

#[crate::view(
    component = EchoModalSurface,
    name = "EchoModalSurface",
    priority = 70
)]
pub(super) fn modal_render(model: model!(component.model, watch(modal)), paint: paint!()) {
    if model.modal() == EchoModal::None {
        return;
    }
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(
        Rect::new(0, 29, 480, 291),
        Color::rgba(24, 27, 48, 224),
        Fixed::ZERO,
    );
    painter.fill(PANEL_RECT, PANEL, Fixed::from_int(7));
    painter.border(PANEL_RECT, LINE, Fixed::ONE, Fixed::from_int(7));
    painter.line(Point::new(52, 76), Point::new(428, 76), LINE, Fixed::ONE);
    if model.modal() == EchoModal::Menu {
        paint_legend(&mut painter);
    } else {
        let solved = model.result_len() + u8::from(!model.complete());
        paint_progress(&mut painter, solved);
    }
}
