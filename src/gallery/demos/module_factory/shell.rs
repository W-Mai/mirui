use super::input::{factory_tick_system, surface_gesture};
use super::render::{modal_render, surface_render};
use super::state::{FactoryModalSurface, FactorySurface};
use super::style::{ACCENT, BACKGROUND, INK, LINE, MUTED};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::factory::{
    FactoryModal, FactoryModel, FactoryPage, FactoryStatus, FactoryTool, GRID_WIDTH, MISSIONS,
    ModuleKind,
};
use crate::gallery::play::font::PlayFontPlugin;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

fn control_color(active: bool, enabled: bool) -> Color {
    if active {
        ACCENT
    } else if enabled {
        INK
    } else {
        LINE
    }
}

fn control_text_color(active: bool, enabled: bool) -> Color {
    if active || enabled { BACKGROUND } else { MUTED }
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

struct StatusText {
    status: FactoryStatus,
    wip: u8,
    blocked: u8,
    rejected: u16,
}

impl fmt::Display for StatusText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            FactoryStatus::Won => out.write_str("✓ 订单完成！可在订单页选择下一条产线。"),
            FactoryStatus::Timeout => out.write_str("试运行到期；请修改布局后重试。"),
            FactoryStatus::Running => write!(
                out,
                "生产中 / 在制 {} / 堵塞 {} / 退回 {}",
                self.wip, self.blocked, self.rejected
            ),
            FactoryStatus::Ready | FactoryStatus::Paused => write!(
                out,
                "已暂停 / 在制 {} / 堵塞 {} / 退回 {}",
                self.wip, self.blocked, self.rejected
            ),
        }
    }
}

struct MissionDescription(usize);

impl fmt::Display for MissionDescription {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mission = MISSIONS[self.0];
        write!(
            out,
            "{} 件 / 预算 {}\n{}",
            mission.goal, mission.budget, mission.description
        )
    }
}

fn modal_title(modal: FactoryModal) -> &'static str {
    match modal {
        FactoryModal::None => "",
        FactoryModal::Tools => "建造模块",
        FactoryModal::Confirm {
            reference: true, ..
        } => "装载示范布局？",
        FactoryModal::Confirm {
            reference: false, ..
        } => "切换订单？",
        FactoryModal::Help => "模块工厂 / 操作手册",
    }
}

fn modal_subtitle(modal: FactoryModal) -> &'static str {
    match modal {
        FactoryModal::None => "",
        FactoryModal::Tools => "选择工具后点击格子；修改布局会重置试运行。",
        FactoryModal::Confirm { .. } => "当前产线、进度与撤销记录将被替换。",
        FactoryModal::Help => "矿石经熔炼和装配后交付；质检订单还需要通过质检台。",
    }
}

fn modal_button_label(modal: FactoryModal, index: usize) -> &'static str {
    match modal {
        FactoryModal::Tools => [
            "选择 / 检查",
            "传送带 · 1 信用",
            "熔炼炉 · 4 / 3P",
            "装配机 · 5 / 4P",
            "质检台 · 3 / 2P",
            "拆除模块",
        ][index],
        FactoryModal::Confirm { .. } => ["取消", "确认装载", "", "", "", ""][index],
        FactoryModal::Help => ["明白了", "", "", "", "", ""][index],
        FactoryModal::None => "",
    }
}

fn modal_button_visible(modal: FactoryModal, index: usize) -> bool {
    match modal {
        FactoryModal::Tools => true,
        FactoryModal::Confirm { .. } => index < 2,
        FactoryModal::Help => index == 0,
        FactoryModal::None => false,
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
        FactoryModal::Help => index == 0,
        FactoryModal::None => false,
    }
}

