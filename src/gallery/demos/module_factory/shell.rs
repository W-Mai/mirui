use super::input::{factory_tick_system, keyboard, surface_tap};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::{modal_render, surface_render, tile_left, tile_top};
use super::state::{FactoryModalSurface, FactorySurface};
use super::style::{
    ACCENT, BACKGROUND, CELL, DIM, GRID_LEFT, HEADER_BUTTON, INK, LINE, MUTED, PRIMARY_TOP,
    SIDE_LEFT, SIDE_WIDTH, TILE_HEIGHT, TILE_WIDTH, TOOL_TOP, TOOLS_TOP,
};
use super::text::{
    HELP_CONTROLS, HELP_FLOW, HELP_KEYS, ModalTitle, ModuleDetail, ModuleName, NextLabel,
    OrderGoal, OrderMeta, OrderNumber, OrderSpecs, OrderTitle, ResultLine, ResultTitle, RunLabel,
    SPEC_NAMES, StarRow, StatusText, TITLE, ToolLabel, modal_button_label, modal_subtitle,
};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::factory::{
    FactoryModal, FactoryModel, FactoryPage, FactoryTool, GRID_WIDTH, ModuleKind,
};
use crate::gallery::play::font::GameFontPlugin;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

fn end_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::End)
}

fn centered() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Center)
}

/// A control that cannot act looks idle even when it is the highlighted one.
fn control_color(active: bool, enabled: bool) -> Color {
    if !enabled {
        LINE
    } else if active {
        ACCENT
    } else {
        INK
    }
}

fn control_text_color(enabled: bool) -> Color {
    if enabled { BACKGROUND } else { MUTED }
}

/// A button on the dark header lights up while it is on.
fn header_color(active: bool) -> Color {
    if active { ACCENT } else { HEADER_BUTTON }
}

/// A page shows while it is chosen and no dialog covers it.
fn page_shown(page: FactoryPage, modal: FactoryModal, wanted: FactoryPage) -> bool {
    page == wanted && modal == FactoryModal::None
}

/// Order tiles keep their number and name at the left edge.
const TILE_PADDING: Padding = Padding {
    top: Dimension::px(0),
    right: Dimension::px(0),
    bottom: Dimension::px(0),
    left: Dimension::px(10),
};

const COLUMN_NAMES: [&str; GRID_WIDTH] = ["C1", "C2", "C3", "C4", "C5", "C6", "C7", "C8", "C9"];

fn modal_button_visible(modal: FactoryModal, index: usize) -> bool {
    match modal {
        FactoryModal::Tools => true,
        FactoryModal::Confirm { .. } => index < 2,
        FactoryModal::Help | FactoryModal::None | FactoryModal::Result => false,
    }
}

fn modal_button_active(modal: FactoryModal, tool: FactoryTool, index: usize) -> bool {
    match modal {
        FactoryModal::Tools => match index {
            0 => tool == FactoryTool::Select,
            1 => tool == FactoryTool::Build(ModuleKind::Belt),
            2 => tool == FactoryTool::Build(ModuleKind::Furnace),
            3 => tool == FactoryTool::Build(ModuleKind::Assembler),
            4 => tool == FactoryTool::Build(ModuleKind::Inspector),
            _ => tool == FactoryTool::Erase,
        },
        FactoryModal::Confirm { .. } => index == 1,
        FactoryModal::Help | FactoryModal::None | FactoryModal::Result => false,
    }
}

