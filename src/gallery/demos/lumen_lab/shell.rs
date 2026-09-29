use super::board::{LumenBoard, board_tap};
use super::style::{
    ACCENT, BACKGROUND, CONTROL, CONTROL_PRESSED, HEADER, MUTED, PANEL, PANEL_BORDER, SUCCESS,
    TEXT, padding,
};
use crate::gallery::play::lumen::{LEVELS, LumenModel};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn hint_label(hint: Option<usize>) -> &'static str {
    match hint {
        Some(0) => "试试镜片 1",
        Some(1) => "试试镜片 2",
        Some(2) => "试试镜片 3",
        Some(3) => "试试镜片 4",
        Some(4) => "试试镜片 5",
        Some(5) => "试试镜片 6",
        Some(6) => "试试镜片 7",
        _ => "镜片只在两种方向间切换",
    }
}

fn control_color(enabled: bool, active: bool) -> Color {
    if active {
        ACCENT
    } else if enabled {
        CONTROL
    } else {
        Color::rgb(35, 47, 43)
    }
}

fn control_text_color(enabled: bool, active: bool) -> Color {
    if active {
        BACKGROUND
    } else if enabled {
        TEXT
    } else {
        Color::rgb(93, 108, 101)
    }
}

fn mirror_visible(level: usize, mirror: usize) -> bool {
    LEVELS[level].mirrors.len() > mirror
}

fn mirror_left(level: usize, mirror: usize) -> i32 {
    LEVELS[level]
        .mirrors
        .get(mirror)
        .map_or(0, |spec| 31 + i32::from(spec.position.x) * 38)
}

fn mirror_top(level: usize, mirror: usize) -> i32 {
    LEVELS[level]
        .mirrors
        .get(mirror)
        .map_or(0, |spec| 10 + i32::from(spec.position.y) * 38)
}

fn level_completed(mask: u8, level: usize) -> bool {
    mask & (1 << level) != 0
}

