extern crate alloc;

use alloc::format;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::factory::{
    CELL_COUNT, FactoryError, FactoryModal, FactoryModel, FactoryPage, FactoryStatus, FactoryTool,
    GRID_HEIGHT, GRID_WIDTH, MISSIONS, MaterialStage, ModuleKind,
};
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, Hidden};

pub const VIEWPORT: (u16, u16) = (480, 320);

const INK: Color = Color::rgb(32, 41, 50);
const BACKGROUND: Color = Color::rgb(237, 238, 232);
const PANEL: Color = Color::rgb(252, 252, 246);
const WORK: Color = Color::rgb(229, 233, 227);
const LINE: Color = Color::rgb(201, 207, 205);
const MUTED: Color = Color::rgb(116, 128, 138);
const ACCENT: Color = Color::rgb(245, 107, 56);
const CYAN: Color = Color::rgb(67, 145, 157);
const GREEN: Color = Color::rgb(88, 132, 92);

#[derive(crate::Component, Default)]
struct FactorySurface;

#[derive(crate::Component, Default)]
struct FactoryModalSurface;

#[derive(Clone, Copy)]
struct FactoryNodes {
    surface: Entity,
    order_meta: Entity,
    power_meta: Entity,
    tick_meta: Entity,
    tabs: [Entity; 3],
    pages: [Entity; 3],
    line_values: [Entity; 7],
    order_buttons: [Entity; 3],
    order_descriptions: [Entity; 3],
    telemetry_values: [Entity; 5],
    footer: [Entity; 5],
    modal: Entity,
    modal_title: Entity,
    modal_subtitle: Entity,
    modal_buttons: [Entity; 6],
}

