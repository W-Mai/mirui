use super::geometry::{board_geometry_for_level, clues_for_level};
use super::input::{PictureKeyboardPlugin, surface_gesture};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::surface_view;
use super::state::{PictureExpeditionState, PictureSurface};
use super::style::{APRICOT, BG, CELL, GRID, LAVENDER, MINT, MUTED, PANEL, TEXT};
use crate::gallery::play::expeditions::{
    ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, picture_level,
};
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::picture::{
    CHAPTER_MECHANICS, CHAPTER_NAMES, PictureMessage, PictureModel, PictureProgress, PictureTool,
};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

struct ClueLabel {
    level: u8,
    row: bool,
    line: u8,
}

impl fmt::Display for ClueLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = picture_level(self.level).expect("validated Picture level");
        if self.line >= level.size() {
            return Ok(());
        }
        let mut clues = [0; 5];
        let len = clues_for_level(self.level, self.row, self.line, &mut clues);
        for (index, clue) in clues[..len].iter().enumerate() {
            if index != 0 {
                out.write_str(if self.row { " " } else { "\n" })?;
            }
            write!(out, "{clue}")?;
        }
        Ok(())
    }
}

struct StatusLabel(PictureMessage);

impl fmt::Display for StatusLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            PictureMessage::Ready => out.write_str("读出行列线索，修复失落图谱"),
            PictureMessage::Observed => out.write_str("观测点已锁定，不能覆盖"),
            PictureMessage::Undone => out.write_str("已撤回上一笔"),
            PictureMessage::Hint(cell) => write!(out, "提示已校准格点 {}", cell + 1),
            PictureMessage::Checked(0) => out.write_str("当前标记没有冲突"),
            PictureMessage::Checked(errors) => write!(out, "发现 {errors} 个冲突标记"),
            PictureMessage::Complete => out.write_str("图谱修复完成"),
        }
    }
}

struct MapLevelLabel {
    level: u8,
    current: u8,
    stars: u8,
}

impl fmt::Display for MapLevelLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.stars != 0 {
            write!(out, "{:02}\n{} STAR", self.level + 1, self.stars)
        } else if self.level == self.current {
            write!(out, "{:02}\nPLAY", self.level + 1)
        } else {
            write!(out, "{:02}\nOPEN", self.level + 1)
        }
    }
}

struct LockedLevelLabel(u8);

impl fmt::Display for LockedLevelLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{:02}\nLOCK", self.0 + 1)
    }
}

struct SummaryLabel {
    chapter: u8,
    completed: u8,
    stars: u8,
}

impl fmt::Display for SummaryLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "0{}  {}   {}/6   {}/18 ★",
            self.chapter + 1,
            CHAPTER_NAMES[usize::from(self.chapter)],
            self.completed,
            self.stars
        )
    }
}

fn chapter_summary(progress: PictureProgress, chapter: u8) -> SummaryLabel {
    let mut completed = 0;
    let mut stars = 0;
    for slot in 0..6 {
        let level = chapter * 6 + slot;
        if progress.completed(level) {
            completed += 1;
            stars += progress.stars(level);
        }
    }
    SummaryLabel {
        chapter,
        completed,
        stars,
    }
}

fn next_label(modal: ExpeditionModal, level: u8, unlocked: u8) -> &'static str {
    match modal {
        ExpeditionModal::Result => "下一幅",
        ExpeditionModal::Final => "重新观测",
        ExpeditionModal::None if level < unlocked => "换一幅",
        ExpeditionModal::None => "第一幅",
    }
}

fn map_level_color(level: u8, current: u8) -> Color {
    if level == current { APRICOT } else { CELL }
}

fn map_level_text_color(level: u8, current: u8) -> Color {
    if level == current { BG } else { TEXT }
}

fn level_size(level: u8) -> u8 {
    picture_level(level)
        .expect("validated Picture level")
        .size()
}

fn row_clue_top(level: u8, line: u8) -> i32 {
    let geometry = board_geometry_for_level(level);
    geometry.y + i32::from(line) * geometry.cell
}

fn column_clue_left(level: u8, line: u8) -> i32 {
    let geometry = board_geometry_for_level(level);
    geometry.x + i32::from(line) * geometry.cell
}

