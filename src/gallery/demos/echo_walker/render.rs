use super::state::{EchoModalSurface, EchoSurface};
use super::style::{
    ACCENT, BACKGROUND, BOARD, HEADER, LINE, MUTED, PANEL, RUST, TEAL, TEXT, VIOLET, WALL,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::echo::{BEAT_LIMIT, BOARD_HEIGHT, BOARD_WIDTH, EchoModal, EchoModel};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};

#[derive(Clone, Copy)]
pub(super) struct MapBox {
    pub(super) min: usize,
    pub(super) max: usize,
    pub(super) size: i32,
    pub(super) x: i32,
    pub(super) y: i32,
}

pub(super) fn map_box(model: &EchoModel) -> MapBox {
    let room = model.room();
    let mut min = BOARD_WIDTH - 1;
    let mut max = 0;
    for cell in 0..BOARD_WIDTH * BOARD_HEIGHT {
        if !room.is_wall(cell) {
            let x = cell % BOARD_WIDTH;
            min = min.min(x);
            max = max.max(x);
        }
    }
    min = min.saturating_sub(1);
    max = (max + 1).min(BOARD_WIDTH - 1);
    let columns = max - min + 1;
    let size = (278 / columns as i32).min(29);
    MapBox {
        min,
        max,
        size,
        x: 13 + (278 - columns as i32 * size) / 2,
        y: 64,
    }
}

pub(super) fn cell_origin(cell: u8, map: MapBox) -> (i32, i32) {
    let x = usize::from(cell) % BOARD_WIDTH;
    let y = usize::from(cell) / BOARD_WIDTH;
    (
        map.x + (x - map.min) as i32 * map.size,
        map.y + y as i32 * map.size,
    )
}

