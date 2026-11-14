use super::input::{factory_tick_system, surface_tap};
use super::render::{modal_render, surface_render};
use super::state::{FactoryModalSurface, FactorySurface};
use super::style::{ACCENT, BACKGROUND, INK, LINE, MUTED};
use super::text::{
    CellCoordinate, HELP_CONTROLS, HELP_FLOW, ModalTitle, ModuleDetail, ModuleName, OrderMeta,
    OrderSpecs, OrderTitle, StatusText, modal_button_label, modal_subtitle, run_label,
    tool_button_label,
};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::factory::{
    FactoryModal, FactoryModel, FactoryPage, FactoryTool, ModuleKind,
};
use crate::gallery::play::font::PlayFontPlugin;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

fn end_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::End)
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

fn control_text_color(_active: bool, enabled: bool) -> Color {
    if enabled { BACKGROUND } else { MUTED }
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
const TILE_WIDTH: i32 = 136;
const TILE_HEIGHT: i32 = 40;

fn tile_left(index: u8) -> i32 {
    17 + i32::from(index % 2) * (TILE_WIDTH + 6)
}

fn tile_top(index: u8) -> i32 {
    70 + i32::from(index / 2) * (TILE_HEIGHT + 6)
}

fn modal_button_visible(modal: FactoryModal, index: usize) -> bool {
    match modal {
        FactoryModal::Tools => true,
        FactoryModal::Confirm { .. } => index < 2,
        FactoryModal::Help | FactoryModal::None => false,
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
        FactoryModal::Help | FactoryModal::None => false,
    }
}

#[compose]
fn compose_header(model: model!(FactoryModel)) -> Entity {
    ui! {
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
    };
    ui! {
        Text (
            text: ${ OrderMeta(model.mission_index()) },
            capacity: 40,
            id: "factory_order_meta",
            position: Position::Absolute,
            left: 230,
            top: 8,
            width: 209,
            height: 16,
            font_size: 9,
            text_color: Color::rgb(174, 186, 190),
            paragraph: end_style()
        )
    };
    ui! {
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
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.open_modal(FactoryModal::Help); }
    };
    ui! {
        Button (
            "产线",
            id: "factory_tab_line",
            position: Position::Absolute,
            left: 24,
            top: 33,
            width: 80,
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
            "订单",
            id: "factory_tab_orders",
            position: Position::Absolute,
            left: 108,
            top: 33,
            width: 80,
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
            "遥测",
            id: "factory_tab_telemetry",
            position: Position::Absolute,
            left: 192,
            top: 33,
            width: 80,
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
            text: ${ format_args!("T+{:03} / {}", model.tick(), model.mission().limit) },
            capacity: 16,
            id: "factory_tick_meta",
            position: Position::Absolute,
            left: 300,
            top: 39,
            width: 122,
            height: 14,
            font_size: 8,
            text_color: MUTED,
            paragraph: end_style()
        )
    };
    ui! {
        Button (
            text: ${ if model.fast() { "×4" } else { "×1" } },
            capacity: 4,
            id: "factory_speed",
            position: Position::Absolute,
            left: 430,
            top: 33,
            width: 42,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.fast(), true) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.toggle_speed(); }
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
            height: 282
        ) {
            Text (
                "DELIVERED",
                position: Position::Absolute,
                left: 318,
                top: 74,
                width: 80,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ model.mission().target.label() },
                capacity: 12,
                id: "factory_target",
                position: Position::Absolute,
                left: 398,
                top: 74,
                width: 60,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: end_style()
            )
            Text (
                text: ${ format_args!("{} / {}", model.delivered(), model.mission().goal) },
                capacity: 16,
                id: "factory_delivered",
                position: Position::Absolute,
                left: 319,
                top: 86,
                width: 139,
                height: 27,
                font_size: 20,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                "CREDITS",
                position: Position::Absolute,
                left: 318,
                top: 126,
                width: 70,
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
                left: 388,
                top: 125,
                width: 70,
                height: 14,
                font_size: 10,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                "POWER",
                position: Position::Absolute,
                left: 318,
                top: 151,
                width: 70,
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
                left: 388,
                top: 150,
                width: 70,
                height: 14,
                font_size: 10,
                text_color: ${ if model.power() > model.mission().power { ACCENT } else { INK } },
                paragraph: end_style()
            )
            Text (
                "SELECTED",
                position: Position::Absolute,
                left: 318,
                top: 184,
                width: 70,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ CellCoordinate(model.selected()) },
                capacity: 12,
                id: "factory_selected_coord",
                position: Position::Absolute,
                left: 388,
                top: 184,
                width: 70,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: end_style()
            )
            Text (
                text: ${ ModuleName(model.selected_cell()) },
                capacity: 24,
                id: "factory_selected",
                position: Position::Absolute,
                left: 318,
                top: 199,
                width: 140,
                height: 16,
                font_size: 10,
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
                capacity: 40,
                id: "factory_selected_detail",
                position: Position::Absolute,
                left: 318,
                top: 218,
                width: 140,
                height: 28,
                font_size: 8,
                line_height: 13,
                text_color: MUTED,
                paragraph: ParagraphStyle::default()
            )
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
                capacity: 24,
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
                "★",
                visible: ${ model.completed() & (1 << index) != 0 },
                position: Position::Absolute,
                left: TILE_WIDTH - 24,
                top: 13,
                width: 14,
                height: 14,
                font_size: 10,
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
            height: 282
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
                text: ${ format_args!("ORDER {:02}", model.browse() + 1) },
                capacity: 12,
                id: "factory_brief_number",
                position: Position::Absolute,
                left: 319,
                top: 73,
                width: 70,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                "★ 已完成",
                visible: ${ model.completed() & (1 << model.browse()) != 0 },
                position: Position::Absolute,
                left: 389,
                top: 73,
                width: 70,
                height: 12,
                font_size: 8,
                text_color: ACCENT,
                paragraph: end_style()
            )
            Text (
                text: ${ model.browsed_mission().name },
                capacity: 16,
                id: "factory_brief_name",
                position: Position::Absolute,
                left: 319,
                top: 87,
                width: 140,
                height: 24,
                font_size: 20,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ model.browsed_mission().description },
                capacity: 96,
                id: "factory_brief_description",
                position: Position::Absolute,
                left: 319,
                top: 116,
                width: 140,
                height: 28,
                font_size: 8,
                line_height: 13,
                text_color: INK,
                paragraph: ParagraphStyle::default()
            )
            Text (
                "交付\n限时\n信用\n功率",
                position: Position::Absolute,
                left: 319,
                top: 152,
                width: 50,
                height: 56,
                font_size: 8,
                line_height: 14,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ OrderSpecs(model.browse()) },
                capacity: 40,
                id: "factory_brief_specs",
                position: Position::Absolute,
                left: 369,
                top: 152,
                width: 90,
                height: 56,
                font_size: 8,
                line_height: 14,
                text_color: INK,
                paragraph: end_style()
            )
            Button (
                text: ${ if model.browse() == model.mission_index() { "↺ 重来" } else { "开工" } },
                capacity: 12,
                id: "factory_start",
                position: Position::Absolute,
                left: 319,
                top: 216,
                width: 68,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACCENT,
                pressed_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.request_browsed(false); }
            Button (
                "示范布局",
                id: "factory_reference",
                position: Position::Absolute,
                left: 391,
                top: 216,
                width: 68,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.request_browsed(true); }
        }
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
            height: 282
        ) {
            Text (
                "DELIVERED / LAST 80 TICKS",
                position: Position::Absolute,
                left: 18,
                top: 72,
                width: 200,
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
                left: 220,
                top: 72,
                width: 80,
                height: 12,
                font_size: 8,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                "BLOCKED CELLS",
                position: Position::Absolute,
                left: 18,
                top: 176,
                width: 200,
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
                left: 220,
                top: 176,
                width: 80,
                height: 12,
                font_size: 8,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                "投料",
                position: Position::Absolute,
                left: 323,
                top: 85,
                width: 50,
                height: 12,
                font_size: 9,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{}", model.produced()) },
                capacity: 8,
                id: "factory_telemetry_produced",
                position: Position::Absolute,
                left: 383,
                top: 77,
                width: 74,
                height: 27,
                font_size: 20,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                "交付",
                position: Position::Absolute,
                left: 323,
                top: 132,
                width: 50,
                height: 12,
                font_size: 9,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{}", model.delivered()) },
                capacity: 8,
                id: "factory_telemetry_delivered",
                position: Position::Absolute,
                left: 383,
                top: 124,
                width: 74,
                height: 27,
                font_size: 20,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                "退回",
                position: Position::Absolute,
                left: 323,
                top: 179,
                width: 50,
                height: 12,
                font_size: 9,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{}", model.rejected()) },
                capacity: 8,
                id: "factory_telemetry_rejected",
                position: Position::Absolute,
                left: 383,
                top: 171,
                width: 74,
                height: 27,
                font_size: 20,
                text_color: ${ if model.rejected() > 0 { ACCENT } else { INK } },
                paragraph: end_style()
            )
            Text (
                "在制",
                position: Position::Absolute,
                left: 323,
                top: 226,
                width: 50,
                height: 12,
                font_size: 9,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{}", model.wip()) },
                capacity: 8,
                id: "factory_telemetry_wip",
                position: Position::Absolute,
                left: 383,
                top: 218,
                width: 74,
                height: 27,
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
                    tick: model.tick(),
                    power: model.power(),
                    power_cap: model.mission().power,
                    completed: model.completed(),
                }
            },
            capacity: 96,
            id: "factory_status",
            visible: ${ model.modal() == FactoryModal::None && model.page() != FactoryPage::Orders },
            position: Position::Absolute,
            left: 16,
            top: 259,
            width: 447,
            height: 18,
            font_size: 8,
            text_color: ${ if model.notice().is_some() { ACCENT } else { MUTED } },
            paragraph: label_style()
        )
    }
}