#[compose(bind(model, expedition))]
fn map_level_button(model: PictureModel, expedition: ExpeditionUiModel, slot: u8) -> Entity {
    ui! {
        View (grow: 1.0, height: 57) {
            if ${ expedition.chapter() * 6 + slot > model.unlocked() } {
                View (
                    grow: 1.0,
                    height: 57,
                    padding: Padding::all(4),
                    bg_color: Color::rgb(22, 35, 48),
                    border_radius: 7,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) [IgnoreHitTest] {
                    Text (
                        text: ${ LockedLevelLabel(expedition.chapter() * 6 + slot) },
                        text_capacity: 8,
                        grow: 1.0,
                        height: 49,
                        font_size: 10,
                        text_color: Color::rgb(74, 96, 109),
                        paragraph: ParagraphStyle::label()
                    )
                }
            } else {
                Button (
                    text: ${ MapLevelLabel {
                        level: expedition.chapter() * 6 + slot,
                        current: model.level_index(),
                        stars: model.progress().stars(expedition.chapter() * 6 + slot),
                    } },
                    text_capacity: 20,
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 57,
                    font_size: 10,
                    normal_color: ${ map_level_color(expedition.chapter() * 6 + slot, model.level_index()) },
                    pressed_color: APRICOT,
                    text_color: ${ map_level_text_color(expedition.chapter() * 6 + slot, model.level_index()) },
                    border_radius: 7
                ) on Tap {
                    let level = expedition.chapter() * 6 + slot;
                    if level <= model.unlocked() {
                        let briefing = level > 0
                            && level % 6 == 0
                            && !model.progress().completed(level);
                        model.select_level(level);
                        if briefing {
                            expedition.open_briefing();
                        } else {
                            expedition.close();
                        }
                    }
                }
            }
        }
    }
}

#[compose(bind(model))]
fn compose_header(model: PictureModel) -> Entity {
    ui! {
            Text (
                "ATLAS RESTORATION",
                position: Position::Absolute,
                left: 15,
                top: 7,
                width: 230,
                height: 22,
                font_size: 14,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
    };
    ui! {
            Text (
                text: ${ CHAPTER_NAMES[usize::from(model.level_index() / 6)] },
                text_capacity: 32,
                id: "picture_chapter",
                position: Position::Absolute,
                left: 247,
                top: 10,
                width: 127,
                height: 16,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ format_args!("A{:02} / 36", model.level_index() + 1) },
                text_capacity: 16,
                id: "picture_level",
                position: Position::Absolute,
                left: 383,
                top: 9,
                width: 81,
                height: 17,
                font_size: 9,
                text_color: MINT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    }
}

#[compose(bind(model))]
fn compose_row_clues(model: PictureModel) -> Entity {
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 0 } },
                text_capacity: 16,
                id: "pic_row_0",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 0) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 1 } },
                text_capacity: 16,
                id: "pic_row_1",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 1) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 2 } },
                text_capacity: 16,
                id: "pic_row_2",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 2) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 3 } },
                text_capacity: 16,
                id: "pic_row_3",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 3) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 4 } },
                text_capacity: 16,
                id: "pic_row_4",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 4) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 5 } },
                text_capacity: 16,
                id: "pic_row_5",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 5) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 6 } },
                text_capacity: 16,
                id: "pic_row_6",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 6) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 7 } },
                text_capacity: 16,
                id: "pic_row_7",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 7) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 8 } },
                text_capacity: 16,
                id: "pic_row_8",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 8) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: true, line: 9 } },
                text_capacity: 16,
                id: "pic_row_9",
                position: Position::Absolute,
                left: 20,
                top: ${ row_clue_top(model.level_index(), 9) },
                width: ${ board_geometry_for_level(model.level_index()).x - 28 },
                height: ${ board_geometry_for_level(model.level_index()).cell },
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    }
}

