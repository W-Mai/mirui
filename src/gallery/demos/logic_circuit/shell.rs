use super::input::{circuit_tick_system, footer_action, modal_action, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{CircuitModalSurface, CircuitNodes, CircuitSurface};
use super::style::{ACCENT, BACKGROUND, GREEN, INK, MUTED, PANEL};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::circuit::{CircuitModal, CircuitModel, CircuitPage};
use crate::gallery::play::font::register_play_font;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

#[compose]
pub(super) fn build_widgets() {
    ui! {
        CircuitSurface (id: "circuit_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "逻辑工作台",
                position: Position::Absolute,
                left: 24,
                top: 6,
                width: 180,
                height: 20,
                font_size: 14,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                "NET / 1:6 GATES",
                id: "circuit_gate_count",
                position: Position::Absolute,
                left: 320,
                top: 8,
                width: 130,
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
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.open_modal(CircuitModal::Help)); }
            Button (
                "布线",
                id: "circuit_tab_wire",
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
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.set_page(CircuitPage::Wire)); }
            Button (
                "真值",
                id: "circuit_tab_truth",
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
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.set_page(CircuitPage::Truth)); }
            Button (
                "时序",
                id: "circuit_tab_trace",
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
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.set_page(CircuitPage::Trace)); }
            Button (
                "任务 01 · 不同才亮",
                id: "circuit_task",
                position: Position::Absolute,
                left: 310,
                top: 34,
                width: 159,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: PANEL,
                pressed_color: ACCENT,
                text_color: INK,
                border_radius: 0
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.open_modal(CircuitModal::Tasks)); }
            View (
                id: "circuit_wire_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Button (
                    "A 0",
                    id: "circuit_input_0",
                    position: Position::Absolute,
                    left: 17,
                    top: 87,
                    width: 30,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: PANEL,
                    pressed_color: ACCENT,
                    text_color: INK,
                    border_radius: 0
                ) on Tap { CircuitNodes::update(ctx.world, |model| model.toggle_input(0)); }
                Button (
                    "B 0",
                    id: "circuit_input_1",
                    position: Position::Absolute,
                    left: 17,
                    top: 141,
                    width: 30,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: PANEL,
                    pressed_color: ACCENT,
                    text_color: INK,
                    border_radius: 0
                ) on Tap { CircuitNodes::update(ctx.world, |model| model.toggle_input(1)); }
                Button (
                    "C 0",
                    id: "circuit_input_2",
                    position: Position::Absolute,
                    left: 17,
                    top: 195,
                    width: 30,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: PANEL,
                    pressed_color: ACCENT,
                    text_color: INK,
                    border_radius: 0
                ) on Tap { CircuitNodes::update(ctx.world, |model| model.toggle_input(2)); }
                Text (
                    "",
                    id: "circuit_gate_0",
                    position: Position::Absolute,
                    left: 116,
                    top: 136,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_1",
                    position: Position::Absolute,
                    left: 186,
                    top: 136,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_2",
                    position: Position::Absolute,
                    left: 116,
                    top: 190,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_3",
                    position: Position::Absolute,
                    left: 186,
                    top: 82,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_4",
                    position: Position::Absolute,
                    left: 186,
                    top: 190,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_5",
                    position: Position::Absolute,
                    left: 116,
                    top: 82,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "Y\n?",
                    id: "circuit_output",
                    position: Position::Absolute,
                    left: 288,
                    top: 139,
                    width: 24,
                    height: 28,
                    font_size: 8,
                    text_color: BACKGROUND,
                    line_height: 10,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "LIVE TRUTH TABLE",
                    position: Position::Absolute,
                    left: 334,
                    top: 76,
                    width: 125,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "Y = A XOR B",
                    id: "circuit_live_code",
                    position: Position::Absolute,
                    left: 334,
                    top: 94,
                    width: 125,
                    height: 13,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "A B C → Y / 目标",
                    position: Position::Absolute,
                    left: 334,
                    top: 108,
                    width: 125,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_0",
                    position: Position::Absolute,
                    left: 335,
                    top: 121,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_1",
                    position: Position::Absolute,
                    left: 335,
                    top: 134,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_2",
                    position: Position::Absolute,
                    left: 335,
                    top: 147,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_3",
                    position: Position::Absolute,
                    left: 335,
                    top: 160,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_4",
                    position: Position::Absolute,
                    left: 335,
                    top: 173,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_5",
                    position: Position::Absolute,
                    left: 335,
                    top: 186,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_6",
                    position: Position::Absolute,
                    left: 335,
                    top: 199,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_7",
                    position: Position::Absolute,
                    left: 335,
                    top: 212,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "点输出端，再点输入端；拖动模块改变位置。",
                    id: "circuit_wire_status",
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
            View (
                id: "circuit_truth_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "EXHAUSTIVE INPUT TEST",
                    position: Position::Absolute,
                    left: 23,
                    top: 77,
                    width: 250,
                    height: 13,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "A   B   C      EXPECT     ACTUAL     RESULT",
                    position: Position::Absolute,
                    left: 27,
                    top: 98,
                    width: 265,
                    height: 13,
                    font_size: 7,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_0",
                    position: Position::Absolute,
                    left: 27,
                    top: 115,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_1",
                    position: Position::Absolute,
                    left: 27,
                    top: 131,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_2",
                    position: Position::Absolute,
                    left: 27,
                    top: 147,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_3",
                    position: Position::Absolute,
                    left: 27,
                    top: 163,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_4",
                    position: Position::Absolute,
                    left: 27,
                    top: 179,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_5",
                    position: Position::Absolute,
                    left: 27,
                    top: 195,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_6",
                    position: Position::Absolute,
                    left: 27,
                    top: 211,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_7",
                    position: Position::Absolute,
                    left: 27,
                    top: 227,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "TARGET SPECIFICATION",
                    position: Position::Absolute,
                    left: 334,
                    top: 78,
                    width: 125,
                    height: 13,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "不同才亮",
                    id: "circuit_truth_title",
                    position: Position::Absolute,
                    left: 334,
                    top: 100,
                    width: 125,
                    height: 22,
                    font_size: 13,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "A 与 B 不同时，Y 才为 1。",
                    id: "circuit_truth_desc",
                    position: Position::Absolute,
                    left: 334,
                    top: 132,
                    width: 120,
                    height: 54,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::default()
                )
                Text (
                    "未连接输入按 0 求值，但不会通过完整性验证。",
                    position: Position::Absolute,
                    left: 334,
                    top: 190,
                    width: 120,
                    height: 48,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::default()
                )
            }
            View (
                id: "circuit_trace_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "A",
                    id: "circuit_trace_a",
                    position: Position::Absolute,
                    left: 22,
                    top: 83,
                    width: 18,
                    height: 14,
                    font_size: 9,
                    text_color: Color::rgb(182, 204, 211),
                    paragraph: label_style()
                )
                Text (
                    "B",
                    id: "circuit_trace_b",
                    position: Position::Absolute,
                    left: 22,
                    top: 120,
                    width: 18,
                    height: 14,
                    font_size: 9,
                    text_color: Color::rgb(182, 204, 211),
                    paragraph: label_style()
                )
                Text (
                    "C",
                    id: "circuit_trace_c",
                    position: Position::Absolute,
                    left: 22,
                    top: 157,
                    width: 18,
                    height: 14,
                    font_size: 9,
                    text_color: Color::rgb(182, 204, 211),
                    paragraph: label_style()
                )
                Text (
                    "Y",
                    id: "circuit_trace_y",
                    position: Position::Absolute,
                    left: 22,
                    top: 194,
                    width: 18,
                    height: 14,
                    font_size: 9,
                    text_color: ACCENT,
                    paragraph: label_style()
                )
                Text (
                    "LAST 32 SAMPLES / LOGICAL TIME",
                    position: Position::Absolute,
                    left: 22,
                    top: 240,
                    width: 260,
                    height: 12,
                    font_size: 7,
                    text_color: Color::rgb(156, 174, 183),
                    paragraph: label_style()
                )
                Text (
                    "SIGNAL SCANNER",
                    position: Position::Absolute,
                    left: 334,
                    top: 78,
                    width: 125,
                    height: 13,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "STEP / READY",
                    id: "circuit_trace_mode",
                    position: Position::Absolute,
                    left: 334,
                    top: 103,
                    width: 125,
                    height: 16,
                    font_size: 10,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "0 / 32",
                    id: "circuit_trace_count",
                    position: Position::Absolute,
                    left: 334,
                    top: 127,
                    width: 125,
                    height: 28,
                    font_size: 18,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "扫描按顺序遍历输入组合，记录 A/B/C/Y。它不模拟传播延迟。",
                    position: Position::Absolute,
                    left: 334,
                    top: 164,
                    width: 120,
                    height: 70,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::default()
                )
            }
            Button (
                "＋ 逻辑门",
                id: "circuit_footer_0",
                position: Position::Absolute,
                left: 7,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 0); }
            Button (
                "类型 / 删除",
                id: "circuit_footer_1",
                position: Position::Absolute,
                left: 101,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 1); }
            Button (
                "断开连线",
                id: "circuit_footer_2",
                position: Position::Absolute,
                left: 195,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 2); }
            Button (
                "撤销",
                id: "circuit_footer_3",
                position: Position::Absolute,
                left: 289,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 3); }
            Button (
                "✓ 验证",
                id: "circuit_footer_4",
                position: Position::Absolute,
                left: 383,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: ACCENT,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 4); }
            CircuitModalSurface (
                id: "circuit_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                clip_children: true
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Text (
                    "选择逻辑任务",
                    id: "circuit_modal_title",
                    position: Position::Absolute,
                    left: 43,
                    top: 72,
                    width: 330,
                    height: 20,
                    font_size: 12,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "切换会清空当前网络、时序和撤销记录。",
                    id: "circuit_modal_subtitle",
                    position: Position::Absolute,
                    left: 43,
                    top: 105,
                    width: 360,
                    height: 18,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 417,
                    top: 69,
                    width: 28,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 13,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { CircuitNodes::update(ctx.world, CircuitModel::close_modal); }
                Button (
                    "任务 01",
                    id: "circuit_modal_0",
                    position: Position::Absolute,
                    left: 43,
                    top: 128,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 0); }
                Button (
                    "示范布局",
                    id: "circuit_modal_1",
                    position: Position::Absolute,
                    left: 173,
                    top: 128,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 1); }
                Button (
                    "任务 02",
                    id: "circuit_modal_2",
                    position: Position::Absolute,
                    left: 303,
                    top: 128,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 2); }
                Button (
                    "示范布局",
                    id: "circuit_modal_3",
                    position: Position::Absolute,
                    left: 43,
                    top: 183,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 3); }
                Button (
                    "任务 03",
                    id: "circuit_modal_4",
                    position: Position::Absolute,
                    left: 173,
                    top: 183,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 4); }
                Button (
                    "示范布局",
                    id: "circuit_modal_5",
                    position: Position::Absolute,
                    left: 303,
                    top: 183,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 5); }
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
    app.world.insert_resource(CircuitModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(circuit_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Logic Circuit node");
    let nodes = CircuitNodes {
        surface: find("circuit_surface"),
        gate_count: find("circuit_gate_count"),
        task: find("circuit_task"),
        tabs: [
            find("circuit_tab_wire"),
            find("circuit_tab_truth"),
            find("circuit_tab_trace"),
        ],
        pages: [
            find("circuit_wire_page"),
            find("circuit_truth_page"),
            find("circuit_trace_page"),
        ],
        inputs: [
            find("circuit_input_0"),
            find("circuit_input_1"),
            find("circuit_input_2"),
        ],
        gates: core::array::from_fn(|index| {
            find(match index {
                0 => "circuit_gate_0",
                1 => "circuit_gate_1",
                2 => "circuit_gate_2",
                3 => "circuit_gate_3",
                4 => "circuit_gate_4",
                _ => "circuit_gate_5",
            })
        }),
        output: find("circuit_output"),
        live_code: find("circuit_live_code"),
        wire_status: find("circuit_wire_status"),
        live_rows: core::array::from_fn(|index| {
            find(match index {
                0 => "circuit_live_0",
                1 => "circuit_live_1",
                2 => "circuit_live_2",
                3 => "circuit_live_3",
                4 => "circuit_live_4",
                5 => "circuit_live_5",
                6 => "circuit_live_6",
                _ => "circuit_live_7",
            })
        }),
        truth_rows: core::array::from_fn(|index| {
            find(match index {
                0 => "circuit_truth_0",
                1 => "circuit_truth_1",
                2 => "circuit_truth_2",
                3 => "circuit_truth_3",
                4 => "circuit_truth_4",
                5 => "circuit_truth_5",
                6 => "circuit_truth_6",
                _ => "circuit_truth_7",
            })
        }),
        truth_title: find("circuit_truth_title"),
        truth_desc: find("circuit_truth_desc"),
        trace_labels: [
            find("circuit_trace_a"),
            find("circuit_trace_b"),
            find("circuit_trace_c"),
            find("circuit_trace_y"),
        ],
        trace_mode: find("circuit_trace_mode"),
        trace_count: find("circuit_trace_count"),
        footer: [
            find("circuit_footer_0"),
            find("circuit_footer_1"),
            find("circuit_footer_2"),
            find("circuit_footer_3"),
            find("circuit_footer_4"),
        ],
        modal: find("circuit_modal"),
        modal_title: find("circuit_modal_title"),
        modal_subtitle: find("circuit_modal_subtitle"),
        modal_buttons: [
            find("circuit_modal_0"),
            find("circuit_modal_1"),
            find("circuit_modal_2"),
            find("circuit_modal_3"),
            find("circuit_modal_4"),
            find("circuit_modal_5"),
        ],
    };
    app.world.insert_resource(nodes);
    CircuitNodes::sync(&mut app.world);
}