#[compose]
fn compose_header(model: model!(FactoryModel)) -> Entity {
    ui! {
        Text (
            TITLE,
            position: Position::Absolute,
            left: 8,
            top: 4,
            width: 88,
            height: 18,
            font_size: 14,
            text_color: BACKGROUND,
            paragraph: label_style()
        )
    };
    ui! {
        Button (
            tr!(en : "Line", zh : "产线"),
            id: "factory_tab_line",
            position: Position::Absolute,
            left: 98,
            top: 1,
            width: 42,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.page() == FactoryPage::Line, true) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(FactoryPage::Line); }
    };
    ui! {
        Button (
            tr!(en : "Orders", zh : "订单"),
            id: "factory_tab_orders",
            position: Position::Absolute,
            left: 142,
            top: 1,
            width: 42,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.page() == FactoryPage::Orders, true) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(FactoryPage::Orders); }
    };
    ui! {
        Button (
            tr!(en : "Stats", zh : "遥测"),
            id: "factory_tab_telemetry",
            position: Position::Absolute,
            left: 186,
            top: 1,
            width: 42,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.page() == FactoryPage::Telemetry, true) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(FactoryPage::Telemetry); }
    };
    ui! {
        Text (
            text: ${ OrderMeta(model.mission_index()) },
            capacity: 40,
            id: "factory_order_meta",
            position: Position::Absolute,
            left: 234,
            top: 6,
            width: 116,
            height: 14,
            font_size: 9,
            text_color: DIM,
            paragraph: label_style()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("T+{:03}/{}", model.tick(), model.mission().limit) },
            capacity: 16,
            id: "factory_tick_meta",
            position: Position::Absolute,
            left: 352,
            top: 7,
            width: 64,
            height: 14,
            font_size: 8,
            text_color: DIM,
            paragraph: end_style()
        )
    };
    ui! {
        Button (
            text: ${ if model.fast() { "×4" } else { "×1" } },
            capacity: 4,
            id: "factory_speed",
            position: Position::Absolute,
            left: 420,
            top: 1,
            width: 30,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ header_color(model.fast()) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.toggle_speed(); }
    };
    ui! {
        Button (
            "?",
            id: "factory_help",
            position: Position::Absolute,
            left: 452,
            top: 1,
            width: 24,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 12,
            normal_color: HEADER_BUTTON,
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.open_modal(FactoryModal::Help); }
    }
}

/// The number over column `index`, which the order briefs refer to.
#[compose]
fn compose_column_name(index: usize) -> Entity {
    ui! {
        Text (
            COLUMN_NAMES[index],
            position: Position::Absolute,
            left: GRID_LEFT + index as i32 * CELL,
            top: 32,
            width: CELL - 1,
            height: 12,
            font_size: 8,
            text_color: MUTED,
            paragraph: centered()
        )
    }
}

#[compose]
fn compose_line_page(model: model!(FactoryModel)) -> Entity {
    ui! {
        View (
            id: "factory_line_page",
            visible: ${ page_shown(model.page(), model.modal(), FactoryPage::Line) },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 296
        ) {
            compose_column_name (0)
            compose_column_name (1)
            compose_column_name (2)
            compose_column_name (3)
            compose_column_name (4)
            compose_column_name (5)
            compose_column_name (6)
            compose_column_name (7)
            compose_column_name (8)
            Text (
                text: ${ model.mission().target.label() },
                capacity: 16,
                id: "factory_target",
                position: Position::Absolute,
                left: SIDE_LEFT + 12,
                top: 34,
                width: SIDE_WIDTH - 12,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} / {}", model.delivered(), model.mission().goal) },
                capacity: 16,
                id: "factory_delivered",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 47,
                width: SIDE_WIDTH,
                height: 26,
                font_size: 20,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                tr!(en : "CREDITS", zh : "信用"),
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 84,
                width: 48,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} / {}", model.cost(), model.mission().budget) },
                capacity: 16,
                id: "factory_cost",
                position: Position::Absolute,
                left: SIDE_LEFT + 48,
                top: 83,
                width: SIDE_WIDTH - 48,
                height: 14,
                font_size: 9,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                tr!(en : "POWER", zh : "功率"),
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 105,
                width: 48,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} / {}", model.power(), model.mission().power) },
                capacity: 16,
                id: "factory_power",
                position: Position::Absolute,
                left: SIDE_LEFT + 48,
                top: 104,
                width: SIDE_WIDTH - 48,
                height: 14,
                font_size: 9,
                text_color: ${ if model.power() > model.mission().power { ACCENT } else { INK } },
                paragraph: end_style()
            )
            Text (
                text: ${ ModuleName(model.selected_cell()) },
                capacity: 24,
                id: "factory_selected",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 132,
                width: SIDE_WIDTH,
                height: 14,
                font_size: 9,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${
                    ModuleDetail {
                        cell: model.selected_cell(),
                        target: model.mission().target,
                        tool: model.tool(),
                    }
                },
                capacity: 48,
                id: "factory_selected_detail",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 149,
                width: SIDE_WIDTH,
                height: 42,
                font_size: 8,
                line_height: 13,
                text_color: MUTED,
                paragraph: ParagraphStyle::default()
            )
        }
    }
}