impl FactoryNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut FactoryModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<FactoryModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes.contains(ChangeSet::VISUAL)
            && let Some(surface) = world.resource::<Self>().map(|nodes| nodes.surface)
        {
            world.invalidate_visual(surface);
        }
        if changes.contains(ChangeSet::MODEL) || changes.contains(ChangeSet::LAYOUT) {
            Self::sync(world);
        }
    }

    fn result(
        world: &mut World,
        update: impl FnOnce(&mut FactoryModel) -> Result<ChangeSet, FactoryError>,
    ) {
        Self::update(world, |model| update(model).unwrap_or(ChangeSet::VISUAL));
    }

    fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<FactoryModel>() else {
            return;
        };
        let page = model.page();
        let modal = model.modal();
        let tool = model.tool();
        let mission_index = model.mission_index();
        let mission = model.mission();
        let power = model.power();
        let cost = model.cost();
        let selected = model.cell(model.selected());
        let selected_index = model.selected();
        let status = model.status();
        let running = model.running();
        let history_len = model.history_len();
        let values = [
            model.tick(),
            model.produced(),
            model.delivered(),
            model.rejected(),
            u16::from(model.wip()),
        ];
        let line_values = [
            format!("{} / {}", model.delivered(), mission.goal),
            format!("{} / {}", cost, mission.budget),
            selected.map_or("待建造空位".into(), |cell| cell.kind.label().into()),
            format!(
                "C{} · R{}",
                selected_index % GRID_WIDTH + 1,
                selected_index / GRID_WIDTH + 1
            ),
            status_line(status, model.wip(), model.blocked(), model.rejected()),
            format!("方向 {}", direction_glyph(model.tool_direction())),
            tool_label(model.tool()).into(),
        ];
        let _ = model;

        set_text(
            world,
            nodes.order_meta,
            format!(
                "ORDER 0{} / {}:{}",
                mission_index + 1,
                values[2],
                mission.goal
            ),
        );
        set_text(
            world,
            nodes.power_meta,
            format!("PWR {power}/{}", mission.power),
        );
        set_text(world, nodes.tick_meta, format!("T+{:03}", values[0]));
        for (index, entity) in nodes.tabs.into_iter().enumerate() {
            set_button_state(world, entity, index == page_index(page), true);
        }
        for (index, entity) in nodes.pages.into_iter().enumerate() {
            set_hidden(world, entity, index != page_index(page));
        }
        for (entity, value) in nodes.line_values.into_iter().zip(line_values) {
            set_text(world, entity, value);
        }
        for (index, entity) in nodes.order_buttons.into_iter().enumerate() {
            set_text(
                world,
                entity,
                format!("0{}  {}", index + 1, MISSIONS[index].name),
            );
            set_button_state(world, entity, index == usize::from(mission_index), true);
        }
        for (index, entity) in nodes.order_descriptions.into_iter().enumerate() {
            set_text(
                world,
                entity,
                format!(
                    "{} 件 / 预算 {}\n{}",
                    MISSIONS[index].goal, MISSIONS[index].budget, MISSIONS[index].description
                ),
            );
        }
        for (entity, value) in nodes.telemetry_values.into_iter().zip(values) {
            set_text(world, entity, format!("{value}"));
        }

        let footer_labels = [
            "＋ 建造",
            "旋转 ↻",
            "撤销",
            "单步",
            if running { "Ⅱ 暂停" } else { "▶ 运行" },
        ];
        for (index, entity) in nodes.footer.into_iter().enumerate() {
            set_text(world, entity, footer_labels[index]);
            let enabled = match index {
                1 => {
                    selected.is_some_and(|cell| {
                        !matches!(cell.kind, ModuleKind::Source | ModuleKind::Dock)
                    }) || tool != FactoryTool::Select
                }
                2 => history_len > 0,
                3 | 4 => power <= mission.power,
                _ => true,
            };
            set_button_state(
                world,
                entity,
                index == 4 || (index == 0 && modal == FactoryModal::Tools),
                enabled,
            );
        }

        set_hidden(world, nodes.modal, modal == FactoryModal::None);
        let (title, subtitle) = match modal {
            FactoryModal::None => ("", ""),
            FactoryModal::Tools => ("建造模块", "选择工具后点击格子；修改布局会重置试运行。"),
            FactoryModal::Confirm {
                reference: true, ..
            } => ("装载示范布局？", "当前产线、进度与撤销记录将被替换。"),
            FactoryModal::Confirm {
                reference: false, ..
            } => ("切换订单？", "当前产线、进度与撤销记录将被替换。"),
            FactoryModal::Help => (
                "模块工厂 / 操作手册",
                "矿石经熔炼和装配后交付；质检订单还需要通过质检台。",
            ),
        };
        set_text(world, nodes.modal_title, title);
        set_text(world, nodes.modal_subtitle, subtitle);
        for (index, entity) in nodes.modal_buttons.into_iter().enumerate() {
            let (label, visible, active) = match modal {
                FactoryModal::Tools => match index {
                    0 => ("选择 / 检查", true, matches!(tool, FactoryTool::Select)),
                    1 => (
                        "传送带 · 1 信用",
                        true,
                        matches!(tool, FactoryTool::Build(ModuleKind::Belt)),
                    ),
                    2 => (
                        "熔炼炉 · 4 / 3P",
                        true,
                        matches!(tool, FactoryTool::Build(ModuleKind::Furnace)),
                    ),
                    3 => (
                        "装配机 · 5 / 4P",
                        true,
                        matches!(tool, FactoryTool::Build(ModuleKind::Assembler)),
                    ),
                    4 => (
                        "质检台 · 3 / 2P",
                        true,
                        matches!(tool, FactoryTool::Build(ModuleKind::Inspector)),
                    ),
                    _ => ("拆除模块", true, matches!(tool, FactoryTool::Erase)),
                },
                FactoryModal::Confirm { .. } => match index {
                    0 => ("取消", true, false),
                    1 => ("确认装载", true, true),
                    _ => ("", false, false),
                },
                FactoryModal::Help => (
                    if index == 0 { "明白了" } else { "" },
                    index == 0,
                    index == 0,
                ),
                FactoryModal::None => ("", false, false),
            };
            set_hidden(world, entity, !visible);
            if visible {
                set_text(world, entity, label);
                set_button_state(world, entity, active, true);
            }
        }
    }
}

fn page_index(page: FactoryPage) -> usize {
    match page {
        FactoryPage::Line => 0,
        FactoryPage::Orders => 1,
        FactoryPage::Telemetry => 2,
    }
}

fn direction_glyph(direction: u8) -> &'static str {
    match direction % 4 {
        0 => "→",
        1 => "↓",
        2 => "←",
        _ => "↑",
    }
}

fn tool_label(tool: FactoryTool) -> &'static str {
    match tool {
        FactoryTool::Select => "选择建造模块",
        FactoryTool::Build(kind) => kind.label(),
        FactoryTool::Erase => "拆除模块",
    }
}

