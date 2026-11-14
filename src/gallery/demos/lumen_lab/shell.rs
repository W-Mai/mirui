use super::board::{LumenBoard, board_tap};
use super::style::{
    ACCENT, BACKGROUND, BOARD, CONTROL, CONTROL_PRESSED, HEADER, HINT, MUTED, PANEL, PANEL_BORDER,
    SUCCESS, TEXT, padding,
};
use crate::gallery::play::lumen::{
    BeamStatus, LEVEL_COUNT, LEVELS, LumenModel, LumenNote, SET_SIZE, SETS,
};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

fn status_label(status: BeamStatus) -> &'static str {
    match status {
        BeamStatus::Lit => "接通",
        BeamStatus::LampsDark => "还有灯没亮",
        BeamStatus::Wall => "光撞墙了",
        BeamStatus::Edge => "光出界了",
        BeamStatus::Loop => "光在绕圈",
    }
}

fn status_color(status: BeamStatus) -> Color {
    match status {
        BeamStatus::Lit => SUCCESS,
        BeamStatus::LampsDark => HINT,
        _ => TEXT,
    }
}

/// What the beam touches, in the small caps under the status.
struct BeamCount {
    mirrors: (u8, u8),
    lamps: (u8, u8),
}

impl fmt::Display for BeamCount {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (hit, mirrors) = self.mirrors;
        let (lit, lamps) = self.lamps;
        if lamps == 0 {
            write!(output, "MIRRORS {hit} / {mirrors}")
        } else {
            write!(output, "MIRRORS {hit}/{mirrors}  LAMPS {lit}/{lamps}")
        }
    }
}

struct NoteText(LumenNote);

impl fmt::Display for NoteText {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            LumenNote::Tip => output.write_str("每面镜子都会用上"),
            LumenNote::Hint(_) => output.write_str("转动高亮的镜子"),
            LumenNote::Fixed => output.write_str("这面镜子是固定的"),
        }
    }
}

/// A level tile: its number over a star once it is solved.
struct TileLabel {
    level: usize,
    done: bool,
}

impl fmt::Display for TileLabel {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "{:02}\n{}",
            self.level + 1,
            if self.done { "★" } else { "☆" }
        )
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

fn tile_text_color(current: bool, done: bool) -> Color {
    if current {
        BACKGROUND
    } else if done {
        SUCCESS
    } else {
        TEXT
    }
}

fn level_completed(mask: u16, level: usize) -> bool {
    mask & (1 << level) != 0
}