/// The tool, rotate, undo, step and run under the line and the charts.
#[compose]
fn compose_controls(model: model!(FactoryModel)) -> Entity {
    ui! {
        View (
            id: "factory_controls",
            visible: ${ model.page() != FactoryPage::Orders && model.modal() == FactoryModal::None },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 296
        ) {
            Button (
                text: ${ ToolLabel(model.tool()) },
                capacity: 24,
                id: "factory_tool",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: TOOL_TOP,
                width: SIDE_WIDTH,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(model.tool() != FactoryTool::Select, true) },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.open_modal(FactoryModal::Tools); }
            Button (
                "↻",
                id: "factory_rotate",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: TOOLS_TOP,
                width: 30,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(false, model.can_rotate()) },
                pressed_color: ACCENT,
                text_color: ${ control_text_color(model.can_rotate()) },
                border_radius: 0
            ) on Tap { let _ = model.rotate_active(); }
            Button (
                "↺",
                id: "factory_undo",
                position: Position::Absolute,
                left: SIDE_LEFT + 32,
                top: TOOLS_TOP,
                width: 30,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(false, model.history_len() > 0) },
                pressed_color: ACCENT,
                text_color: ${ control_text_color(model.history_len() > 0) },
                border_radius: 0
            ) on Tap { model.undo(); }
            Button (
                tr!(en : "Step", zh : "单步"),
                id: "factory_step",
                position: Position::Absolute,
                left: SIDE_LEFT + 64,
                top: TOOLS_TOP,
                width: 30,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: ${ control_color(false, model.power() <= model.mission().power) },
                pressed_color: ACCENT,
                text_color: ${ control_text_color(model.power() <= model.mission().power) },
                border_radius: 0
            ) on Tap { let _ = model.step_once(); }
            Button (
                text: ${
                    RunLabel {
                        has_next: model.has_next(),
                        running: model.running(),
                    }
                },
                capacity: 24,
                id: "factory_run",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: PRIMARY_TOP,
                width: SIDE_WIDTH,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(true, model.power() <= model.mission().power) },
                pressed_color: INK,
                text_color: ${ control_text_color(model.power() <= model.mission().power) },
                border_radius: 0
            ) on Tap {
                if model.has_next() {
                    model.next_mission();
                } else {
                    let _ = model.toggle_run();
                }
            }
        }
    }
}

#[compose]
fn compose_order_tile(model: model!(FactoryModel), index: u8) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: tile_left(index),
            top: tile_top(index),
            width: TILE_WIDTH,
            height: TILE_HEIGHT
        ) {
            Button (
                text: ${ OrderTitle(usize::from(index)) },
                capacity: 32,
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: TILE_WIDTH,
                height: TILE_HEIGHT,
                size: ButtonSize::Compact,
                padding: TILE_PADDING,
                paragraph: label_style(),
                font_size: 10,
                normal_color: ${ control_color(model.browse() == index, true) },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.browse_order(index); }
            Text (
                text: ${ StarRow(model.stars()[usize::from(index)]) },
                capacity: 12,
                visible: ${ model.stars()[usize::from(index)] > 0 },
                position: Position::Absolute,
                left: TILE_WIDTH - 44,
                top: 22,
                width: 36,
                height: 14,
                font_size: 8,
                text_color: BACKGROUND,
                paragraph: end_style()
            )
        }
    }
}