fn status_line(
    status: FactoryStatus,
    wip: u8,
    blocked: u8,
    rejected: u16,
) -> alloc::string::String {
    match status {
        FactoryStatus::Won => "✓ 订单完成！可在订单页选择下一条产线。".into(),
        FactoryStatus::Timeout => "试运行到期；请修改布局后重试。".into(),
        FactoryStatus::Running => format!("生产中 / 在制 {wip} / 堵塞 {blocked} / 退回 {rejected}"),
        FactoryStatus::Ready | FactoryStatus::Paused => {
            format!("已暂停 / 在制 {wip} / 堵塞 {blocked} / 退回 {rejected}")
        }
    }
}

fn set_text(world: &mut World, entity: Entity, content: impl Into<alloc::string::String>) {
    if let Some(text) = world.get_mut::<Text>(entity) {
        text.set_content(content.into());
    }
    world.invalidate(entity);
}

fn set_hidden(world: &mut World, entity: Entity, hidden: bool) {
    if hidden {
        if !world.has::<Hidden>(entity) {
            world.insert(entity, Hidden);
        }
    } else {
        world.remove::<Hidden>(entity);
    }
    world.invalidate(entity);
}

fn set_button_state(world: &mut World, entity: Entity, active: bool, enabled: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if active {
            ACCENT.into()
        } else if enabled {
            INK.into()
        } else {
            LINE.into()
        };
        button.pressed_color = ACCENT.into();
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active || enabled {
            BACKGROUND.into()
        } else {
            MUTED.into()
        };
    }
    world.invalidate_visual(entity);
}

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

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<FactoryModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_factory_surface(&mut painter, model);
}

fn modal_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<FactoryModel>() else {
        return;
    };
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

fn surface_view() -> View {
    View::new("FactorySurface", 60, surface_render).with_filter::<FactorySurface>()
}

fn modal_view() -> View {
    View::new("FactoryModalSurface", 70, modal_render).with_filter::<FactoryModalSurface>()
}

fn local_point(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<Point> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some(Point {
        x: (x - rect.x) * Fixed::from_int(480) / rect.w,
        y: (y - rect.y) * Fixed::from_int(320) / rect.h,
    })
}

fn tap_surface(model: &mut FactoryModel, point: Point) -> ChangeSet {
    if model.modal() != FactoryModal::None {
        return ChangeSet::NONE;
    }
    if model.page() == FactoryPage::Line {
        let x = point.x.to_int();
        let y = point.y.to_int();
        if (18..288).contains(&x) && (72..240).contains(&y) {
            let column = ((x - 18) / 30) as usize;
            let row = ((y - 72) / 28) as usize;
            return model
                .select_or_apply(row * GRID_WIDTH + column)
                .unwrap_or(ChangeSet::VISUAL);
        }
    }
    ChangeSet::NONE
}

fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(point) = local_point(world, entity, *x, *y) else {
        return false;
    };
    FactoryNodes::update(world, |model| tap_surface(model, point));
    true
}

fn footer_action(world: &mut World, index: usize) {
    match index {
        0 => FactoryNodes::update(world, |model| model.open_modal(FactoryModal::Tools)),
        1 => {
            let tool = world
                .resource::<FactoryModel>()
                .map_or(FactoryTool::Select, FactoryModel::tool);
            if tool == FactoryTool::Select {
                FactoryNodes::result(world, FactoryModel::rotate_selected);
            } else {
                FactoryNodes::update(world, FactoryModel::rotate_tool);
            }
        }
        2 => FactoryNodes::update(world, FactoryModel::undo),
        3 => FactoryNodes::result(world, FactoryModel::step_once),
        _ => FactoryNodes::result(world, FactoryModel::toggle_run),
    }
}

fn modal_action(world: &mut World, index: usize) {
    let modal = world
        .resource::<FactoryModel>()
        .map_or(FactoryModal::None, FactoryModel::modal);
    match modal {
        FactoryModal::Tools => {
            let tool = match index {
                0 => FactoryTool::Select,
                1 => FactoryTool::Build(ModuleKind::Belt),
                2 => FactoryTool::Build(ModuleKind::Furnace),
                3 => FactoryTool::Build(ModuleKind::Assembler),
                4 => FactoryTool::Build(ModuleKind::Inspector),
                _ => FactoryTool::Erase,
            };
            FactoryNodes::update(world, |model| model.set_tool(tool));
        }
        FactoryModal::Confirm { .. } if index == 0 => {
            FactoryNodes::update(world, FactoryModel::close_modal);
        }
        FactoryModal::Confirm { .. } if index == 1 => {
            FactoryNodes::update(world, FactoryModel::confirm_mission);
        }
        FactoryModal::Help => FactoryNodes::update(world, FactoryModel::close_modal),
        _ => {}
    }
}