fn paint_surface(painter: &mut PlayPainter<'_, '_>, model: &EchoModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(Point::new(14, 34), Point::new(466, 34), LINE, Fixed::ONE);
    let map = map_box(model);
    let room = model.room();
    for cell in 0..BOARD_WIDTH * BOARD_HEIGHT {
        let x = cell % BOARD_WIDTH;
        if x < map.min || x > map.max {
            continue;
        }
        let (left, top) = cell_origin(cell as u8, map);
        let area = Rect::new(left + 1, top + 1, map.size - 2, map.size - 2);
        if room.is_wall(cell) {
            painter.fill(area, WALL, Fixed::ONE);
            painter.line(
                Point::new(left + 4, top + map.size - 5),
                Point::new(left + map.size - 4, top + map.size - 5),
                LINE,
                Fixed::ONE,
            );
            continue;
        }
        painter.fill(area, BOARD, Fixed::ONE);
        painter.border(area, LINE, Fixed::ONE, Fixed::ONE);
        let center = Point::new(left + map.size / 2, top + map.size / 2);
        if let Some(door) = room.door_at(cell as u8) {
            let open = model.door_open(door, model.tick(), model.position());
            painter.fill(
                Rect::new(left + 3, top + 3, map.size - 6, map.size - 6),
                if open {
                    Color::rgb(43, 77, 83)
                } else {
                    Color::rgb(112, 93, 88)
                },
                Fixed::ONE,
            );
            let color = if open { TEAL } else { RUST };
            for bar in 0..3 {
                painter.line(
                    Point::new(left + 6 + bar * 5, top + 5),
                    Point::new(left + 6 + bar * 5, top + map.size - 5),
                    color,
                    Fixed::ONE,
                );
            }
        }
        if let Some(plate) = room.plate_at(cell as u8) {
            painter.circle(center, Fixed::from_int(map.size / 3), VIOLET);
            painter.circle(center, Fixed::from_int(map.size / 5), BOARD);
            if model.door_open(plate, model.tick(), model.position()) {
                painter.arc(
                    center,
                    Fixed::from_int(map.size / 2 - 2),
                    Fixed::ZERO,
                    Fixed::from_int(360),
                    TEAL,
                    Fixed::ONE,
                );
            }
        }
        if cell as u8 == room.start {
            painter.circle(center, Fixed::from_int(4), MUTED);
        }
        if cell as u8 == room.end {
            painter.border(
                Rect::new(left + 6, top + 6, map.size - 12, map.size - 12),
                ACCENT,
                Fixed::ONE,
                Fixed::ONE,
            );
        }
        if let Some(gem) = room.gem_at(cell as u8)
            && !model.collected(gem)
        {
            painter.circle(center, Fixed::from_int(5), ACCENT);
            painter.circle(center, Fixed::from_int(2), BACKGROUND);
        }
        if room.clock_at(cell as u8).is_some() {
            let open = model.clock_open(cell as u8, model.tick() + 1);
            let color = if open {
                TEAL
            } else {
                Color::rgb(177, 121, 131)
            };
            painter.circle(center, Fixed::from_int(7), BOARD);
            painter.arc(
                center,
                Fixed::from_int(7),
                Fixed::ZERO,
                Fixed::from_int(360),
                color,
                Fixed::ONE,
            );
            painter.line(
                center,
                Point::new(center.x, center.y - Fixed::from_int(5)),
                color,
                Fixed::ONE,
            );
            painter.line(
                center,
                Point::new(center.x + Fixed::from_int(4), center.y),
                color,
                Fixed::ONE,
            );
        }
    }
    let colors = [TEAL, VIOLET, RUST];
    for (ghost, color) in colors
        .iter()
        .copied()
        .enumerate()
        .take(usize::from(model.ghost_count()))
    {
        if model
            .peek_ghost()
            .is_some_and(|peek| usize::from(peek) != ghost)
        {
            continue;
        }
        let len = model.ghost_route_len(ghost);
        for beat in 1..=len {
            let a = model.route_cell(ghost + 1, beat - 1);
            let b = model.route_cell(ghost + 1, beat);
            let (ax, ay) = cell_origin(a, map);
            let (bx, by) = cell_origin(b, map);
            painter.line(
                Point::new(ax + map.size / 2, ay + map.size / 2),
                Point::new(bx + map.size / 2, by + map.size / 2),
                color,
                Fixed::ONE,
            );
        }
        let cell = model.ghost_position(ghost, model.tick());
        let (left, top) = cell_origin(cell, map);
        let center = Point::new(left + map.size / 2, top + map.size / 2);
        painter.circle(center, Fixed::from_int(map.size / 3), colors[ghost]);
        painter.circle(center, Fixed::from_int(map.size / 5), BOARD);
    }
    let (left, top) = cell_origin(model.position(), map);
    let center = Point::new(left + map.size / 2, top + map.size / 2);
    painter.circle(center, Fixed::from_int(7), TEXT);
    painter.circle(center, Fixed::from_int(3), BACKGROUND);
    for beat in 0..BEAT_LIMIT {
        painter.fill(
            Rect::new(14 + i32::from(beat) * 575 / 100, 274, 4, 5),
            if beat < model.tick() {
                ACCENT
            } else {
                Color::rgb(71, 81, 105)
            },
            Fixed::ONE,
        );
    }
    painter.line(Point::new(14, 298), Point::new(466, 298), LINE, Fixed::ONE);
}

#[crate::view(
    component = EchoSurface,
    read(model),
    watch(model.visual_revision()),
    name = "EchoSurface",
    priority = 60
)]
pub(super) fn surface_render(
    renderer: &mut dyn Renderer,
    model: &EchoModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_surface(&mut painter, model);
}

#[crate::view(
    component = EchoModalSurface,
    read(model),
    watch(model.modal()),
    name = "EchoModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    renderer: &mut dyn Renderer,
    model: &EchoModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if model.modal() == EchoModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 34, 480, 264),
        Color::rgba(24, 27, 48, 239),
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