#[compose(bind(model))]
fn compose_column_clues(model: PictureModel) -> Entity {
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 0 } },
                text_capacity: 16,
                id: "pic_col_0",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 0) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 1 } },
                text_capacity: 16,
                id: "pic_col_1",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 1) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 2 } },
                text_capacity: 16,
                id: "pic_col_2",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 2) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 3 } },
                text_capacity: 16,
                id: "pic_col_3",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 3) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 4 } },
                text_capacity: 16,
                id: "pic_col_4",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 4) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 5 } },
                text_capacity: 16,
                id: "pic_col_5",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 5) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 6 } },
                text_capacity: 16,
                id: "pic_col_6",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 6) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 7 } },
                text_capacity: 16,
                id: "pic_col_7",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 7) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 8 } },
                text_capacity: 16,
                id: "pic_col_8",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 8) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    };
    ui! {
            Text (
                text: ${ ClueLabel { level: model.level_index(), row: false, line: 9 } },
                text_capacity: 16,
                id: "pic_col_9",
                position: Position::Absolute,
                left: ${ column_clue_left(model.level_index(), 9) },
                top: 43,
                width: ${ board_geometry_for_level(model.level_index()).cell },
                height: ${ (board_geometry_for_level(model.level_index()).y - 45).max(12) },
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
    }
}

#[compose(bind(model))]
fn compose_clues(model: PictureModel) -> Entity {
    ui!(compose_row_clues(model));
    ui!(compose_column_clues(model))
}

