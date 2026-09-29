use super::state::{FactoryModalSurface, FactorySurface};
use super::style::{ACCENT, BACKGROUND, CYAN, GREEN, INK, LINE, MUTED, PANEL, WORK};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::factory::{
    CELL_COUNT, FactoryModal, FactoryModel, FactoryPage, GRID_HEIGHT, GRID_WIDTH, MaterialStage,
    ModuleKind,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::ViewCtx;

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

fn cell_rect(index: usize) -> Rect {
    let x = 18 + (index % GRID_WIDTH) as i32 * 30;
    let y = 72 + (index / GRID_WIDTH) as i32 * 28;
    Rect::new(x, y, 29, 27)
}

fn paint_direction(painter: &mut PlayPainter<'_, '_>, rect: Rect, direction: u8, color: Color) {
    let center = Point {
        x: rect.x + rect.w / Fixed::from_int(2),
        y: rect.y + rect.h / Fixed::from_int(2),
    };
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

fn paint_line_page(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    painter.fill(Rect::new(11, 65, 288, 188), WORK, Fixed::ZERO);
    painter.border(Rect::new(11, 65, 288, 188), LINE, Fixed::ONE, Fixed::ZERO);
    painter.fill(Rect::new(308, 66, 161, 186), PANEL, Fixed::ZERO);
    painter.border(Rect::new(308, 66, 161, 186), LINE, Fixed::ONE, Fixed::ZERO);
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
        if index == model.selected() {
            painter.border(rect, ACCENT, Fixed::from_int(2), Fixed::ZERO);
        }
        let Some(cell) = model.cell(index) else {
            continue;
        };
        let fill = match cell.kind {
            ModuleKind::Source | ModuleKind::Dock => INK,
            ModuleKind::Furnace => CYAN,
            ModuleKind::Assembler => ACCENT,
            ModuleKind::Inspector => GREEN,
            ModuleKind::Belt => PANEL,
        };
        painter.fill(
            Rect {
                x: rect.x + Fixed::ONE,
                y: rect.y + Fixed::ONE,
                w: rect.w - Fixed::from_int(2),
                h: rect.h - Fixed::from_int(2),
            },
            fill,
            Fixed::ZERO,
        );
        if !matches!(cell.kind, ModuleKind::Dock) {
            paint_direction(
                painter,
                rect,
                cell.direction,
                if matches!(cell.kind, ModuleKind::Belt) {
                    MUTED
                } else {
                    PANEL
                },
            );
        }
        if matches!(
            cell.kind,
            ModuleKind::Furnace | ModuleKind::Assembler | ModuleKind::Inspector
        ) {
            painter.border(
                Rect::new(rect.x.to_int() + 9, rect.y.to_int() + 7, 11, 13),
                PANEL,
                Fixed::ONE,
                Fixed::ZERO,
            );
        }
        if let Some(item) = model.item(index) {
            let color = match item.stage {
                MaterialStage::Ore => Color::rgb(156, 112, 73),
                MaterialStage::Plate => Color::rgb(188, 198, 194),
                MaterialStage::Gear => ACCENT,
                MaterialStage::Certified => GREEN,
            };
            painter.circle(
                Point {
                    x: rect.x + rect.w / Fixed::from_int(2),
                    y: rect.y + rect.h / Fixed::from_int(2),
                },
                Fixed::from_int(4),
                color,
            );
        }
    }
    let goal = i32::from(model.mission().goal.max(1));
    painter.fill(Rect::new(319, 116, 139, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(
            319,
            116,
            139 * i32::from(model.delivered().min(model.mission().goal)) / goal,
            4,
        ),
        ACCENT,
        Fixed::ZERO,
    );
    let budget = i32::from(model.mission().budget.max(1));
    painter.fill(Rect::new(319, 154, 139, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(319, 154, 139 * i32::from(model.cost()) / budget, 4),
        CYAN,
        Fixed::ZERO,
    );
}

fn paint_orders_page(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    painter.fill(Rect::new(11, 65, 289, 188), WORK, Fixed::ZERO);
    painter.fill(Rect::new(309, 65, 160, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(309, 65, 160, 188), LINE, Fixed::ONE, Fixed::ZERO);
    let target_y = 92 + i32::from(model.mission_index()) * 56;
    painter.fill(Rect::new(16, target_y, 4, 45), ACCENT, Fixed::ZERO);
    painter.line(Point::new(327, 115), Point::new(447, 115), LINE, Fixed::ONE);
    painter.line(Point::new(327, 146), Point::new(447, 146), LINE, Fixed::ONE);
    painter.line(Point::new(327, 177), Point::new(447, 177), LINE, Fixed::ONE);
    painter.line(Point::new(327, 208), Point::new(447, 208), LINE, Fixed::ONE);
}

fn paint_chart(
    painter: &mut PlayPainter<'_, '_>,
    model: &FactoryModel,
    rect: Rect,
    blocked: bool,
    color: Color,
    max_value: u16,
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
    let max_value = i32::from(max_value.max(1));
    let mut previous = None;
    for index in 0..len {
        let Some(sample) = model.telemetry(index) else {
            continue;
        };
        let value = if blocked {
            i32::from(sample.blocked)
        } else {
            i32::from(sample.delivered)
        };
        let point = Point {
            x: rect.x + rect.w * Fixed::from_int(index as i32) / Fixed::from_int((len - 1) as i32),
            y: rect.y + rect.h
                - rect.h * Fixed::from_int(value.min(max_value)) / Fixed::from_int(max_value),
        };
        if let Some(from) = previous {
            painter.line(from, point, color, Fixed::from_ratio(3, 2));
        }
        previous = Some(point);
    }
}

fn paint_telemetry_page(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    paint_chart(
        painter,
        model,
        Rect::new(16, 91, 286, 78),
        false,
        ACCENT,
        model.mission().goal,
    );
    paint_chart(painter, model, Rect::new(16, 198, 286, 50), true, CYAN, 6);
    painter.fill(Rect::new(315, 66, 153, 186), PANEL, Fixed::ZERO);
    painter.border(Rect::new(315, 66, 153, 186), LINE, Fixed::ONE, Fixed::ZERO);
}

fn paint_factory_surface(painter: &mut PlayPainter<'_, '_>, model: &FactoryModel) {
    paint_shell(painter);
    match model.page() {
        FactoryPage::Line => paint_line_page(painter, model),
        FactoryPage::Orders => paint_orders_page(painter, model),
        FactoryPage::Telemetry => paint_telemetry_page(painter, model),
    }
}

#[crate::view(
    component = FactorySurface,
    read(model),
    watch(model.visual_revision()),
    name = "FactorySurface",
    priority = 60
)]
pub(super) fn surface_render(
    renderer: &mut dyn Renderer,
    model: &FactoryModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_factory_surface(&mut painter, model);
}

#[crate::view(
    component = FactoryModalSurface,
    read(model),
    watch(model.modal()),
    name = "FactoryModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    renderer: &mut dyn Renderer,
    model: &FactoryModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if model.modal() == FactoryModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(20, 28, 34, 220),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(25, 62, 430, 209), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(25, 62, 430, 209),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(29, 66, 422, 32), INK, Fixed::ZERO);
}