#[compose]
fn compose_footer(model: model!(FactoryModel)) -> Entity {
    ui! {
        Button (
            text: ${ tool_button_label(model.tool()) },
            capacity: 16,
            id: "factory_footer_0",
            position: Position::Absolute,
            left: 8,
            top: 287,
            width: 85,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(model.tool() != FactoryTool::Select, true) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.open_modal(FactoryModal::Tools); }
    };
    ui! {
        Button (
            "旋转 ↻",
            id: "factory_footer_1",
            position: Position::Absolute,
            left: 98,
            top: 287,
            width: 85,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(false, model.can_rotate()) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.can_rotate()) },
            border_radius: 0
        ) on Tap { let _ = model.rotate_active(); }
    };
    ui! {
        Button (
            "↺ 撤销",
            id: "factory_footer_2",
            position: Position::Absolute,
            left: 188,
            top: 287,
            width: 85,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(false, model.history_len() > 0) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.history_len() > 0) },
            border_radius: 0
        ) on Tap { model.undo(); }
    };
    ui! {
        Button (
            "单步",
            id: "factory_footer_3",
            position: Position::Absolute,
            left: 278,
            top: 287,
            width: 85,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(false, model.power() <= model.mission().power) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.power() <= model.mission().power) },
            border_radius: 0
        ) on Tap { let _ = model.step_once(); }
    };
    ui! {
        Button (
            text: ${ run_label(model.has_next(), model.running()) },
            capacity: 16,
            id: "factory_footer_4",
            position: Position::Absolute,
            left: 368,
            top: 287,
            width: 104,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(true, model.power() <= model.mission().power) },
            pressed_color: INK,
            text_color: ${ control_text_color(true, model.power() <= model.mission().power) },
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
                capacity: 40,
                id: "factory_modal_title",
                position: Position::Absolute,
                left: 43,
                top: 72,
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
                top: 107,
                width: 386,
                height: 18,
                font_size: 9,
                text_color: MUTED,
                paragraph: label_style()
            )
            Button (
                text: ${ modal_button_label(model.modal(), 0) },
                capacity: 40,
                id: "factory_modal_0",
                visible: ${ modal_button_visible(model.modal(), 0) },
                position: Position::Absolute,
                left: 43,
                top: 137,
                width: 188,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), model.tool(), 0), true) },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(0); }
            Button (
                text: ${ modal_button_label(model.modal(), 1) },
                capacity: 40,
                id: "factory_modal_1",
                visible: ${ modal_button_visible(model.modal(), 1) },
                position: Position::Absolute,
                left: 249,
                top: 137,
                width: 188,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), model.tool(), 1), true) },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(1); }
            Button (
                text: ${ modal_button_label(model.modal(), 2) },
                capacity: 40,
                id: "factory_modal_2",
                visible: ${ modal_button_visible(model.modal(), 2) },
                position: Position::Absolute,
                left: 43,
                top: 180,
                width: 188,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), model.tool(), 2), true) },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(2); }
            Button (
                text: ${ modal_button_label(model.modal(), 3) },
                capacity: 40,
                id: "factory_modal_3",
                visible: ${ modal_button_visible(model.modal(), 3) },
                position: Position::Absolute,
                left: 249,
                top: 180,
                width: 188,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), model.tool(), 3), true) },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(3); }
            Button (
                text: ${ modal_button_label(model.modal(), 4) },
                capacity: 40,
                id: "factory_modal_4",
                visible: ${ modal_button_visible(model.modal(), 4) },
                position: Position::Absolute,
                left: 43,
                top: 223,
                width: 188,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), model.tool(), 4), true) },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(4); }
            Button (
                text: ${ modal_button_label(model.modal(), 5) },
                capacity: 40,
                id: "factory_modal_5",
                visible: ${ modal_button_visible(model.modal(), 5) },
                position: Position::Absolute,
                left: 249,
                top: 223,
                width: 188,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), model.tool(), 5), true) },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(5); }
            Text (
                "MATERIAL FLOW",
                visible: ${ model.modal() == FactoryModal::Help },
                position: Position::Absolute,
                left: 43,
                top: 108,
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
                top: 126,
                width: 150,
                height: 108,
                font_size: 8,
                line_height: 15,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                "CONTROLS",
                visible: ${ model.modal() == FactoryModal::Help },
                position: Position::Absolute,
                left: 205,
                top: 108,
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
                top: 126,
                width: 236,
                height: 93,
                font_size: 8,
                line_height: 15,
                text_color: INK,
                paragraph: label_style()
            )
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
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.close_modal(); }
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
            compose_status (model)
            compose_footer (model)
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
    app.add_plugin(PlayFontPlugin);
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    let model = app.add_model(FactoryModel::default()).finish();
    app.add_system(factory_tick_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}