#[compose(bind(model))]
fn compose_tools(model: PictureModel) -> Entity {
    ui! {
            Text (
                "OBSERVATION TOOLS",
                position: Position::Absolute,
                left: 335,
                top: 63,
                width: 119,
                height: 15,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
    };
    ui! {
            Button (
                "FILL",
                id: "picture_fill",
                position: Position::Absolute,
                left: 336,
                top: 86,
                width: 56,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: ${ if model.tool() == PictureTool::Fill { MINT } else { CELL } },
                pressed_color: MINT,
                text_color: ${ if model.tool() == PictureTool::Fill { BG } else { TEXT } },
                border_radius: 7
            ) on Tap { model.set_tool(PictureTool::Fill); }
    };
    ui! {
            Button (
                "MARK",
                id: "picture_mark",
                position: Position::Absolute,
                left: 398,
                top: 86,
                width: 56,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: ${ if model.tool() == PictureTool::Mark { LAVENDER } else { CELL } },
                pressed_color: LAVENDER,
                text_color: ${ if model.tool() == PictureTool::Mark { BG } else { TEXT } },
                border_radius: 7
            ) on Tap { model.set_tool(PictureTool::Mark); }
    };
    ui! {
            Button (
                "UNDO",
                position: Position::Absolute,
                left: 336,
                top: 126,
                width: 118,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: LAVENDER,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.undo(); }
    };
    ui! {
            Button (
                "REVEAL ONE",
                position: Position::Absolute,
                left: 336,
                top: 161,
                width: 118,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.reveal_hint(); }
    };
    ui! {
            Button (
                "CHECK",
                position: Position::Absolute,
                left: 336,
                top: 196,
                width: 118,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: MINT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.check(); }
    };
    ui! {
            Text (
                text: ${ format_args!(
                    "{}×{}   UNDO {:02}   HINT {}",
                    level_size(model.level_index()),
                    level_size(model.level_index()),
                    model.history_len(),
                    model.hints()
                ) },
                text_capacity: 48,
                id: "picture_progress",
                position: Position::Absolute,
                left: 334,
                top: 232,
                width: 121,
                height: 14,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
    }
}

#[compose(bind(model, expedition))]
fn compose_footer(model: PictureModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            Text (
                text: ${ StatusLabel(model.message()) },
                text_capacity: 48,
                id: "picture_status",
                position: Position::Absolute,
                left: 18,
                top: 274,
                width: 126,
                height: 18,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
    };
    ui! {
            Button (
                "CHAPTERS",
                position: Position::Absolute,
                left: 150,
                top: 269,
                width: 79,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { expedition.open(model.level_index()); }
    };
    ui! {
            Button (
                "RULES",
                position: Position::Absolute,
                left: 235,
                top: 269,
                width: 82,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { expedition.open_rules(); }
    };
    ui! {
            Button (
                "RESET",
                position: Position::Absolute,
                left: 324,
                top: 269,
                width: 61,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.restart(); }
    };
    ui! {
            Button (
                text: ${ next_label(model.modal(), model.level_index(), model.unlocked()) },
                text_capacity: 16,
                id: "picture_next",
                position: Position::Absolute,
                left: 391,
                top: 269,
                width: 75,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: MINT,
                pressed_color: LAVENDER,
                text_color: BG,
                border_radius: 7
            ) on Tap {
                let level = model.level_index();
                match model.modal() {
                    ExpeditionModal::Final => expedition.open_summary(),
                    ExpeditionModal::Result => {
                        model.continue_campaign();
                        if level % 6 == 5 {
                            expedition.open_briefing();
                        }
                    }
                    ExpeditionModal::None => {
                        let next = if level < model.unlocked() { level + 1 } else { 0 };
                        model.select_level(next);
                    }
                }
            }
    }
}

#[compose(bind(model, expedition))]
fn compose_result_modal(model: PictureModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            View (
                id: "picture_result",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244),
                visible: ${ model.modal() != ExpeditionModal::None }
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 48,
                    top: 66,
                    width: 384,
                    height: 188,
                    padding: Padding::all(18),
                    row_gap: 13,
                    bg_color: PANEL,
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        text: ${ if model.modal() == ExpeditionModal::Final {
                            "远征完成"
                        } else if model.level_index() % 6 == 5 {
                            "章节完成"
                        } else {
                            "图谱已复原"
                        } },
                        text_capacity: 24,
                        id: "picture_result_title",
                        height: 28,
                        font_size: 15,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        text: ${ format_args!(
                            "星级 {} / 3   提示 {} 次   图纸 {}×{}",
                            model.progress().stars(model.level_index()),
                            model.hints(),
                            level_size(model.level_index()),
                            level_size(model.level_index())
                        ) },
                        text_capacity: 64,
                        id: "picture_result_stats",
                        height: 22,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "下一幅图已经开放；提示不会影响关卡解锁。",
                        height: 18,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Row (height: 34, column_gap: 8) {
                        Button (
                            "重新挑战",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { model.restart(); }
                        Button (
                            "查看航图",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { expedition.open(model.level_index()); }
                        Button (
                            "继续远征 →",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: APRICOT,
                            pressed_color: LAVENDER,
                            text_color: BG,
                            border_radius: 7
                        ) on Tap {
                            let level = model.level_index();
                            match model.modal() {
                                ExpeditionModal::Final => expedition.open_summary(),
                                ExpeditionModal::Result => {
                                    model.continue_campaign();
                                    if level % 6 == 5 {
                                        expedition.open_briefing();
                                    }
                                }
                                ExpeditionModal::None => {
                                    let next = if level < model.unlocked() { level + 1 } else { 0 };
                                    model.select_level(next);
                                }
                            }
                        }
                    }
                }
            }
    }
}

#[compose(bind(model, expedition))]
fn compose_map_panel(model: PictureModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            View (
                id: "picture_map",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244),
                visible: ${ expedition.panel() == ExpeditionPanel::Map }
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 5,
                    top: 36,
                    width: 470,
                    height: 239,
                    padding: Padding::all(12),
                    row_gap: 5,
                    bg_color: PANEL,
                    border_color: GRID,
                    border_width: 1,
                    border_radius: 8
                ) {
                    Row (height: 26, align: AlignItems::Center) {
                        Text (
                            "EXPEDITION MAP",
                            grow: 1.0,
                            height: 22,
                            font_size: 13,
                            text_color: TEXT,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "×",
                            size: ButtonSize::Compact,
                            width: 28,
                            height: 25,
                            font_size: 12,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { expedition.close(); }
                    }
                    Text (
                        text: ${ format_args!(
                            "COMPLETE {:02} / 36   CHAPTER {:02} / 06",
                            model.progress().completed_count(),
                            expedition.chapter() + 1
                        ) },
                        text_capacity: 48,
                        id: "picture_map_summary",
                        height: 14,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 26, column_gap: 7) {
                        Button (
                            "01",
                            id: "picture_map_chapter_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ if expedition.chapter() == 0 { APRICOT } else { CELL } },
                            pressed_color: APRICOT,
                            text_color: ${ if expedition.chapter() == 0 { BG } else { TEXT } },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(0); }
                        Button (
                            "02",
                            id: "picture_map_chapter_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ if expedition.chapter() == 1 { APRICOT } else { CELL } },
                            pressed_color: APRICOT,
                            text_color: ${ if expedition.chapter() == 1 { BG } else { TEXT } },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(1); }
                        Button (
                            "03",
                            id: "picture_map_chapter_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ if expedition.chapter() == 2 { APRICOT } else { CELL } },
                            pressed_color: APRICOT,
                            text_color: ${ if expedition.chapter() == 2 { BG } else { TEXT } },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(2); }
                        Button (
                            "04",
                            id: "picture_map_chapter_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ if expedition.chapter() == 3 { APRICOT } else { CELL } },
                            pressed_color: APRICOT,
                            text_color: ${ if expedition.chapter() == 3 { BG } else { TEXT } },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(3); }
                        Button (
                            "05",
                            id: "picture_map_chapter_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ if expedition.chapter() == 4 { APRICOT } else { CELL } },
                            pressed_color: APRICOT,
                            text_color: ${ if expedition.chapter() == 4 { BG } else { TEXT } },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(4); }
                        Button (
                            "06",
                            id: "picture_map_chapter_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ if expedition.chapter() == 5 { APRICOT } else { CELL } },
                            pressed_color: APRICOT,
                            text_color: ${ if expedition.chapter() == 5 { BG } else { TEXT } },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(5); }
                    }
                    Text (
                        text: ${ CHAPTER_NAMES[usize::from(expedition.chapter())] },
                        text_capacity: 32,
                        id: "picture_map_chapter_name",
                        height: 16,
                        font_size: 12,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ CHAPTER_MECHANICS[usize::from(expedition.chapter())] },
                        text_capacity: 64,
                        id: "picture_map_chapter_mechanic",
                        height: 14,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 58, column_gap: 7) {
                        View (id: "picture_map_level_0", grow: 1.0, height: 57) {
                            map_level_button (model, expedition, 0)
                        }
                        View (id: "picture_map_level_1", grow: 1.0, height: 57) {
                            map_level_button (model, expedition, 1)
                        }
                        View (id: "picture_map_level_2", grow: 1.0, height: 57) {
                            map_level_button (model, expedition, 2)
                        }
                        View (id: "picture_map_level_3", grow: 1.0, height: 57) {
                            map_level_button (model, expedition, 3)
                        }
                        View (id: "picture_map_level_4", grow: 1.0, height: 57) {
                            map_level_button (model, expedition, 4)
                        }
                        View (id: "picture_map_level_5", grow: 1.0, height: 57) {
                            map_level_button (model, expedition, 5)
                        }
                    }
                    Text (
                        "6 CHAPTERS × 6 LEVELS · COMPLETE TO UNLOCK",
                        height: 16,
                        font_size: 7,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
    }
}

#[compose(bind(expedition))]
fn compose_rules_panel(expedition: ExpeditionUiModel) -> Entity {
    ui! {
            View (
                id: "picture_rules",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244),
                visible: ${ expedition.panel() == ExpeditionPanel::Rules }
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 30,
                    top: 54,
                    width: 420,
                    height: 212,
                    padding: Padding::all(16),
                    row_gap: 9,
                    bg_color: PANEL,
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Row (height: 26, align: AlignItems::Center) {
                        Text (
                            "ATLAS RESTORATION · RULES",
                            grow: 1.0,
                            height: 22,
                            font_size: 12,
                            text_color: APRICOT,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "×",
                            size: ButtonSize::Compact,
                            width: 28,
                            height: 25,
                            font_size: 12,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { expedition.close(); }
                    }
                    Text (
                        "01  边缘数字表示这一行或列中连续填色段的长度。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "02  例如 2 1：填两格，至少空一格，再填一格。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "03  选择填色或标空，按住拖动；同一笔画可一次撤销。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "04  青点是固定观测点，不可修改；空格不必全部标叉。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "05  方向键选格，空格使用当前工具，X 键标空。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
    }
}

#[compose(bind(model, expedition))]
fn compose_briefing_panel(model: PictureModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            View (
                id: "picture_briefing",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244),
                visible: ${ expedition.panel() == ExpeditionPanel::Briefing }
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 42,
                    top: 58,
                    width: 396,
                    height: 204,
                    padding: Padding::all(17),
                    row_gap: 10,
                    bg_color: PANEL,
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        text: ${ format_args!(
                            "第 {} 章 · {}",
                            model.level_index() / 6 + 1,
                            CHAPTER_NAMES[usize::from(model.level_index() / 6)]
                        ) },
                        text_capacity: 48,
                        id: "picture_briefing_title",
                        height: 24,
                        font_size: 13,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "新机制解锁 · 先理解规则，再启程",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        text: ${ CHAPTER_MECHANICS[usize::from(model.level_index() / 6)] },
                        text_capacity: 64,
                        id: "picture_briefing_mechanic",
                        height: 20,
                        font_size: 11,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "尺寸增加时优先处理大数字与完整行。",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "段与段之间至少隔一个空格；青点观测不可修改。",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Button (
                        "开始这一章 →",
                        size: ButtonSize::Compact,
                        width: 190,
                        height: 31,
                        font_size: 9,
                        normal_color: APRICOT,
                        pressed_color: LAVENDER,
                        text_color: BG,
                        border_radius: 7
                    ) on Tap { expedition.close(); }
                }
            }
    }
}

#[compose(bind(model, expedition))]
fn compose_summary_panel(model: PictureModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            View (
                id: "picture_summary",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 248),
                visible: ${ expedition.panel() == ExpeditionPanel::Summary }
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 36,
                    top: 38,
                    width: 408,
                    height: 244,
                    padding: Padding::all(15),
                    row_gap: 5,
                    bg_color: PANEL,
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "远征档案 · 星图修复局",
                        height: 25,
                        font_size: 13,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "36 关全部完成；每章记录保留，可随时重访。",
                        height: 16,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        text: ${ chapter_summary(model.progress(), 0) },
                        text_capacity: 48,
                        id: "picture_summary_line_0",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ chapter_summary(model.progress(), 1) },
                        text_capacity: 48,
                        id: "picture_summary_line_1",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ chapter_summary(model.progress(), 2) },
                        text_capacity: 48,
                        id: "picture_summary_line_2",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ chapter_summary(model.progress(), 3) },
                        text_capacity: 48,
                        id: "picture_summary_line_3",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ chapter_summary(model.progress(), 4) },
                        text_capacity: 48,
                        id: "picture_summary_line_4",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ chapter_summary(model.progress(), 5) },
                        text_capacity: 48,
                        id: "picture_summary_line_5",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 27, column_gap: 5) {
                        Button (
                            "01",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { expedition.open(0); }
                        Button (
                            "02",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { expedition.open(6); }
                        Button (
                            "03",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { expedition.open(12); }
                        Button (
                            "04",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { expedition.open(18); }
                        Button (
                            "05",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { expedition.open(24); }
                        Button (
                            "06",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { expedition.open(30); }
                    }
                }
            }
    }
}

#[compose(bind(model, expedition))]
fn build_widgets(model: PictureModel, expedition: ExpeditionUiModel) {
    ui! {
        View (id: "picture_surface", width: 480, height: 320, clip_children: true) [
            PictureSurface { model: model.clone() },
            PictureExpeditionState {
                expedition: expedition.clone(),
            },
            TouchAction::None,
        ] on Tap { surface_gesture(&ctx); } on DragStart { surface_gesture(&ctx); } on DragMove { surface_gesture(&ctx); } on DragEnd { surface_gesture(&ctx); } on DragCancel { surface_gesture(&ctx); }
        {
            compose_header (model)
            compose_clues (model)
            compose_tools (model)
            compose_footer (model, expedition)
            compose_result_modal (model, expedition)
            compose_map_panel (model, expedition)
            compose_rules_panel (expedition)
            compose_briefing_panel (model, expedition)
            compose_summary_panel (model, expedition)
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    register_play_font(&mut app.world);
    let model = app.add_model(PictureModel::default());
    let expedition = app.add_model(ExpeditionUiModel::default());
    app.add_plugin(PictureKeyboardPlugin::new(
        model.clone(),
        expedition.clone(),
    ));
    #[cfg(feature = "persistence")]
    install_persistence(app, model.clone());
    app.with_widget(surface_view());
    app.compose(parent, |cx| build_widgets(cx, model, expedition));
}
