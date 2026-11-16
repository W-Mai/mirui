use super::state::{FactoryModalSurface, FactorySurface};
use super::style::{
    ACCENT, BACKGROUND, CARD, CELL, CYAN, FRAME, GREEN, GRID_LEFT, GRID_TOP, INK, LINE, MUTED,
    PANEL, RED, SIDE, SIDE_LEFT, SIDE_WIDTH, STAR_EMPTY, TILE_HEIGHT, TILE_WIDTH, WORK,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::factory::{
    CELL_COUNT, FactoryModal, FactoryModel, FactoryPage, GRID_WIDTH, MAX_ARRIVALS, MaterialStage,
    ModuleKind,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::path::Path;
use crate::types::Transform;

/// A five-pointed star of radius one, scaled by `paint_star`.
static STAR: Path = crate::path!(
    "M 0 -1 L 0.2245 -0.309 L 0.9511 -0.309 L 0.3633 0.118 L 0.5878 0.809 L 0 0.382 L -0.5878 0.809 L -0.3633 0.118 L -0.9511 -0.309 L -0.2245 -0.309 Z"
);

fn rect((x, y, w, h): (i32, i32, i32, i32)) -> Rect {
    Rect::new(x, y, w, h)
}

fn paint_shell(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 26), INK, Fixed::ZERO);
}

/// The floor on the left and the side column of every page.
fn paint_frame(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(rect(FRAME), WORK, Fixed::ZERO);
    painter.border(rect(FRAME), LINE, Fixed::ONE, Fixed::ZERO);
    painter.fill(rect(SIDE), PANEL, Fixed::ZERO);
    painter.border(rect(SIDE), LINE, Fixed::ONE, Fixed::ZERO);
}

pub(super) fn cell_rect(index: usize) -> Rect {
    let x = GRID_LEFT + (index % GRID_WIDTH) as i32 * CELL;
    let y = GRID_TOP + (index / GRID_WIDTH) as i32 * CELL;
    Rect::new(x, y, CELL - 1, CELL - 1)
}

fn center(rect: Rect) -> Point {
    Point {
        x: rect.x + rect.w / Fixed::from_int(2),
        y: rect.y + rect.h / Fixed::from_int(2),
    }
}

/// The point `share` of the way from `from` to `to`.
fn between(from: Point, to: Point, share: Fixed) -> Point {
    Point {
        x: from.x + (to.x - from.x) * share,
        y: from.y + (to.y - from.y) * share,
    }
}

fn paint_direction(painter: &mut PlayPainter<'_, '_>, rect: Rect, direction: u8, color: Color) {
    let center = center(rect);
    let (dx, dy) = match direction % 4 {
        0 => (10, 0),
        1 => (0, 10),
        2 => (-10, 0),
        _ => (0, -10),
    };
    let end = Point {
        x: center.x + Fixed::from_int(dx),
        y: center.y + Fixed::from_int(dy),
    };
    painter.line(center, end, color, Fixed::from_int(2));
    let (ax, ay) = if dx != 0 {
        (-dx.signum() * 5, 5)
    } else {
        (5, -dy.signum() * 5)
    };
    painter.line(
        end,
        Point {
            x: end.x + Fixed::from_int(ax),
            y: end.y + Fixed::from_int(ay),
        },
        color,
        Fixed::from_int(2),
    );
    painter.line(
        end,
        Point {
            x: end.x + Fixed::from_int(if dx != 0 { ax } else { -ax }),
            y: end.y + Fixed::from_int(if dx != 0 { -ay } else { ay }),
        },
        color,
        Fixed::from_int(2),
    );
}

