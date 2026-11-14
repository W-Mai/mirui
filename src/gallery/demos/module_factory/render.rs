use super::state::{FactoryModalSurface, FactorySurface};
use super::style::{ACCENT, BACKGROUND, CYAN, GREEN, INK, LINE, MUTED, PANEL, WORK};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::factory::{
    CELL_COUNT, FactoryModal, FactoryModel, FactoryPage, GRID_HEIGHT, GRID_WIDTH, MaterialStage,
    ModuleKind,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;

fn paint_shell(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 31), INK, Fixed::ZERO);
    painter.fill(
        Rect::new(0, 31, 480, 28),
        Color::rgb(226, 231, 226),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(0, 282, 480, 38), INK, Fixed::ZERO);
}

/// The work area on the left and the side panel of every page.
fn paint_frame(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(Rect::new(11, 65, 289, 188), WORK, Fixed::ZERO);
    painter.border(Rect::new(11, 65, 289, 188), LINE, Fixed::ONE, Fixed::ZERO);
    painter.fill(Rect::new(309, 65, 160, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(309, 65, 160, 188), LINE, Fixed::ONE, Fixed::ZERO);
}

fn cell_rect(index: usize) -> Rect {
    let x = 18 + (index % GRID_WIDTH) as i32 * 30;
    let y = 72 + (index / GRID_WIDTH) as i32 * 28;
    Rect::new(x, y, 29, 27)
}

fn center(rect: Rect) -> Point {
    Point {
        x: rect.x + rect.w / Fixed::from_int(2),
        y: rect.y + rect.h / Fixed::from_int(2),
    }
}

fn paint_direction(painter: &mut PlayPainter<'_, '_>, rect: Rect, direction: u8, color: Color) {
    let center = center(rect);
    let (dx, dy) = match direction % 4 {
        0 => (8, 0),
        1 => (0, 8),
        2 => (-8, 0),
        _ => (0, -8),
    };
    let end = Point {
        x: center.x + Fixed::from_int(dx),
        y: center.y + Fixed::from_int(dy),
    };
    painter.line(center, end, color, Fixed::from_ratio(3, 2));
    let (ax, ay) = if dx != 0 {
        (-dx.signum() * 4, 4)
    } else {
        (4, -dy.signum() * 4)
    };
    painter.line(
        end,
        Point {
            x: end.x + Fixed::from_int(ax),
            y: end.y + Fixed::from_int(ay),
        },
        color,
        Fixed::from_ratio(3, 2),
    );
    painter.line(
        end,
        Point {
            x: end.x + Fixed::from_int(if dx != 0 { ax } else { -ax }),
            y: end.y + Fixed::from_int(if dx != 0 { -ay } else { ay }),
        },
        color,
        Fixed::from_ratio(3, 2),
    );
}

/// A short orange bar just past the side where a module drops its items
/// into nothing.
fn paint_dead_end(painter: &mut PlayPainter<'_, '_>, rect: Rect, direction: u8) {
    let (x, y) = (rect.x.to_int(), rect.y.to_int());
    let bar = match direction % 4 {
        0 => Rect::new(x + 30, y + 7, 3, 13),
        1 => Rect::new(x + 8, y + 28, 13, 3),
        2 => Rect::new(x - 4, y + 7, 3, 13),
        _ => Rect::new(x + 8, y - 4, 13, 3),
    };
    painter.fill(bar, ACCENT, Fixed::ZERO);
}

fn paint_wall(painter: &mut PlayPainter<'_, '_>, rect: Rect) {
    painter.fill(rect, LINE, Fixed::ZERO);
    let (x, y) = (rect.x.to_int(), rect.y.to_int());
    for offset in [0, 9, 18] {
        painter.line(
            Point::new(x + offset, y + 26),
            Point::new(x + offset + 10, y + 1),
            MUTED,
            Fixed::ONE,
        );
    }
}

fn stage_color(stage: MaterialStage) -> Color {
    match stage {
        MaterialStage::Ore => Color::rgb(156, 112, 73),
        MaterialStage::Plate => Color::rgb(188, 198, 194),
        MaterialStage::Gear => ACCENT,
        MaterialStage::Certified => GREEN,
    }
}

fn paint_grid(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    for x in 0..=GRID_WIDTH {
        let px = 18 + x as i32 * 30;
        painter.line(
            Point::new(px, 72),
            Point::new(px, 240),
            LINE,
            Fixed::from_ratio(1, 2),
        );
    }
    for y in 0..=GRID_HEIGHT {
        let py = 72 + y as i32 * 28;
        painter.line(
            Point::new(18, py),
            Point::new(288, py),
            LINE,
            Fixed::from_ratio(1, 2),
        );
    }
    for index in 0..CELL_COUNT {
        let rect = cell_rect(index);
        let inner = Rect {
            x: rect.x + Fixed::ONE,
            y: rect.y + Fixed::ONE,
            w: rect.w - Fixed::from_int(2),
            h: rect.h - Fixed::from_int(2),
        };
        if let Some(cell) = model.cell(index) {
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
                    Rect::new(rect.x.to_int() + 9, rect.y.to_int() + 7, 11, 13),
                    PANEL,
                    Fixed::ONE,
                    Fixed::ZERO,
                );
            }
        }
        if let Some(item) = model.item(index) {
            painter.circle(center(rect), Fixed::from_int(5), PANEL);
            painter.circle(center(rect), Fixed::from_int(4), stage_color(item.stage));
        }
    }
    for index in 0..CELL_COUNT {
        if let Some(cell) = model.cell(index)
            && model.dead_end(index)
        {
            paint_dead_end(painter, cell_rect(index), cell.direction);
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
    painter.fill(Rect::new(319, y, 139, 3), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(319, y, 139 * value.min(max) / max, 3),
        if value > max { ACCENT } else { color },
        Fixed::ZERO,
    );
}

fn paint_line_page(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    paint_frame(painter);
    paint_grid(painter, model);
    let order = model.mission();
    let goal = i32::from(order.goal.max(1));
    painter.fill(Rect::new(319, 115, 139, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(
            319,
            115,
            139 * i32::from(model.delivered().min(order.goal)) / goal,
            4,
        ),
        ACCENT,
        Fixed::ZERO,
    );
    paint_meter(
        painter,
        142,
        i32::from(model.cost()),
        i32::from(order.budget),
        CYAN,
    );
    paint_meter(
        painter,
        167,
        i32::from(model.power()),
        i32::from(order.power),
        GREEN,
    );
    painter.line(Point::new(318, 178), Point::new(458, 178), LINE, Fixed::ONE);
}

fn paint_orders_page(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    paint_frame(painter);
    let current = i32::from(model.mission_index());
    painter.fill(
        Rect::new(13 + current % 2 * 142, 70 + current / 2 * 46, 3, 40),
        ACCENT,
        Fixed::ZERO,
    );
    painter.line(Point::new(319, 146), Point::new(459, 146), LINE, Fixed::ONE);
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
    paint_chart(painter, model, Rect::new(16, 88, 279, 76), false, ACCENT);
    paint_chart(painter, model, Rect::new(16, 192, 279, 54), true, CYAN);
    for row in 1..4 {
        let y = 65 + row * 47;
        painter.line(Point::new(319, y), Point::new(459, y), LINE, Fixed::ONE);
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

#[crate::view(
    component = FactoryModalSurface,
    name = "FactoryModalSurface",
    priority = 70
)]
pub(super) fn modal_render(model: model!(component.model, watch(modal)), paint: paint!()) {
    if model.modal() == FactoryModal::None {
        return;
    }
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(20, 28, 34, 220),
        Fixed::ZERO,
    );
    // A question needs only its two answers; the picker and the manual
    // take the full panel.
    let height = if matches!(model.modal(), FactoryModal::Confirm { .. }) {
        128
    } else {
        209
    };
    painter.fill(Rect::new(25, 62, 430, height), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(25, 62, 430, height),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(29, 66, 422, 32), INK, Fixed::ZERO);
    if model.modal() == FactoryModal::Help {
        painter.line(Point::new(195, 108), Point::new(195, 250), LINE, Fixed::ONE);
    }
}