#[mirui_macros::system(order = ANIMATION)]
fn factory_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    FactoryNodes::update(world, |model| model.advance_ms(elapsed));
}

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

#[compose]
fn build_widgets() {
    ui! {
        FactorySurface (id: "factory_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "模块工厂",
                position: Position::Absolute,
                left: 24,
                top: 6,
                width: 170,
                height: 20,
                font_size: 14,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                "ORDER 01 / 0:8",
                id: "factory_order_meta",
                position: Position::Absolute,
                left: 304,
                top: 8,
                width: 135,
                height: 16,
                font_size: 9,
                text_color: Color::rgb(174, 186, 190),
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Button (
                "?",
                position: Position::Absolute,
                left: 451,
                top: 4,
                width: 24,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 12,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { FactoryNodes::update(ctx.world, |model| model.open_modal(FactoryModal::Help)); }
            Button (
                "产线",
                id: "factory_tab_line",
                position: Position::Absolute,
                left: 24,
                top: 33,
                width: 96,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACCENT,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { FactoryNodes::update(ctx.world, |model| model.set_page(FactoryPage::Line)); }
            Button (
                "订单",
                id: "factory_tab_orders",
                position: Position::Absolute,
                left: 124,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { FactoryNodes::update(ctx.world, |model| model.set_page(FactoryPage::Orders)); }
            Button (
                "遥测",
                id: "factory_tab_telemetry",
                position: Position::Absolute,
                left: 210,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { FactoryNodes::update(ctx.world, |model| model.set_page(FactoryPage::Telemetry)); }
            Text (
                "PWR 7/9",
                id: "factory_power_meta",
                position: Position::Absolute,
                left: 302,
                top: 39,
                width: 80,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                "T+000",
                id: "factory_tick_meta",
                position: Position::Absolute,
                left: 394,
                top: 39,
                width: 63,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            View (
                id: "factory_line_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "PRODUCTION ORDER",
                    position: Position::Absolute,
                    left: 318,
                    top: 76,
                    width: 140,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "0 / 8",
                    id: "factory_delivered",
                    position: Position::Absolute,
                    left: 319,
                    top: 88,
                    width: 139,
                    height: 27,
                    font_size: 20,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "BUILD CREDITS",
                    position: Position::Absolute,
                    left: 318,
                    top: 128,
                    width: 140,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "13 / 23",
                    id: "factory_cost",
                    position: Position::Absolute,
                    left: 388,
                    top: 128,
                    width: 68,
                    height: 15,
                    font_size: 10,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "SELECTED MODULE",
                    position: Position::Absolute,
                    left: 318,
                    top: 169,
                    width: 140,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "待建造空位",
                    id: "factory_selected",
                    position: Position::Absolute,
                    left: 318,
                    top: 186,
                    width: 138,
                    height: 23,
                    font_size: 13,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "C6 · R3",
                    id: "factory_selected_coord",
                    position: Position::Absolute,
                    left: 388,
                    top: 189,
                    width: 68,
                    height: 15,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "已暂停 / 在制 0 / 堵塞 0 / 退回 0",
                    id: "factory_status",
                    position: Position::Absolute,
                    left: 16,
                    top: 259,
                    width: 447,
                    height: 18,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "方向 →",
                    id: "factory_direction",
                    position: Position::Absolute,
                    left: 318,
                    top: 213,
                    width: 65,
                    height: 15,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "选择建造模块",
                    id: "factory_tool",
                    position: Position::Absolute,
                    left: 318,
                    top: 232,
                    width: 140,
                    height: 15,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
            }
            View (
                id: "factory_orders_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Button (
                    "01  微型装配",
                    id: "factory_order_0",
                    position: Position::Absolute,
                    left: 20,
                    top: 92,
                    width: 276,
                    height: 45,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { FactoryNodes::update(ctx.world, |model| model.request_mission(0, false)); }
                Button (
                    "02  折返产线",
                    id: "factory_order_1",
                    position: Position::Absolute,
                    left: 20,
                    top: 148,
                    width: 276,
                    height: 45,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { FactoryNodes::update(ctx.world, |model| model.request_mission(1, false)); }
                Button (
                    "03  质量检验",
                    id: "factory_order_2",
                    position: Position::Absolute,
                    left: 20,
                    top: 204,
                    width: 276,
                    height: 45,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { FactoryNodes::update(ctx.world, |model| model.request_mission(2, false)); }
                Text (
                    "",
                    id: "factory_order_desc_0",
                    position: Position::Absolute,
                    left: 98,
                    top: 97,
                    width: 188,
                    height: 35,
                    font_size: 7,
                    line_height: 11,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "factory_order_desc_1",
                    position: Position::Absolute,
                    left: 98,
                    top: 153,
                    width: 188,
                    height: 35,
                    font_size: 7,
                    line_height: 11,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "factory_order_desc_2",
                    position: Position::Absolute,
                    left: 98,
                    top: 209,
                    width: 188,
                    height: 35,
                    font_size: 7,
                    line_height: 11,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "MATERIAL FLOW",
                    position: Position::Absolute,
                    left: 327,
                    top: 78,
                    width: 120,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "矿石 / ORE\n↓ 熔炼 3 tick\n板材 / PLATE\n↓ 装配 4 tick\n零件 / GEAR\n↓ 质检 2 tick\n合格品 / CERT",
                    position: Position::Absolute,
                    left: 327,
                    top: 94,
                    width: 125,
                    height: 108,
                    font_size: 8,
                    line_height: 15,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "装载示范布局",
                    id: "factory_reference",
                    position: Position::Absolute,
                    left: 327,
                    top: 216,
                    width: 124,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: ACCENT,
                    pressed_color: INK,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap {
                    let mission = ctx
                        .world
                        .resource::<FactoryModel>()
                        .map_or(0, FactoryModel::mission_index);
                    FactoryNodes::update(ctx.world, |model| model.request_mission(mission, true));
                }
            }
            View (
                id: "factory_telemetry_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "DELIVERED / LAST 80 TICKS",
                    position: Position::Absolute,
                    left: 18,
                    top: 72,
                    width: 280,
                    height: 14,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "BLOCKED CELLS",
                    position: Position::Absolute,
                    left: 18,
                    top: 180,
                    width: 280,
                    height: 14,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "试运行 tick",
                    position: Position::Absolute,
                    left: 327,
                    top: 83,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "累计投料",
                    position: Position::Absolute,
                    left: 327,
                    top: 112,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "正确交付",
                    position: Position::Absolute,
                    left: 327,
                    top: 141,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "退回产品",
                    position: Position::Absolute,
                    left: 327,
                    top: 170,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "当前在制",
                    position: Position::Absolute,
                    left: 327,
                    top: 199,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "0",
                    id: "factory_telemetry_tick",
                    position: Position::Absolute,
                    left: 419,
                    top: 81,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "0",
                    id: "factory_telemetry_produced",
                    position: Position::Absolute,
                    left: 419,
                    top: 110,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "0",
                    id: "factory_telemetry_delivered",
                    position: Position::Absolute,
                    left: 419,
                    top: 139,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "0",
                    id: "factory_telemetry_rejected",
                    position: Position::Absolute,
                    left: 419,
                    top: 168,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "0",
                    id: "factory_telemetry_wip",
                    position: Position::Absolute,
                    left: 419,
                    top: 197,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "曲线来自真实模型状态，不是设备性能数据。",
                    position: Position::Absolute,
                    left: 16,
                    top: 259,
                    width: 447,
                    height: 18,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
            }
            Button (
                "＋ 建造",
                id: "factory_footer_0",
                position: Position::Absolute,
                left: 8,
                top: 287,
                width: 88,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 0); }
            Button (
                "旋转 ↻",
                id: "factory_footer_1",
                position: Position::Absolute,
                left: 101,
                top: 287,
                width: 88,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 1); }
            Button (
                "撤销",
                id: "factory_footer_2",
                position: Position::Absolute,
                left: 194,
                top: 287,
                width: 83,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 2); }
            Button (
                "单步",
                id: "factory_footer_3",
                position: Position::Absolute,
                left: 282,
                top: 287,
                width: 82,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 3); }
            Button (
                "▶ 运行",
                id: "factory_footer_4",
                position: Position::Absolute,
                left: 369,
                top: 287,
                width: 103,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ACCENT,
                pressed_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 4); }
            FactoryModalSurface (
                id: "factory_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320
            ) on Tap { }
            {
                Text (
                    "建造模块",
                    id: "factory_modal_title",
                    position: Position::Absolute,
                    left: 43,
                    top: 72,
                    width: 330,
                    height: 20,
                    font_size: 14,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "factory_modal_subtitle",
                    position: Position::Absolute,
                    left: 43,
                    top: 107,
                    width: 386,
                    height: 28,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Button (
                    "选择 / 检查",
                    id: "factory_modal_0",
                    position: Position::Absolute,
                    left: 43,
                    top: 137,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 0); }
                Button (
                    "传送带",
                    id: "factory_modal_1",
                    position: Position::Absolute,
                    left: 249,
                    top: 137,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 1); }
                Button (
                    "熔炼炉",
                    id: "factory_modal_2",
                    position: Position::Absolute,
                    left: 43,
                    top: 180,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 2); }
                Button (
                    "装配机",
                    id: "factory_modal_3",
                    position: Position::Absolute,
                    left: 249,
                    top: 180,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 3); }
                Button (
                    "质检台",
                    id: "factory_modal_4",
                    position: Position::Absolute,
                    left: 43,
                    top: 223,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 4); }
                Button (
                    "拆除模块",
                    id: "factory_modal_5",
                    position: Position::Absolute,
                    left: 249,
                    top: 223,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 5); }
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 414,
                    top: 70,
                    width: 24,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { FactoryNodes::update(ctx.world, FactoryModel::close_modal); }
            }
        }
    };
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    register_play_font(&mut app.world);
    app.world.insert_resource(FactoryModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(factory_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Module Factory node");
    let nodes = FactoryNodes {
        surface: find("factory_surface"),
        order_meta: find("factory_order_meta"),
        power_meta: find("factory_power_meta"),
        tick_meta: find("factory_tick_meta"),
        tabs: [
            find("factory_tab_line"),
            find("factory_tab_orders"),
            find("factory_tab_telemetry"),
        ],
        pages: [
            find("factory_line_page"),
            find("factory_orders_page"),
            find("factory_telemetry_page"),
        ],
        line_values: [
            find("factory_delivered"),
            find("factory_cost"),
            find("factory_selected"),
            find("factory_selected_coord"),
            find("factory_status"),
            find("factory_direction"),
            find("factory_tool"),
        ],
        order_buttons: [
            find("factory_order_0"),
            find("factory_order_1"),
            find("factory_order_2"),
        ],
        order_descriptions: [
            find("factory_order_desc_0"),
            find("factory_order_desc_1"),
            find("factory_order_desc_2"),
        ],
        telemetry_values: [
            find("factory_telemetry_tick"),
            find("factory_telemetry_produced"),
            find("factory_telemetry_delivered"),
            find("factory_telemetry_rejected"),
            find("factory_telemetry_wip"),
        ],
        footer: [
            find("factory_footer_0"),
            find("factory_footer_1"),
            find("factory_footer_2"),
            find("factory_footer_3"),
            find("factory_footer_4"),
        ],
        modal: find("factory_modal"),
        modal_title: find("factory_modal_title"),
        modal_subtitle: find("factory_modal_subtitle"),
        modal_buttons: [
            find("factory_modal_0"),
            find("factory_modal_1"),
            find("factory_modal_2"),
            find("factory_modal_3"),
            find("factory_modal_4"),
            find("factory_modal_5"),
        ],
    };
    app.world.insert_resource(nodes);
    FactoryNodes::sync(&mut app.world);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_uses_one_dense_surface_and_semantic_controls() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert!(app.world.find_by_id("factory_surface").is_some());
        assert!(app.world.find_by_id("factory_footer_4").is_some());
        assert!(app.world.find_by_id("factory_reference").is_some());
        assert_eq!(app.world.query::<FactorySurface>().iter().count(), 1);
    }

    #[test]
    fn surface_places_selected_tool_without_per_cell_widgets() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        app.systems.run_all(&mut app.world);
        let surface = app.world.find_by_id("factory_surface").unwrap();
        app.world
            .insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
        FactoryNodes::update(&mut app.world, |model| {
            model.set_tool(FactoryTool::Build(ModuleKind::Belt))
        });
        assert!(surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::Tap {
                x: Fixed::from_int(183),
                y: Fixed::from_int(142),
                target: surface,
            }
        ));
        assert_eq!(
            app.world
                .resource::<FactoryModel>()
                .unwrap()
                .cell(23)
                .unwrap()
                .kind,
            ModuleKind::Belt
        );
    }
}