/// A short orange bar just past the side where a module drops its items
/// into nothing.
fn paint_dead_end(painter: &mut PlayPainter<'_, '_>, rect: Rect, direction: u8) {
    let (x, y) = (rect.x.to_int(), rect.y.to_int());
    let bar = match direction % 4 {
        0 => Rect::new(x + CELL, y + 10, 3, CELL - 21),
        1 => Rect::new(x + 10, y + CELL, CELL - 21, 3),
        2 => Rect::new(x - 4, y + 10, 3, CELL - 21),
        _ => Rect::new(x + 10, y - 4, CELL - 21, 3),
    };
    painter.fill(bar, ACCENT, Fixed::ZERO);
}

fn paint_wall(painter: &mut PlayPainter<'_, '_>, rect: Rect) {
    painter.fill(rect, LINE, Fixed::ZERO);
    let (x, y) = (rect.x.to_int(), rect.y.to_int());
    for offset in [0, 12, 24] {
        painter.line(
            Point::new(x + offset, y + CELL - 4),
            Point::new(x + offset + 12, y + 1),
            MUTED,
            Fixed::ONE,
        );
    }
}

pub(super) fn stage_color(stage: MaterialStage) -> Color {
    match stage {
        MaterialStage::Ore => Color::rgb(156, 112, 73),
        MaterialStage::Plate => Color::rgb(188, 198, 194),
        MaterialStage::Gear => ACCENT,
        MaterialStage::Certified => GREEN,
    }
}

fn paint_item(painter: &mut PlayPainter<'_, '_>, at: Point, stage: MaterialStage, ring: Color) {
    painter.circle(at, Fixed::from_int(7), ring);
    painter.circle(at, Fixed::from_int(5), stage_color(stage));
}

