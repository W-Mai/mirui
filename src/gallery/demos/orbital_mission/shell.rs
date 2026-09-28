use super::input::{cancel_node, footer_action, modal_action, orbit_tick_system};
use super::render::{modal_view, surface_view};
use super::state::{OrbitModalSurface, OrbitSurface};
use super::style::{BACKGROUND, CYAN, INK, MUTED, ORANGE, SPACE, label_style};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::orbit::{
    MISSIONS, ManeuverNode, OrbitEventKind, OrbitModal, OrbitModel, OrbitPage, OrbitStatus,
};
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

fn control_color(active: bool, enabled: bool) -> Color {
    if !enabled {
        super::style::LINE
    } else if active {
        ORANGE
    } else {
        INK
    }
}

fn control_text_color(enabled: bool) -> Color {
    if enabled { BACKGROUND } else { MUTED }
}

struct Decimal1(crate::types::Fixed64);

impl fmt::Display for Decimal1 {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scaled = (self.0 * 10).to_int();
        write!(out, "{}.{:01}", scaled / 10, scaled.unsigned_abs() % 10)
    }
}

struct NodeText {
    node: Option<ManeuverNode>,
    number: usize,
}

impl fmt::Display for NodeText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(node) = self.node else {
            return write!(out, "0{}  空节点", self.number);
        };
        write!(
            out,
            "0{}  T+{}  Δv {}.{}  {}°",
            self.number,
            Decimal1(node.at),
            node.dv_tenths / 10,
            node.dv_tenths % 10,
            node.angle_degrees
        )
    }
}

fn event_label(kind: OrbitEventKind) -> &'static str {
    match kind {
        OrbitEventKind::Loaded => "任务装载 · 等待机动",
        OrbitEventKind::Burn => "机动点火完成",
        OrbitEventKind::NodeSkipped => "计划节点跳过",
        OrbitEventKind::Goal => "目标窗口采样完成",
        OrbitEventKind::Won => "全部目标完成",
        OrbitEventKind::Crashed => "接近中心边界 · 中止",
        OrbitEventKind::Escaped => "越出模拟区域 · 中止",
    }
}

fn status_label(status: OrbitStatus) -> &'static str {
    match status {
        OrbitStatus::Ready => "READY",
        OrbitStatus::Running => "RUNNING",
        OrbitStatus::Paused => "PAUSED",
        OrbitStatus::Won => "COMPLETE",
        OrbitStatus::Crashed => "CRASHED",
        OrbitStatus::Escaped => "ESCAPED",
    }
}

fn modal_title(modal: OrbitModal) -> &'static str {
    match modal {
        OrbitModal::None => "",
        OrbitModal::Missions => "选择轨道任务",
        OrbitModal::Confirm(_) => "重新装载任务？",
        OrbitModal::Help => "轨道任务台 / 操作手册",
    }
}

fn modal_subtitle(modal: OrbitModal) -> &'static str {
    match modal {
        OrbitModal::None => "",
        OrbitModal::Missions => "切换会清空当前轨迹、计划与遥测记录。",
        OrbitModal::Confirm(_) => "当前模拟状态将被重置。",
        OrbitModal::Help => "设定机动并点火；进入目标窗口后手动采样。",
    }
}

fn mission_name(index: u8) -> &'static str {
    MISSIONS[usize::from(index)].name
}

fn modal_button(modal: OrbitModal, index: usize, mission: u8) -> (&'static str, bool, bool) {
    match modal {
        OrbitModal::Missions => (
            MISSIONS.get(index).map_or("", |entry| entry.name),
            index < 3,
            index as u8 == mission,
        ),
        OrbitModal::Confirm(_) => match index {
            0 => ("取消", true, false),
            1 => ("确认装载", true, true),
            _ => ("", false, false),
        },
        OrbitModal::Help => (
            if index == 0 { "明白了" } else { "" },
            index == 0,
            index == 0,
        ),
        OrbitModal::None => ("", false, false),
    }
}