#[compose]
fn compose_header(model: model!(LumenModel)) -> Entity {
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
                text: ${ format_args!("PUZZLE {:02} / {LEVEL_COUNT}", model.level_index() + 1) },
                capacity: 14,
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

#[compose]
fn compose_level_intro(model: model!(LumenModel)) -> Entity {
    ui! {
        Row (
            height: 28,
            padding: padding(4, 19),
            align: AlignItems::Center,
            column_gap: 8,
            visible: ${ !model.levels_open() }
        ) {
            Text (
                text: ${ LEVELS[model.level_index()].name },
                capacity: 15,
                id: "lumen_level_name",
                width: 164,
                height: 20,
                font_size: 12,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${ LEVELS[model.level_index()].subtitle },
                capacity: 48,
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

#[compose]
fn compose_playfield(model: model!(LumenModel)) -> Entity {
    ui! {
        Row (height: 204, padding: padding(0, 12), column_gap: 11) {
            View (
                width: 282,
                height: 204,
                clip_children: true
            ) {
                View (id: "lumen_board", grow: 1.0) [
                    LumenBoard { model: model.clone() },
                    TouchAction::None,
                ] on Tap(|event: event!(Tap), rows: query!(LumenBoard, ComputedRect)| {
                    let Some((board, rect)) = rows.get(event.target) else {
                        return false;
                    };
                    let rect = rect.0;
                    board_tap(board, rect, event.x, event.y)
                })
            }
            Column (
                width: 163,
                height: 204,
                row_gap: 6,
                visible: ${ !model.levels_open() }
            ) {
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
                        text: ${ status_label(model.beam_status()) },
                        capacity: 15,
                        id: "lumen_status",
                        height: 20,
                        font_size: 11,
                        text_color: ${ status_color(model.beam_status()) },
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${
                            BeamCount {
                                mirrors: model.mirror_count(),
                                lamps: model.lamp_count(),
                            }
                        },
                        capacity: 24,
                        id: "lumen_status_subtitle",
                        height: 13,
                        font_size: 7,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (height: 1, bg_color: Color::rgb(66, 82, 79))
                    Row (grow: 1.0, align: AlignItems::Center, column_gap: 4) {
                        Text (
                            text: ${ format_args!("{:02}", model.moves().min(99)) },
                            capacity: 2,
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
                                text: ${ format_args!("{} / {LEVEL_COUNT}", model.completed_count()) },
                                capacity: 7,
                                id: "lumen_completed",
                                height: 21,
                                font_size: 12,
                                text_color: Color::rgb(193, 214, 197),
                                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                            )
                            Text (
                                "已完成",
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
                    pressed_text_color: BOARD,
                    text_color: TEXT,
                    border_radius: 8
                ) on Tap { model.reveal_hint(); }
                Button (
                    "↻ 重来",
                    size: ButtonSize::Compact,
                    width: 163,
                    height: 32,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    pressed_text_color: BOARD,
                    text_color: TEXT,
                    border_radius: 8
                ) on Tap { model.reset(); }
                Text (
                    text: ${ NoteText(model.note()) },
                    capacity: 24,
                    id: "lumen_hint_note",
                    grow: 1.0,
                    font_size: 7,
                    text_color: ${ if model.note() == LumenNote::Tip { MUTED } else { HINT } },
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}

#[compose]
fn compose_footer(model: model!(LumenModel)) -> Entity {
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
                "选关",
                size: ButtonSize::Compact,
                width: 93,
                height: 26,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: CONTROL_PRESSED,
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.open_levels(); }
            Button (
                "↺ 撤销",
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
                capacity: 10,
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
                "下一关 →",
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

#[compose]
fn level_tile(model: model!(LumenModel), level: usize) -> Entity {
    ui! {
        Button (
            text: ${
                TileLabel {
                    level,
                    done: level_completed(model.completion_mask(), level),
                }
            },
            capacity: 8,
            size: ButtonSize::Compact,
            grow: 1.0,
            height: 56,
            font_size: 10,
            normal_color: ${ control_color(true, model.level_index() == level) },
            pressed_color: CONTROL_PRESSED,
            text_color: ${
                tile_text_color(
                    model.level_index() == level,
                    level_completed(model.completion_mask(), level),
                )
            },
            border_radius: 7
        ) on Tap { model.select_level(level); }
    }
}

#[compose]
fn level_set(model: model!(LumenModel), set: usize) -> Entity {
    ui! {
        Row (height: 60, column_gap: 6, align: AlignItems::Center) {
            Column (width: 88, height: 56, row_gap: 2, justify: JustifyContent::Center) {
                Text (
                    SETS[set].name,
                    height: 18,
                    font_size: 12,
                    text_color: ACCENT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    SETS[set].rule,
                    height: 15,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
            level_tile (model, set * SET_SIZE)
            level_tile (model, set * SET_SIZE + 1)
            level_tile (model, set * SET_SIZE + 2)
            level_tile (model, set * SET_SIZE + 3)
            level_tile (model, set * SET_SIZE + 4)
        }
    }
}

#[compose]
fn compose_level_modal(model: model!(LumenModel)) -> Entity {
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
        ] {
            Column (
                position: Position::Absolute,
                left: 20,
                top: 24,
                width: 440,
                height: 272,
                padding: Padding::all(12),
                row_gap: 6,
                bg_color: PANEL,
                border_color: ACCENT,
                border_width: 1,
                border_radius: 10
            ) {
                Row (height: 25, align: AlignItems::Center, column_gap: 8) {
                    Text (
                        "选关",
                        grow: 1.0,
                        height: 22,
                        font_size: 12,
                        text_color: ACCENT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ format_args!("★ {} / {LEVEL_COUNT}", model.completed_count()) },
                        capacity: 12,
                        id: "lumen_levels_stars",
                        width: 90,
                        height: 18,
                        font_size: 10,
                        text_color: SUCCESS,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                    )
                    Button (
                        "×",
                        size: ButtonSize::Compact,
                        width: 28,
                        height: 24,
                        font_size: 12,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        pressed_text_color: BOARD,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { model.close_levels(); }
                }
                level_set (model, 0)
                level_set (model, 1)
                level_set (model, 2)
            }
        } on Tap { }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(LumenModel)) {
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