fn paint_grid(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    let right = GRID_LEFT + GRID_WIDTH as i32 * CELL;
    let bottom = GRID_TOP + (CELL_COUNT / GRID_WIDTH) as i32 * CELL;
    for column in 0..=GRID_WIDTH as i32 {
        let x = GRID_LEFT + column * CELL - 1;
        painter.line(
            Point::new(x, GRID_TOP),
            Point::new(x, bottom),
            LINE,
            Fixed::from_ratio(1, 2),
        );
    }
    for row in 0..=(CELL_COUNT / GRID_WIDTH) as i32 {
        let y = GRID_TOP + row * CELL - 1;
        painter.line(
            Point::new(GRID_LEFT, y),
            Point::new(right, y),
            LINE,
            Fixed::from_ratio(1, 2),
        );
    }
    for index in 0..CELL_COUNT {
        let Some(cell) = model.cell(index) else {
            continue;
        };
        let rect = cell_rect(index);
        let inner = Rect {
            x: rect.x + Fixed::ONE,
            y: rect.y + Fixed::ONE,
            w: rect.w - Fixed::from_int(2),
            h: rect.h - Fixed::from_int(2),
        };
        let fill = match cell.kind {
            ModuleKind::Wall => {
                paint_wall(painter, inner);
                continue;
            }
            ModuleKind::Source | ModuleKind::Dock => INK,
            ModuleKind::Furnace => CYAN,
            ModuleKind::Assembler => ACCENT,
            ModuleKind::Inspector => GREEN,
            ModuleKind::Belt => PANEL,
        };
        painter.fill(inner, fill, Fixed::ZERO);
        if cell.kind != ModuleKind::Dock {
            paint_direction(
                painter,
                rect,
                cell.direction,
                if cell.kind == ModuleKind::Belt {
                    MUTED
                } else {
                    PANEL
                },
            );
        }
        if cell.kind.process().is_some() {
            painter.border(
                Rect::new(rect.x.to_int() + 12, rect.y.to_int() + 10, 15, 19),
                PANEL,
                Fixed::ONE,
                Fixed::ZERO,
            );
        }
    }
    for index in 0..CELL_COUNT {
        if let Some(cell) = model.cell(index)
            && model.dead_end(index)
        {
            paint_dead_end(painter, cell_rect(index), cell.direction);
        }
    }
    // Items go on top of every module, so one sliding between two cells is
    // never covered by the cell it is leaving.
    let glide = model.glide();
    for index in 0..CELL_COUNT {
        if let Some(item) = model.item(index) {
            let at = between(
                center(cell_rect(usize::from(item.from))),
                center(cell_rect(index)),
                glide,
            );
            paint_item(painter, at, item.stage, PANEL);
        }
    }
    if glide < Fixed::ONE {
        for arrival in (0..MAX_ARRIVALS).filter_map(|index| model.arrival(index)) {
            let at = between(
                center(cell_rect(usize::from(arrival.from))),
                center(cell_rect(usize::from(arrival.dock))),
                glide,
            );
            paint_item(
                painter,
                at,
                arrival.stage,
                if arrival.accepted { PANEL } else { RED },
            );
        }
    }
    painter.border(
        cell_rect(model.selected()),
        ACCENT,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
}

/// A thin track and its fill, orange once the value runs over.
fn paint_meter(painter: &mut PlayPainter<'_, '_>, y: i32, value: i32, max: i32, color: Color) {
    let max = max.max(1);
    painter.fill(Rect::new(SIDE_LEFT, y, SIDE_WIDTH, 3), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(SIDE_LEFT, y, SIDE_WIDTH * value.min(max) / max, 3),
        if value > max { ACCENT } else { color },
        Fixed::ZERO,
    );
}

fn paint_line_page(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    paint_frame(painter);
    paint_grid(painter, model);
    let order = model.mission();
    let goal = i32::from(order.goal.max(1));
    painter.circle(
        Point::new(SIDE_LEFT + 4, 40),
        Fixed::from_int(4),
        stage_color(order.target),
    );
    painter.fill(Rect::new(SIDE_LEFT, 74, SIDE_WIDTH, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(
            SIDE_LEFT,
            74,
            SIDE_WIDTH * i32::from(model.delivered().min(order.goal)) / goal,
            4,
        ),
        ACCENT,
        Fixed::ZERO,
    );
    paint_meter(
        painter,
        98,
        i32::from(model.cost()),
        i32::from(order.budget),
        CYAN,
    );
    paint_meter(
        painter,
        119,
        i32::from(model.power()),
        i32::from(order.power),
        GREEN,
    );
    painter.line(
        Point::new(SIDE_LEFT, 127),
        Point::new(SIDE_LEFT + SIDE_WIDTH, 127),
        LINE,
        Fixed::ONE,
    );
}

pub(super) fn tile_left(index: u8) -> i32 {
    8 + i32::from(index % 2) * (TILE_WIDTH + 6)
}

pub(super) fn tile_top(index: u8) -> i32 {
    36 + i32::from(index / 2) * (TILE_HEIGHT + 6)
}

fn paint_orders_page(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    paint_frame(painter);
    let current = model.mission_index();
    painter.fill(
        Rect::new(tile_left(current) - 4, tile_top(current), 3, TILE_HEIGHT),
        ACCENT,
        Fixed::ZERO,
    );
    painter.line(
        Point::new(SIDE_LEFT, 146),
        Point::new(SIDE_LEFT + SIDE_WIDTH, 146),
        LINE,
        Fixed::ONE,
    );
}

fn paint_chart(
    painter: &mut PlayPainter<'_, '_>,
    model: &FactoryModel,
    rect: Rect,
    blocked: bool,
    color: Color,
) {
    painter.fill(rect, PANEL, Fixed::ZERO);
    painter.border(rect, LINE, Fixed::ONE, Fixed::ZERO);
    for row in 1..4 {
        let y = rect.y + rect.h * Fixed::from_int(row) / Fixed::from_int(4);
        painter.line(
            Point { x: rect.x, y },
            Point {
                x: rect.x + rect.w,
                y,
            },
            LINE,
            Fixed::from_ratio(1, 2),
        );
    }
    let len = usize::from(model.telemetry_len());
    if len < 2 {
        return;
    }
    let value = |index: usize| {
        model.telemetry(index).map_or(0, |sample| {
            if blocked {
                i32::from(sample.blocked)
            } else {
                i32::from(sample.delivered)
            }
        })
    };
    let max_value = if blocked {
        (0..len).map(value).max().unwrap_or(0).max(4)
    } else {
        i32::from(model.mission().goal.max(1))
    };
    let mut previous = None;
    for index in 0..len {
        let point = Point {
            x: rect.x + rect.w * Fixed::from_int(index as i32) / Fixed::from_int((len - 1) as i32),
            y: rect.y + rect.h
                - rect.h * Fixed::from_int(value(index).min(max_value))
                    / Fixed::from_int(max_value),
        };
        if let Some(from) = previous {
            painter.line(from, point, color, Fixed::from_ratio(3, 2));
        }
        previous = Some(point);
    }
}

fn paint_telemetry_page(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    paint_frame(painter);
    paint_chart(painter, model, Rect::new(8, 50, 354, 112), false, ACCENT);
    paint_chart(painter, model, Rect::new(8, 186, 354, 100), true, CYAN);
    for row in 1..4 {
        let y = 32 + row * 40;
        painter.line(
            Point::new(SIDE_LEFT, y),
            Point::new(SIDE_LEFT + SIDE_WIDTH, y),
            LINE,
            Fixed::ONE,
        );
    }
}

fn paint_factory_surface(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    paint_shell(painter);
    match model.page() {
        FactoryPage::Line => paint_line_page(painter, model),
        FactoryPage::Orders => paint_orders_page(painter, model),
        FactoryPage::Telemetry => paint_telemetry_page(painter, model),
    }
}

#[crate::view(component = FactorySurface, name = "FactorySurface", priority = 60)]
pub(super) fn surface_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    paint_factory_surface(&mut painter, model);
}

fn paint_star(painter: &mut PlayPainter<'_, '_>, center: Point, radius: Fixed, color: Color) {
    if radius <= Fixed::ZERO {
        return;
    }
    painter
        .transformed(
            Transform::translate(center.x, center.y).compose(&Transform::scale(radius, radius)),
        )
        .fill_path(&STAR, color);
}

/// The card after a finished order: a title band and three stars, the
/// earned ones growing in one after another.
fn paint_result(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    let (x, y, w, h) = CARD;
    painter.fill(Rect::new(x, y, w, h), PANEL, Fixed::ZERO);
    painter.border(Rect::new(x, y, w, h), INK, Fixed::from_int(2), Fixed::ZERO);
    painter.fill(Rect::new(x + 3, y + 3, w - 6, 26), INK, Fixed::ZERO);
    let earned = i32::from(model.stars_earned());
    for index in 0..3 {
        let center = Point::new(x + w / 2 + (index - 1) * 46, y + 60);
        paint_star(painter, center, Fixed::from_int(17), STAR_EMPTY);
        if index < earned {
            paint_star(
                painter,
                center,
                Fixed::from_int(17) * model.star_scale(index as usize),
                ACCENT,
            );
        }
    }
}

#[crate::view(
    component = FactoryModalSurface,
    name = "FactoryModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    if model.modal() == FactoryModal::None {
        return;
    }
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(20, 28, 34, 200),
        Fixed::ZERO,
    );
    if model.modal() == FactoryModal::Result {
        paint_result(&mut painter, model);
        return;
    }
    // A question needs only its two answers; the picker and the manual
    // take the full panel.
    let height = if matches!(model.modal(), FactoryModal::Confirm { .. }) {
        128
    } else {
        220
    };
    painter.fill(Rect::new(25, 56, 430, height), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(25, 56, 430, height),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(29, 60, 422, 32), INK, Fixed::ZERO);
    if model.modal() == FactoryModal::Help {
        painter.line(Point::new(195, 104), Point::new(195, 234), LINE, Fixed::ONE);
    }
}