#[compose]
fn compose_orders_page(model: model!(FactoryModel)) -> Entity {
    ui! {
        View (
            id: "factory_orders_page",
            visible: ${ page_shown(model.page(), model.modal(), FactoryPage::Orders) },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 296
        ) {
            compose_order_tile (model, 0)
            compose_order_tile (model, 1)
            compose_order_tile (model, 2)
            compose_order_tile (model, 3)
            compose_order_tile (model, 4)
            compose_order_tile (model, 5)
            compose_order_tile (model, 6)
            compose_order_tile (model, 7)
            Text (
                text: ${ OrderNumber(model.browse()) },
                capacity: 16,
                id: "factory_brief_number",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 34,
                width: 50,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ StarRow(model.stars()[usize::from(model.browse())]) },
                capacity: 12,
                id: "factory_brief_stars",
                visible: ${ model.stars()[usize::from(model.browse())] > 0 },
                position: Position::Absolute,
                left: SIDE_LEFT + 50,
                top: 34,
                width: SIDE_WIDTH - 50,
                height: 12,
                font_size: 8,
                text_color: ACCENT,
                paragraph: end_style()
            )
            Text (
                text: ${ model.browsed_mission().name },
                capacity: 32,
                id: "factory_brief_name",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 49,
                width: SIDE_WIDTH,
                height: 14,
                font_size: 10,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ model.browsed_mission().description },
                capacity: 96,
                id: "factory_brief_description",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 66,
                width: SIDE_WIDTH,
                height: 78,
                font_size: 8,
                line_height: 13,
                text_color: MUTED,
                paragraph: ParagraphStyle::default()
            )
            Text (
                text: ${ OrderGoal(model.browse()) },
                capacity: 32,
                id: "factory_brief_goal",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 152,
                width: SIDE_WIDTH,
                height: 14,
                font_size: 8,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                SPEC_NAMES,
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 170,
                width: 44,
                height: 42,
                font_size: 8,
                line_height: 14,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ OrderSpecs(model.browse()) },
                capacity: 48,
                id: "factory_brief_specs",
                position: Position::Absolute,
                left: SIDE_LEFT + 44,
                top: 170,
                width: SIDE_WIDTH - 44,
                height: 42,
                font_size: 8,
                line_height: 14,
                text_color: INK,
                paragraph: end_style()
            )
            Button (
                text: ${
                    if model.browse() == model.mission_index() {
                        tr!(en : "↺ Retry", zh : "↺ 重来")
                    } else {
                        tr!(en : "Start", zh : "开工")
                    }
                },
                capacity: 16,
                id: "factory_start",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: PRIMARY_TOP,
                width: 46,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ACCENT,
                pressed_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.request_browsed(false); }
            Button (
                tr!(en : "Example", zh : "示范布局"),
                id: "factory_reference",
                position: Position::Absolute,
                left: SIDE_LEFT + 48,
                top: PRIMARY_TOP,
                width: 46,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.request_browsed(true); }
        }
    }
}

/// The name of one count of the run beside the charts.
#[compose]
fn compose_stat_name(words: crate::core::i18n::Phrase, top: i32) -> Entity {
    ui! {
        Text (
            words,
            position: Position::Absolute,
            left: SIDE_LEFT,
            top: top + 10,
            width: 50,
            height: 12,
            font_size: 8,
            text_color: MUTED,
            paragraph: label_style()
        )
    }
}