#[compose(bind(model))]
fn compose_header(model: FactoryModel) -> Entity {
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
            text: ${
                format_args!(
                    "ORDER 0{} / {}:{}", model.mission_index() + 1, model.delivered(), model
                    .mission().goal,
                )
            },
            text_capacity: 24,
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
            width: 96,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.page() == FactoryPage::Line, true) },
            pressed_color: ACCENT,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(FactoryPage::Line); }
    };
    ui! {
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
            normal_color: ${ control_color(model.page() == FactoryPage::Orders, true) },
            pressed_color: ACCENT,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(FactoryPage::Orders); }
    };
    ui! {
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
            normal_color: ${ control_color(model.page() == FactoryPage::Telemetry, true) },
            pressed_color: ACCENT,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(FactoryPage::Telemetry); }
    };
    ui! {
        Text (
            text: ${ format_args!("PWR {}/{}", model.power(), model.mission().power) },
            text_capacity: 12,
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
    };
    ui! {
        Text (
            text: ${ format_args!("T+{:03}", model.tick()) },
            text_capacity: 8,
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
    }
}

#[compose(bind(model))]
fn compose_line_page(model: FactoryModel) -> Entity {
    ui! {
        View (
            id: "factory_line_page",
            visible: ${ model.page() == FactoryPage::Line },
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
                text: ${ format_args!("{} / {}", model.delivered(), model.mission().goal) },
                text_capacity: 16,
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
                text: ${ format_args!("{} / {}", model.cost(), model.mission().budget) },
                text_capacity: 16,
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
                text: ${ model.selected_cell().map_or("待建造空位", |cell| cell.kind.label()) },
                text_capacity: 18,
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
                text: ${
                    format_args!(
                        "C{} · R{}", model.selected() % GRID_WIDTH + 1, model.selected() / GRID_WIDTH +
                        1,
                    )
                },
                text_capacity: 12,
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
                text: ${
                    StatusText {
                        status: model.status(),
                        wip: model.wip(),
                        blocked: model.blocked(),
                        rejected: model.rejected(),
                    }
                },
                text_capacity: 96,
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
                text: ${ format_args!("方向 {}", direction_glyph(model.tool_direction())) },
                text_capacity: 12,
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
                text: ${ tool_label(model.tool()) },
                text_capacity: 18,
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
    }
}

#[compose(bind(model))]
fn compose_orders_page(model: FactoryModel) -> Entity {
    ui! {
        View (
            id: "factory_orders_page",
            visible: ${ model.page() == FactoryPage::Orders },
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
                normal_color: ${ control_color(model.mission_index() == 0, true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.request_mission(0, false); }
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
                normal_color: ${ control_color(model.mission_index() == 1, true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.request_mission(1, false); }
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
                normal_color: ${ control_color(model.mission_index() == 2, true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.request_mission(2, false); }
            Text (
                text: ${ MissionDescription(0) },
                text_capacity: 96,
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
                text: ${ MissionDescription(1) },
                text_capacity: 96,
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
                text: ${ MissionDescription(2) },
                text_capacity: 96,
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
            ) on Tap { model.request_reference(); }
        }
    }
}

#[compose(bind(model))]
fn compose_telemetry_page(model: FactoryModel) -> Entity {
    ui! {
        View (
            id: "factory_telemetry_page",
            visible: ${ model.page() == FactoryPage::Telemetry },
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
                text: ${ format_args!("{}", model.tick()) },
                text_capacity: 8,
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
                text: ${ format_args!("{}", model.produced()) },
                text_capacity: 8,
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
                text: ${ format_args!("{}", model.delivered()) },
                text_capacity: 8,
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
                text: ${ format_args!("{}", model.rejected()) },
                text_capacity: 8,
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
                text: ${ format_args!("{}", model.wip()) },
                text_capacity: 8,
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
    }
}

#[compose(bind(model))]
fn compose_footer(model: FactoryModel) -> Entity {
    ui! {
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
            normal_color: ${ control_color(model.modal() == FactoryModal::Tools, true) },
            pressed_color: ACCENT,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.open_modal(FactoryModal::Tools); }
    };
    ui! {
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
            normal_color: ${
                control_color(
                    false,
                    model.tool() != FactoryTool::Select
                        || model
                            .selected_cell()
                            .is_some_and(|cell| {
                                !matches!(cell.kind, ModuleKind::Source | ModuleKind::Dock)
                            }),
                )
            },
            pressed_color: ACCENT,
            text_color: ${
                control_text_color(
                    false,
                    model.tool() != FactoryTool::Select
                        || model
                            .selected_cell()
                            .is_some_and(|cell| {
                                !matches!(cell.kind, ModuleKind::Source | ModuleKind::Dock)
                            }),
                )
            },
            border_radius: 0
        ) on Tap { let _ = model.rotate_active(); }
    };
    ui! {
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
            left: 282,
            top: 287,
            width: 82,
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
            text: ${ if model.running() { "Ⅱ 暂停" } else { "▶ 运行" } },
            text_capacity: 12,
            id: "factory_footer_4",
            position: Position::Absolute,
            left: 369,
            top: 287,
            width: 103,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(true, model.power() <= model.mission().power) },
            pressed_color: INK,
            text_color: ${ control_text_color(true, model.power() <= model.mission().power) },
            border_radius: 0
        ) on Tap { let _ = model.toggle_run(); }
    }
}

#[compose(bind(model))]
fn compose_modal(model: FactoryModel) -> Entity {
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
        ] on Tap { }
        {
            Text (
                text: ${ modal_title(model.modal()) },
                text_capacity: 32,
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
                text: ${ modal_subtitle(model.modal()) },
                text_capacity: 96,
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
                text: ${ modal_button_label(model.modal(), 0) },
                text_capacity: 32,
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
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(0); }
            Button (
                text: ${ modal_button_label(model.modal(), 1) },
                text_capacity: 32,
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
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(1); }
            Button (
                text: ${ modal_button_label(model.modal(), 2) },
                text_capacity: 32,
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
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(2); }
            Button (
                text: ${ modal_button_label(model.modal(), 3) },
                text_capacity: 32,
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
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(3); }
            Button (
                text: ${ modal_button_label(model.modal(), 4) },
                text_capacity: 32,
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
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(4); }
            Button (
                text: ${ modal_button_label(model.modal(), 5) },
                text_capacity: 32,
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
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.select_modal_option(5); }
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
            ) on Tap { model.close_modal(); }
        }
    }
}

#[compose(bind(model))]
pub(super) fn build_widgets(model: FactoryModel) {
    ui! {
        View (id: "factory_surface", width: 480, height: 320, clip_children: true) [
            FactorySurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] on Tap { surface_gesture(&ctx); }
        {
            compose_header (model)
            compose_line_page (model)
            compose_orders_page (model)
            compose_telemetry_page (model)
            compose_footer (model)
            compose_modal (model)
        }
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
    let model = app.add_model(FactoryModel::default());
    app.add_system(factory_tick_system::system(model.clone()));
    app.compose(parent, |cx| build_widgets(cx, model));
}