#[compose(bind(model))]
fn mirror_labels(model: LumenModel) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 282,
            height: 204
        ) [
            IgnoreHitTest,
        ] {
            Text (
                "1",
                position: Position::Absolute,
                left: ${ mirror_left(model.level_index(), 0) },
                top: ${ mirror_top(model.level_index(), 0) },
                visible: ${ mirror_visible(model.level_index(), 0) },
                width: 7,
                height: 8,
                font_size: 6,
                text_color: Color::rgb(187, 203, 192),
                paragraph: ParagraphStyle::label()
            )
            Text (
                "2",
                position: Position::Absolute,
                left: ${ mirror_left(model.level_index(), 1) },
                top: ${ mirror_top(model.level_index(), 1) },
                visible: ${ mirror_visible(model.level_index(), 1) },
                width: 7,
                height: 8,
                font_size: 6,
                text_color: Color::rgb(187, 203, 192),
                paragraph: ParagraphStyle::label()
            )
            Text (
                "3",
                position: Position::Absolute,
                left: ${ mirror_left(model.level_index(), 2) },
                top: ${ mirror_top(model.level_index(), 2) },
                visible: ${ mirror_visible(model.level_index(), 2) },
                width: 7,
                height: 8,
                font_size: 6,
                text_color: Color::rgb(187, 203, 192),
                paragraph: ParagraphStyle::label()
            )
            Text (
                "4",
                position: Position::Absolute,
                left: ${ mirror_left(model.level_index(), 3) },
                top: ${ mirror_top(model.level_index(), 3) },
                visible: ${ mirror_visible(model.level_index(), 3) },
                width: 7,
                height: 8,
                font_size: 6,
                text_color: Color::rgb(187, 203, 192),
                paragraph: ParagraphStyle::label()
            )
            Text (
                "5",
                position: Position::Absolute,
                left: ${ mirror_left(model.level_index(), 4) },
                top: ${ mirror_top(model.level_index(), 4) },
                visible: ${ mirror_visible(model.level_index(), 4) },
                width: 7,
                height: 8,
                font_size: 6,
                text_color: Color::rgb(187, 203, 192),
                paragraph: ParagraphStyle::label()
            )
            Text (
                "6",
                position: Position::Absolute,
                left: ${ mirror_left(model.level_index(), 5) },
                top: ${ mirror_top(model.level_index(), 5) },
                visible: ${ mirror_visible(model.level_index(), 5) },
                width: 7,
                height: 8,
                font_size: 6,
                text_color: Color::rgb(187, 203, 192),
                paragraph: ParagraphStyle::label()
            )
            Text (
                "7",
                position: Position::Absolute,
                left: ${ mirror_left(model.level_index(), 6) },
                top: ${ mirror_top(model.level_index(), 6) },
                visible: ${ mirror_visible(model.level_index(), 6) },
                width: 7,
                height: 8,
                font_size: 6,
                text_color: Color::rgb(187, 203, 192),
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose(bind(model))]
fn compose_header(model: LumenModel) -> Entity {
    ui! {
        Row (
            height: 35,
            padding: padding(7, 12),
            align: AlignItems::Center,
            column_gap: 8,
            bg_color: HEADER
        ) {
            View (width: 13, height: 13, bg_color: ACCENT, border_radius: 7)
            Text (
                "LUMEN LAB",
                grow: 1.0,
                height: 21,
                font_size: 12,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${ format_args!("PUZZLE {:02} / 05", model.level_index() + 1) },
                text_capacity: 14,
                id: "lumen_puzzle",
                width: 110,
                height: 20,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
        }
    }
}

#[compose(bind(model))]
fn compose_level_intro(model: LumenModel) -> Entity {
    ui! {
        Row (
            height: 28,
            padding: padding(4, 19),
            align: AlignItems::Center,
            column_gap: 8
        ) {
            Text (
                text: ${ LEVELS[model.level_index()].name },
                text_capacity: 15,
                id: "lumen_level_name",
                width: 164,
                height: 20,
                font_size: 12,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${ LEVELS[model.level_index()].subtitle },
                text_capacity: 42,
                id: "lumen_level_subtitle",
                grow: 1.0,
                height: 18,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
        }
    }
}

#[compose(bind(model))]
fn compose_playfield(model: LumenModel) -> Entity {
    ui! {
        Row (height: 204, padding: padding(0, 12), column_gap: 11) {
            View (
                id: "lumen_board",
                width: 282,
                height: 204,
                clip_children: true
            ) [
                LumenBoard { model: model.clone() },
                TouchAction::None,
            ] on Tap { board_tap(&ctx); }
            {
                mirror_labels (model)
            }
            Column (width: 163, height: 204, row_gap: 6) {
                Column (
                    height: 102,
                    padding: padding(9, 14),
                    row_gap: 3,
                    bg_color: PANEL,
                    border_color: PANEL_BORDER,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        text: ${ if model.solved() { "光路接通" } else { "等待点亮" } },
                        text_capacity: 12,
                        id: "lumen_status",
                        height: 20,
                        font_size: 11,
                        text_color: ${ if model.solved() { SUCCESS } else { TEXT } },
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ if model.solved() { "NICE CONNECTION" } else { "FOLLOW THE LIGHT" } },
                        text_capacity: 16,
                        id: "lumen_status_subtitle",
                        height: 13,
                        font_size: 7,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (height: 1, bg_color: Color::rgb(66, 82, 79))
                    Row (grow: 1.0, align: AlignItems::Center, column_gap: 4) {
                        Text (
                            text: ${ format_args!("{:02}", model.moves()) },
                            text_capacity: 2,
                            id: "lumen_moves",
                            width: 42,
                            height: 35,
                            font_size: 24,
                            text_color: ACCENT,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Text (
                            "次旋转",
                            grow: 1.0,
                            height: 18,
                            font_size: 8,
                            text_color: MUTED,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Column (width: 43, height: 38) {
                            Text (
                                text: ${ format_args!("{} / 5", model.completion_mask().count_ones()) },
                                text_capacity: 5,
                                id: "lumen_completed",
                                height: 21,
                                font_size: 12,
                                text_color: Color::rgb(193, 214, 197),
                                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                            )
                            Text (
                                "关卡已点亮",
                                height: 14,
                                font_size: 7,
                                text_color: MUTED,
                                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                            )
                        }
                    }
                }
                Button (
                    "提示一步",
                    size: ButtonSize::Compact,
                    width: 163,
                    height: 32,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 8
                ) on Tap { model.reveal_hint(); }
                Button (
                    "重新摆放",
                    size: ButtonSize::Compact,
                    width: 163,
                    height: 32,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 8
                ) on Tap { model.reset(); }
                Text (
                    text: ${ hint_label(model.hint()) },
                    text_capacity: 33,
                    id: "lumen_hint_note",
                    grow: 1.0,
                    font_size: 7,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}

#[compose(bind(model))]
fn compose_footer(model: LumenModel) -> Entity {
    ui! {
        View (height: 15)
    };
    ui! {
        Row (
            height: 38,
            padding: padding(6, 12),
            column_gap: 7,
            bg_color: Color::rgb(20, 30, 27)
        ) {
            Button (
                "关卡",
                size: ButtonSize::Compact,
                width: 93,
                height: 26,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: CONTROL_PRESSED,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.open_levels(); }
            Button (
                "撤销",
                id: "lumen_undo",
                size: ButtonSize::Compact,
                width: 93,
                height: 26,
                font_size: 10,
                normal_color: ${ control_color(model.can_undo(), false) },
                pressed_color: CONTROL_PRESSED,
                text_color: ${ control_text_color(model.can_undo(), false) },
                border_radius: 7
            ) on Tap { model.undo(); }
            Button (
                text: ${ if model.scan() { "追光 开" } else { "追光 关" } },
                text_capacity: 10,
                id: "lumen_scan",
                size: ButtonSize::Compact,
                width: 113,
                height: 26,
                font_size: 10,
                normal_color: ${ control_color(true, model.scan()) },
                pressed_color: CONTROL_PRESSED,
                text_color: ${ control_text_color(true, model.scan()) },
                border_radius: 7
            ) on Tap { model.toggle_scan(); }
            Button (
                "下一关",
                id: "lumen_next",
                size: ButtonSize::Compact,
                grow: 1.0,
                height: 26,
                font_size: 10,
                normal_color: ${ control_color(true, model.solved()) },
                pressed_color: CONTROL_PRESSED,
                text_color: ${ control_text_color(true, model.solved()) },
                border_radius: 7
            ) on Tap { model.next_level(); }
        }
    }
}

#[compose(bind(model))]
fn compose_level_modal(model: LumenModel) -> Entity {
    ui! {
        View (
            id: "lumen_levels_modal",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            visible: ${ model.levels_open() },
            bg_color: Color::rgba(8, 14, 13, 224)
        ) [
            TouchAction::None,
        ] on Tap { }
        {
            Column (
                position: Position::Absolute,
                left: 24,
                top: 39,
                width: 432,
                height: 242,
                padding: Padding::all(12),
                row_gap: 5,
                bg_color: PANEL,
                border_color: ACCENT,
                border_width: 1,
                border_radius: 10
            ) {
                Row (height: 25, align: AlignItems::Center) {
                    Text (
                        "五封写给光的信",
                        grow: 1.0,
                        height: 22,
                        font_size: 12,
                        text_color: ACCENT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Button (
                        "×",
                        size: ButtonSize::Compact,
                        width: 28,
                        height: 24,
                        font_size: 12,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { model.close_levels(); }
                }
                Text (
                    "可以自由选关；只需把光送进圆环。",
                    height: 17,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "01   第一束光",
                    size: ButtonSize::Compact,
                    width: 408,
                    height: 28,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.select_level(0); }
                Button (
                    "02   折返航线",
                    size: ButtonSize::Compact,
                    width: 408,
                    height: 28,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.select_level(1); }
                Button (
                    "03   绕过岛屿",
                    size: ButtonSize::Compact,
                    width: 408,
                    height: 28,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.select_level(2); }
                Button (
                    "04   交错的光",
                    size: ButtonSize::Compact,
                    width: 408,
                    height: 28,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.select_level(3); }
                Button (
                    "05   最后一公里",
                    size: ButtonSize::Compact,
                    width: 408,
                    height: 28,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.select_level(4); }
                Text (
                    "✓",
                    id: "lumen_level_check_1",
                    position: Position::Absolute,
                    left: 388,
                    top: 66,
                    visible: ${ level_completed(model.completion_mask(), 0) },
                    width: 18,
                    height: 18,
                    font_size: 10,
                    text_color: SUCCESS,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "✓",
                    id: "lumen_level_check_2",
                    position: Position::Absolute,
                    left: 388,
                    top: 99,
                    visible: ${ level_completed(model.completion_mask(), 1) },
                    width: 18,
                    height: 18,
                    font_size: 10,
                    text_color: SUCCESS,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "✓",
                    id: "lumen_level_check_3",
                    position: Position::Absolute,
                    left: 388,
                    top: 132,
                    visible: ${ level_completed(model.completion_mask(), 2) },
                    width: 18,
                    height: 18,
                    font_size: 10,
                    text_color: SUCCESS,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "✓",
                    id: "lumen_level_check_4",
                    position: Position::Absolute,
                    left: 388,
                    top: 165,
                    visible: ${ level_completed(model.completion_mask(), 3) },
                    width: 18,
                    height: 18,
                    font_size: 10,
                    text_color: SUCCESS,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "✓",
                    id: "lumen_level_check_5",
                    position: Position::Absolute,
                    left: 388,
                    top: 198,
                    visible: ${ level_completed(model.completion_mask(), 4) },
                    width: 18,
                    height: 18,
                    font_size: 10,
                    text_color: SUCCESS,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}

#[compose(bind(model))]
pub(super) fn build_widgets(model: LumenModel) {
    ui! {
        Column (width: 480, height: 320, bg_color: BACKGROUND) {
            compose_header (model)
            compose_level_intro (model)
            compose_playfield (model)
            compose_footer (model)
            compose_level_modal (model)
        }
    };
}