#[compose]
fn compose_telemetry_page(model: model!(FactoryModel)) -> Entity {
    ui! {
        View (
            id: "factory_telemetry_page",
            visible: ${ page_shown(model.page(), model.modal(), FactoryPage::Telemetry) },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 296
        ) {
            Text (
                tr!(en : "DELIVERED / LAST 80 TICKS", zh : "交付 / 最近 80 刻"),
                position: Position::Absolute,
                left: 10,
                top: 35,
                width: 250,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} / {}", model.delivered(), model.mission().goal) },
                capacity: 16,
                id: "factory_chart_delivered",
                position: Position::Absolute,
                left: 270,
                top: 35,
                width: 90,
                height: 12,
                font_size: 8,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                tr!(en : "BLOCKED CELLS", zh : "堵塞格数"),
                position: Position::Absolute,
                left: 10,
                top: 171,
                width: 250,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{}", model.blocked()) },
                capacity: 8,
                id: "factory_chart_blocked",
                position: Position::Absolute,
                left: 270,
                top: 171,
                width: 90,
                height: 12,
                font_size: 8,
                text_color: INK,
                paragraph: end_style()
            )
            compose_stat_name (tr!(en : "Fed", zh : "投料"), 34)
            compose_stat_name (tr!(en : "Shipped", zh : "交付"), 74)
            compose_stat_name (tr!(en : "Sent back", zh : "退回"), 114)
            compose_stat_name (tr!(en : "On line", zh : "在制"), 154)
            Text (
                text: ${ format_args!("{}", model.produced()) },
                capacity: 8,
                id: "factory_telemetry_produced",
                position: Position::Absolute,
                left: SIDE_LEFT + 50,
                top: 38,
                width: SIDE_WIDTH - 50,
                height: 26,
                font_size: 20,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                text: ${ format_args!("{}", model.delivered()) },
                capacity: 8,
                id: "factory_telemetry_delivered",
                position: Position::Absolute,
                left: SIDE_LEFT + 50,
                top: 78,
                width: SIDE_WIDTH - 50,
                height: 26,
                font_size: 20,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                text: ${ format_args!("{}", model.rejected()) },
                capacity: 8,
                id: "factory_telemetry_rejected",
                position: Position::Absolute,
                left: SIDE_LEFT + 50,
                top: 118,
                width: SIDE_WIDTH - 50,
                height: 26,
                font_size: 20,
                text_color: ${ if model.rejected() > 0 { ACCENT } else { INK } },
                paragraph: end_style()
            )
            Text (
                text: ${ format_args!("{}", model.wip()) },
                capacity: 8,
                id: "factory_telemetry_wip",
                position: Position::Absolute,
                left: SIDE_LEFT + 50,
                top: 158,
                width: SIDE_WIDTH - 50,
                height: 26,
                font_size: 20,
                text_color: INK,
                paragraph: end_style()
            )
        }
    }
}

#[compose]
fn compose_status(model: model!(FactoryModel)) -> Entity {
    ui! {
        Text (
            text: ${
                StatusText {
                    status: model.status(),
                    notice: model.notice(),
                    brief: model.mission().description,
                    wip: model.wip(),
                    blocked: model.blocked(),
                    rejected: model.rejected(),
                    delivered: model.delivered(),
                    goal: model.mission().goal,
                    target: model.mission().target,
                    tick: model.tick(),
                    power: model.power(),
                    power_cap: model.mission().power,
                    completed: model.completed(),
                }
            },
            capacity: 128,
            id: "factory_status",
            visible: ${ model.modal() == FactoryModal::None && model.page() != FactoryPage::Orders },
            position: Position::Absolute,
            left: 8,
            top: 298,
            width: 464,
            height: 18,
            font_size: 8,
            text_color: ${ if model.notice().is_some() { ACCENT } else { MUTED } },
            paragraph: label_style()
        )
    }
}

/// A button of the tool picker or of a question, by its place in the 2 x 3 grid.
#[compose]
fn compose_modal_button(model: model!(FactoryModel), index: usize) -> Entity {
    ui! {
        Button (
            text: ${ modal_button_label(model.modal(), index) },
            capacity: 48,
            visible: ${ modal_button_visible(model.modal(), index) },
            position: Position::Absolute,
            left: 43 + (index % 2) as i32 * 206,
            top: 128 + (index / 2) as i32 * 44,
            width: 188,
            height: 35,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(modal_button_active(model.modal(), model.tool(), index), true) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.select_modal_option(index); }
    }
}