#[compose(bind(model))]
fn build_widgets(model: OrbitModel) {
    ui! {
        View (id: "orbit_surface", width: 480, height: 320, clip_children: true) [
            OrbitSurface {
                model: model.clone(),
            },
        ] {
            Text (
                "轨道任务台",
                position: Position::Absolute,
                left: 24,
                top: 6,
                width: 160,
                height: 20,
                font_size: 14,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Button (
                text: ${ format_args!("任务 0{} · {}", model.mission() + 1, mission_name(model.mission())) },
                text_capacity: 48,
                id: "orbit_mission",
                position: Position::Absolute,
                left: 284,
                top: 4,
                width: 158,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.set_modal(OrbitModal::Missions); }
            Button (
                "?",
                position: Position::Absolute,
                left: 450,
                top: 4,
                width: 25,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 12,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.set_modal(OrbitModal::Help); }
            Button (
                "航图",
                id: "orbit_tab_map",
                position: Position::Absolute,
                left: 24,
                top: 33,
                width: 90,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(model.page() == OrbitPage::Map, true) },
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.set_page(OrbitPage::Map); }
            Button (
                "计划",
                id: "orbit_tab_plan",
                position: Position::Absolute,
                left: 119,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(model.page() == OrbitPage::Plan, true) },
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.set_page(OrbitPage::Plan); }
            Button (
                "记录",
                id: "orbit_tab_record",
                position: Position::Absolute,
                left: 206,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(model.page() == OrbitPage::Record, true) },
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.set_page(OrbitPage::Record); }
            Text (
                text: ${ format_args!("SIM T+{}", Decimal1(model.time())) },
                text_capacity: 24,
                id: "orbit_time",
                position: Position::Absolute,
                left: 356,
                top: 39,
                width: 102,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            View (
                id: "orbit_map_page",
                visible: ${ model.page() == OrbitPage::Map },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    text: ${ format_args!("FUEL  {} / 48", Decimal1(model.fuel())) },
                    text_capacity: 24,
                    id: "orbit_fuel",
                    position: Position::Absolute,
                    left: 318,
                    top: 78,
                    width: 139,
                    height: 15,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    text: ${ format_args!("HEAT  {}%", model.heat().to_int()) },
                    text_capacity: 20,
                    id: "orbit_heat",
                    position: Position::Absolute,
                    left: 318,
                    top: 105,
                    width: 139,
                    height: 15,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    text: ${ Decimal1(model.radius()) },
                    text_capacity: 16,
                    id: "orbit_radius",
                    position: Position::Absolute,
                    left: 318,
                    top: 136,
                    width: 62,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    text: ${ Decimal1(model.speed()) },
                    text_capacity: 16,
                    id: "orbit_speed",
                    position: Position::Absolute,
                    left: 395,
                    top: 136,
                    width: 62,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    text: ${ format_args!("{}.{}", model.dv_tenths() / 10, model.dv_tenths() % 10) },
                    text_capacity: 8,
                    id: "orbit_dv",
                    position: Position::Absolute,
                    left: 417,
                    top: 169,
                    width: 40,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Button (
                    "-",
                    position: Position::Absolute,
                    left: 318,
                    top: 191,
                    width: 34,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.adjust_dv(-2); }
                Button (
                    "参考值",
                    position: Position::Absolute,
                    left: 357,
                    top: 191,
                    width: 61,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.reset_dv(); }
                Button (
                    "+",
                    position: Position::Absolute,
                    left: 423,
                    top: 191,
                    width: 34,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.adjust_dv(2); }
                Button (
                    "-15",
                    position: Position::Absolute,
                    left: 318,
                    top: 222,
                    width: 58,
                    height: 23,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.adjust_angle(-15); }
                Text (
                    text: ${ format_args!("{}°", model.angle_degrees()) },
                    text_capacity: 8,
                    id: "orbit_angle",
                    position: Position::Absolute,
                    left: 377,
                    top: 226,
                    width: 36,
                    height: 13,
                    font_size: 8,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Center)
                )
                Button (
                    "+15",
                    position: Position::Absolute,
                    left: 414,
                    top: 222,
                    width: 43,
                    height: 23,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.adjust_angle(15); }
                Button (
                    text: ${ if model.preview() { "预演 已开" } else { "预演 已关" } },
                    text_capacity: 16,
                    id: "orbit_preview",
                    position: Position::Absolute,
                    left: 18,
                    top: 231,
                    width: 78,
                    height: 18,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: SPACE,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.toggle_preview(); }
                Button (
                    text: ${ format_args!("{}×", model.warp()) },
                    text_capacity: 8,
                    id: "orbit_warp",
                    position: Position::Absolute,
                    left: 244,
                    top: 231,
                    width: 45,
                    height: 18,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: SPACE,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.cycle_warp(); }
            }
            View (
                id: "orbit_plan_page",
                visible: ${ model.page() == OrbitPage::Plan },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    text: ${ NodeText { node: model.queue()[0], number: 1 } },
                    text_capacity: 64,
                    id: "orbit_node_0",
                    position: Position::Absolute,
                    left: 35,
                    top: 103,
                    width: 198,
                    height: 17,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "取消",
                    position: Position::Absolute,
                    left: 239,
                    top: 98,
                    width: 40,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { cancel_node(&model, 0); }
                Text (
                    text: ${ NodeText { node: model.queue()[1], number: 2 } },
                    text_capacity: 64,
                    id: "orbit_node_1",
                    position: Position::Absolute,
                    left: 35,
                    top: 151,
                    width: 198,
                    height: 17,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "取消",
                    position: Position::Absolute,
                    left: 239,
                    top: 146,
                    width: 40,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { cancel_node(&model, 1); }
                Text (
                    text: ${ NodeText { node: model.queue()[2], number: 3 } },
                    text_capacity: 64,
                    id: "orbit_node_2",
                    position: Position::Absolute,
                    left: 35,
                    top: 199,
                    width: 198,
                    height: 17,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "取消",
                    position: Position::Absolute,
                    left: 239,
                    top: 194,
                    width: 40,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { cancel_node(&model, 2); }
                Text (
                    text: ${ format_args!("{} / 3 个节点", model.queue_len()) },
                    text_capacity: 24,
                    id: "orbit_queue_count",
                    position: Position::Absolute,
                    left: 24,
                    top: 234,
                    width: 130,
                    height: 14,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    text: ${ format_args!("{} s", model.delay_seconds()) },
                    text_capacity: 8,
                    id: "orbit_delay",
                    position: Position::Absolute,
                    left: 326,
                    top: 99,
                    width: 130,
                    height: 28,
                    font_size: 20,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Button (
                    "−1 s",
                    position: Position::Absolute,
                    left: 326,
                    top: 186,
                    width: 57,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: Color::rgb(31, 50, 67),
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.adjust_delay(-1); }
                Button (
                    "＋1 s",
                    position: Position::Absolute,
                    left: 391,
                    top: 186,
                    width: 57,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: Color::rgb(31, 50, 67),
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.adjust_delay(1); }
            }
            View (
                id: "orbit_record_page",
                visible: ${ model.page() == OrbitPage::Record },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    text: ${ format_args!("{}", model.telemetry_len()) },
                    text_capacity: 8,
                    id: "orbit_samples",
                    position: Position::Absolute,
                    left: 325,
                    top: 92,
                    width: 45,
                    height: 20,
                    font_size: 14,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    text: ${ format_args!("{}", model.trail_len()) },
                    text_capacity: 8,
                    id: "orbit_trail",
                    position: Position::Absolute,
                    left: 392,
                    top: 92,
                    width: 45,
                    height: 20,
                    font_size: 14,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    text: ${ Decimal1(model.radius()) },
                    text_capacity: 16,
                    id: "orbit_record_radius",
                    position: Position::Absolute,
                    left: 325,
                    top: 134,
                    width: 112,
                    height: 18,
                    font_size: 12,
                    text_color: ORANGE,
                    paragraph: label_style()
                )
                Text (
                    text: ${ Decimal1(model.speed()) },
                    text_capacity: 16,
                    id: "orbit_record_speed",
                    position: Position::Absolute,
                    left: 325,
                    top: 158,
                    width: 112,
                    height: 18,
                    font_size: 12,
                    text_color: CYAN,
                    paragraph: label_style()
                )
                Text (
                    text: ${ model.latest_event_kind().map_or("等待事件", event_label) },
                    text_capacity: 48,
                    id: "orbit_event",
                    position: Position::Absolute,
                    left: 325,
                    top: 194,
                    width: 124,
                    height: 28,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    text: ${ status_label(model.status()) },
                    text_capacity: 16,
                    id: "orbit_status",
                    position: Position::Absolute,
                    left: 325,
                    top: 231,
                    width: 124,
                    height: 15,
                    font_size: 9,
                    text_color: ORANGE,
                    paragraph: label_style()
                )
            }
            Button (
                text: ${ if model.status() == OrbitStatus::Running { "Ⅱ 暂停" } else { "▶ 滑行" } },
                text_capacity: 16,
                id: "orbit_footer_run",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 82,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(true, !model.status().terminal()) },
                pressed_color: ORANGE,
                text_color: ${ control_text_color(!model.status().terminal()) },
                border_radius: 0
            ) on Tap { footer_action(&model, 0); }
            Button (
                "点火",
                id: "orbit_footer_burn",
                position: Position::Absolute,
                left: 99,
                top: 288,
                width: 70,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(false, !model.status().terminal()) },
                pressed_color: ORANGE,
                text_color: ${ control_text_color(!model.status().terminal()) },
                border_radius: 0
            ) on Tap { footer_action(&model, 1); }
            Button (
                text: ${ if model.eligible() { "● 立即采样" } else { "采样" } },
                text_capacity: 16,
                id: "orbit_footer_scan",
                position: Position::Absolute,
                left: 174,
                top: 288,
                width: 91,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(model.eligible(), !model.status().terminal()) },
                pressed_color: ORANGE,
                text_color: ${ control_text_color(!model.status().terminal()) },
                border_radius: 0
            ) on Tap { footer_action(&model, 2); }
            Button (
                "＋ 节点",
                id: "orbit_footer_schedule",
                position: Position::Absolute,
                left: 270,
                top: 288,
                width: 85,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(false, model.can_schedule()) },
                pressed_color: ORANGE,
                text_color: ${ control_text_color(model.can_schedule()) },
                border_radius: 0
            ) on Tap { footer_action(&model, 3); }
            Button (
                "重新装载",
                id: "orbit_footer_reset",
                position: Position::Absolute,
                left: 360,
                top: 288,
                width: 108,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(&model, 4); }
            View (
                id: "orbit_modal",
                visible: ${ model.modal() != OrbitModal::None },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320
            ) [
                OrbitModalSurface {
                    model: model.clone(),
                },
            ] {
                Text (
                    text: ${ modal_title(model.modal()) },
                    text_capacity: 48,
                    id: "orbit_modal_title",
                    position: Position::Absolute,
                    left: 55,
                    top: 80,
                    width: 300,
                    height: 20,
                    font_size: 13,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    text: ${ modal_subtitle(model.modal()) },
                    text_capacity: 96,
                    id: "orbit_modal_subtitle",
                    position: Position::Absolute,
                    left: 55,
                    top: 116,
                    width: 360,
                    height: 28,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Button (
                    text: ${ modal_button(model.modal(), 0, model.mission()).0 },
                    text_capacity: 32,
                    visible: ${ modal_button(model.modal(), 0, model.mission()).1 },
                    id: "orbit_modal_0",
                    position: Position::Absolute,
                    left: 55,
                    top: 151,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: ${ control_color(modal_button(model.modal(), 0, model.mission()).2, true) },
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(&model, 0); }
                Button (
                    text: ${ modal_button(model.modal(), 1, model.mission()).0 },
                    text_capacity: 32,
                    visible: ${ modal_button(model.modal(), 1, model.mission()).1 },
                    id: "orbit_modal_1",
                    position: Position::Absolute,
                    left: 249,
                    top: 151,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: ${ control_color(modal_button(model.modal(), 1, model.mission()).2, true) },
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(&model, 1); }
                Button (
                    text: ${ modal_button(model.modal(), 2, model.mission()).0 },
                    text_capacity: 32,
                    visible: ${ modal_button(model.modal(), 2, model.mission()).1 },
                    id: "orbit_modal_2",
                    position: Position::Absolute,
                    left: 55,
                    top: 198,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: ${ control_color(modal_button(model.modal(), 2, model.mission()).2, true) },
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(&model, 2); }
                Button (
                    text: ${ modal_button(model.modal(), 3, model.mission()).0 },
                    text_capacity: 32,
                    visible: ${ modal_button(model.modal(), 3, model.mission()).1 },
                    id: "orbit_modal_3",
                    position: Position::Absolute,
                    left: 249,
                    top: 198,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: ${ control_color(modal_button(model.modal(), 3, model.mission()).2, true) },
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(&model, 3); }
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 404,
                    top: 77,
                    width: 27,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { model.set_modal(OrbitModal::None); }
            }
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
    register_play_font(&mut app.world);
    let model = app.add_model(OrbitModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(orbit_tick_system::system(model.clone()));
    app.compose(parent, |cx| build_widgets(cx, model));
}