/// The card after a finished order: title, stars (painted), the run and the
/// way on.
#[compose]
fn compose_result_card(model: model!(FactoryModel)) -> Entity {
    ui! {
        View (
            id: "factory_result",
            visible: ${ model.modal() == FactoryModal::Result },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            Text (
                text: ${ ResultTitle(model.mission_index()) },
                capacity: 32,
                id: "factory_result_title",
                position: Position::Absolute,
                left: 82,
                top: 89,
                width: 276,
                height: 18,
                font_size: 10,
                text_color: BACKGROUND,
                paragraph: centered()
            )
            Text (
                text: ${
                    ResultLine {
                        tick: model.tick(),
                        cost: model.cost(),
                        par: model.par_cost(),
                    }
                },
                capacity: 48,
                id: "factory_result_line",
                position: Position::Absolute,
                left: 82,
                top: 168,
                width: 276,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: centered()
            )
            Button (
                tr!(en : "Stay", zh : "留在本单"),
                id: "factory_result_stay",
                position: Position::Absolute,
                left: 84,
                top: 194,
                width: 104,
                height: 30,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(1); }
            Button (
                text: ${ NextLabel(model.has_next()) },
                capacity: 24,
                id: "factory_result_next",
                position: Position::Absolute,
                left: 196,
                top: 194,
                width: 160,
                height: 30,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ACCENT,
                pressed_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(0); }
        }
    }
}

#[compose]
fn compose_modal(model: model!(FactoryModel)) -> Entity {
    ui! {
        View (
            id: "factory_modal",
            visible: ${ model.modal() != FactoryModal::None },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) [
            FactoryModalSurface {
                model: model.clone(),
            },
        ] {
            Text (
                text: ${
                    ModalTitle {
                        modal: model.modal(),
                        mission: model.mission_index(),
                    }
                },
                capacity: 48,
                id: "factory_modal_title",
                position: Position::Absolute,
                left: 43,
                top: 66,
                width: 360,
                height: 20,
                font_size: 14,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                text: ${ modal_subtitle(model.modal()) },
                capacity: 96,
                id: "factory_modal_subtitle",
                position: Position::Absolute,
                left: 43,
                top: 101,
                width: 386,
                height: 18,
                font_size: 9,
                text_color: MUTED,
                paragraph: label_style()
            )
            compose_modal_button (model, 0)
            compose_modal_button (model, 1)
            compose_modal_button (model, 2)
            compose_modal_button (model, 3)
            compose_modal_button (model, 4)
            compose_modal_button (model, 5)
            Text (
                tr!(en : "MATERIAL FLOW", zh : "物料流程"),
                visible: ${ model.modal() == FactoryModal::Help },
                position: Position::Absolute,
                left: 43,
                top: 102,
                width: 150,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                HELP_FLOW,
                visible: ${ model.modal() == FactoryModal::Help },
                position: Position::Absolute,
                left: 43,
                top: 118,
                width: 150,
                height: 108,
                font_size: 8,
                line_height: 15,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                tr!(en : "CONTROLS", zh : "操作"),
                visible: ${ model.modal() == FactoryModal::Help },
                position: Position::Absolute,
                left: 205,
                top: 102,
                width: 150,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                HELP_CONTROLS,
                visible: ${ model.modal() == FactoryModal::Help },
                position: Position::Absolute,
                left: 205,
                top: 118,
                width: 238,
                height: 93,
                font_size: 8,
                line_height: 15,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                HELP_KEYS,
                visible: ${ model.modal() == FactoryModal::Help },
                position: Position::Absolute,
                left: 43,
                top: 246,
                width: 398,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Button (
                "×",
                id: "factory_modal_close",
                visible: ${ model.modal() != FactoryModal::Result },
                position: Position::Absolute,
                left: 420,
                top: 64,
                width: 24,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 12,
                normal_color: INK,
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.close_modal(); }
            compose_result_card (model)
        } on Tap { }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(FactoryModel)) {
    ui! {
        View (id: "factory_surface", width: 480, height: 320, clip_children: true) [
            FactorySurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] {
            compose_header (model)
            compose_line_page (model)
            compose_orders_page (model)
            compose_telemetry_page (model)
            compose_controls (model)
            compose_status (model)
            compose_modal (model)
        } on Tap(|event: event!(Tap), rows: query!(FactorySurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_tap(surface, rect, event.x, event.y)
        })
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(GameFontPlugin);
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    let model = app.add_model(FactoryModel::default()).finish();
    app.add_system(factory_tick_system::system(model.clone()));
    app.on_event(Key::EVENT, keyboard::callback(model.clone()));
    app.compose(parent, build_widgets(model));
}
