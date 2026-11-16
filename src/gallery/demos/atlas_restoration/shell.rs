use super::geometry::{CLUE_LINE, board_geometry_for_level, clues_for_level};
use super::input::{
    keyboard, surface_drag_end, surface_drag_move, surface_drag_start, surface_tap,
};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::surface_render;
use super::state::{PictureSurface, PictureTrack, direct_picture, next, select_map_level};
use super::style::{APRICOT, BG, CELL, DONE, GRID, LAVENDER, MINT, MUTED, PANEL, TEXT};
use crate::core::i18n::Phrase;
use crate::gallery::play::expeditions::{
    ChapterLine, ChapterTitle, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, LevelNumber,
    LevelTile, MAP_FOOTER, MAP_TITLE, MapSummary, ResultStars, START_CHAPTER, chinese,
    install_expedition_motion, picture_level, result_next, result_title,
};
use crate::gallery::play::font::PlayFontPlugin;
use crate::gallery::play::picture::{
    CHAPTER_MECHANICS, CHAPTER_NAMES, PictureMessage, PictureModel, PictureTool,
};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign, TextVerticalAlign};
use crate::ui::{ComputedRect, IgnoreHitTest};
use core::fmt;

const ATLAS: Phrase = tr!(en: "ATLAS", zh: "幅");
const CLEARED: Phrase = tr!(en: "ATLAS RESTORED", zh: "图谱已复原");
const NEXT: Phrase = tr!(en: "Next →", zh: "下一幅 →");
const OVERLAY: Color = Color::rgba(7, 13, 22, 244);
const LOCKED: Color = Color::rgb(22, 35, 48);

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

struct StatusLabel {
    message: PictureMessage,
    size: u8,
}

impl fmt::Display for StatusLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let phrase = match self.message {
            PictureMessage::Ready => tr!(
                en: "Fill cells to match the numbers on the edges",
                zh: "按边上的数字填格子"
            ),
            PictureMessage::Observed => tr!(
                en: "Cells in a gold frame are given",
                zh: "金框格子是给定的，不能改"
            ),
            PictureMessage::Undone => tr!(en: "Stroke undone", zh: "已撤回上一笔"),
            PictureMessage::Hint(cell) => {
                let (row, column) = (cell / self.size + 1, cell % self.size + 1);
                return if chinese() {
                    write!(out, "已标出第 {row} 行第 {column} 列")
                } else {
                    write!(out, "Revealed row {row}, column {column}")
                };
            }
            PictureMessage::Checked(0) => tr!(en: "No mistakes so far", zh: "目前没有填错"),
            PictureMessage::Checked(errors) => {
                return match (chinese(), errors) {
                    (true, _) => write!(out, "有 {errors} 格填错了，已标出"),
                    (false, 1) => out.write_str("1 cell is wrong, now framed"),
                    (false, _) => write!(out, "{errors} cells are wrong, now framed"),
                };
            }
            PictureMessage::Complete => tr!(en: "Atlas restored", zh: "图谱已复原"),
        };
        out.write_str(phrase.get())
    }
}

/// The board size, and the hints taken if any.
struct Readout {
    size: u8,
    hints: u16,
}

impl fmt::Display for Readout {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}×{}", self.size, self.size)?;
        if self.hints != 0 {
            let hint = if chinese() { "提示" } else { "HINT" };
            write!(out, " · {hint} {}", self.hints)?;
        }
        Ok(())
    }
}

struct ResultStats {
    hints: u16,
    size: u8,
}

impl fmt::Display for ResultStats {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(out, "提示 {} 次 · {}×{}", self.hints, self.size, self.size)
        } else {
            write!(out, "Hints {} · {}×{}", self.hints, self.size, self.size)
        }
    }
}

fn level_size(level: u8) -> u8 {
    picture_level(level)
        .expect("validated Picture level")
        .size()
}

fn chapter_of(level: u8) -> usize {
    usize::from(level / 6).min(5)
}

fn chapter_name(level: u8) -> Phrase {
    CHAPTER_NAMES[chapter_of(level)]
}

fn cell_size(level: u8) -> i32 {
    board_geometry_for_level(level).cell
}

fn row_clue_left(level: u8) -> i32 {
    let geometry = board_geometry_for_level(level);
    geometry.x - geometry.row_clues
}

fn row_clue_width(level: u8) -> i32 {
    board_geometry_for_level(level).row_clues - 4
}

fn row_clue_top(level: u8, line: u8) -> i32 {
    let geometry = board_geometry_for_level(level);
    geometry.y + i32::from(line) * geometry.cell
}

fn column_clue_left(level: u8, line: u8) -> i32 {
    let geometry = board_geometry_for_level(level);
    geometry.x + i32::from(line) * geometry.cell
}

fn column_clue_top(level: u8) -> i32 {
    let geometry = board_geometry_for_level(level);
    geometry.y - geometry.column_clues
}

fn column_clue_height(level: u8) -> i32 {
    board_geometry_for_level(level).column_clues - 2
}

/// The clues and tools give way to an open panel or result, which would
/// otherwise sit over them.
fn playing(panel: ExpeditionPanel, modal: ExpeditionModal) -> bool {
    panel == ExpeditionPanel::None && modal == ExpeditionModal::None
}

/// Where the banner sits: the middle of the board, clear of the clues.
fn banner_origin(level: u8) -> (i32, i32) {
    let geometry = board_geometry_for_level(level);
    let half = geometry.size / 2;
    (geometry.x + half - 80, geometry.y + half - 30)
}

/// The clues of a line that already matches them dim.
fn clue_color(matched: u32, bit: u32) -> Color {
    if matched & (1 << bit) != 0 {
        DONE
    } else {
        TEXT
    }
}

fn tab_color(active: bool) -> Color {
    if active { APRICOT } else { CELL }
}

fn tab_text_color(active: bool) -> Color {
    if active { BG } else { TEXT }
}

#[compose]
fn map_level_button(
    model: model!(PictureModel),
    expedition: model!(ExpeditionUiModel),
    slot: u8,
) -> Entity {
    ui! {
        View (grow: 1.0, height: 57) {
            if ${ expedition.chapter() * 6 + slot > model.unlocked() } {
                View (
                    grow: 1.0,
                    height: 57,
                    padding: Padding::all(4),
                    bg_color: LOCKED,
                    border_radius: 7,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) [
                    IgnoreHitTest,
                ] {
                    Text (
                        text: ${
                            LevelTile {
                                level: expedition.chapter() * 6 + slot,
                                stars: 0,
                                current: false,
                                locked: true,
                            }
                        },
                        capacity: 16,
                        grow: 1.0,
                        height: 49,
                        font_size: 10,
                        text_color: Color::rgb(74, 96, 109),
                        paragraph: ParagraphStyle::label()
                    )
                }
            } else {
                Button (
                    text: ${
                        LevelTile {
                            level: expedition.chapter() * 6 + slot,
                            stars: model.progress().stars(expedition.chapter() * 6 + slot),
                            current: expedition.chapter() * 6 + slot == model.level_index(),
                            locked: false,
                        }
                    },
                    capacity: 16,
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 57,
                    font_size: 10,
                    normal_color: ${ tab_color(expedition.chapter() * 6 + slot == model.level_index()) },
                    pressed_color: APRICOT,
                    text_color: ${ tab_text_color(expedition.chapter() * 6 + slot == model.level_index()) },
                    border_radius: 7
                ) on Tap { select_map_level(&model, &expedition, slot); }
            }
        }
    }
}

#[compose]
fn chapter_tab(expedition: model!(ExpeditionUiModel), chapter: u8) -> Entity {
    ui! {
        Button (
            text: ${ format_args!("0{}", chapter + 1) },
            capacity: 4,
            size: ButtonSize::Compact,
            grow: 1.0,
            height: 25,
            font_size: 10,
            normal_color: ${ tab_color(expedition.chapter() == chapter) },
            pressed_color: APRICOT,
            text_color: ${ tab_text_color(expedition.chapter() == chapter) },
            border_radius: 6
        ) on Tap { expedition.select_chapter(chapter); }
    }
}

#[compose]
fn compose_header(model: model!(PictureModel), expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        Text (
            text: tr!(en : "ATLAS RESTORATION", zh : "星图修复局"),
            id: "picture_title",
            position: Position::Absolute,
            left: 12,
            top: 7,
            width: 108,
            height: 14,
            font_size: 10,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("A{:02}", model.level_index() + 1) },
            capacity: 4,
            id: "picture_level",
            position: Position::Absolute,
            left: 124,
            top: 7,
            width: 28,
            height: 14,
            font_size: 10,
            text_color: APRICOT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ chapter_name(model.level_index()) },
            capacity: 24,
            id: "picture_chapter",
            position: Position::Absolute,
            left: 154,
            top: 7,
            width: 110,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${
                Readout {
                    size: level_size(model.level_index()),
                    hints: model.hints(),
                }
            },
            capacity: 32,
            id: "picture_progress",
            position: Position::Absolute,
            left: 268,
            top: 7,
            width: 138,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Button (
            "ⓘ",
            id: "picture_rules_button",
            position: Position::Absolute,
            left: 414,
            top: 3,
            width: 28,
            height: 22,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CELL,
            pressed_color: APRICOT,
            pressed_text_color: BG,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { expedition.open_rules(); }
    };
    ui! {
        Button (
            "☰",
            id: "picture_map_button",
            position: Position::Absolute,
            left: 446,
            top: 3,
            width: 28,
            height: 22,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CELL,
            pressed_color: APRICOT,
            pressed_text_color: BG,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { expedition.open(model.level_index()); }
    }
}

#[compose]
fn compose_row_clues(model: model!(PictureModel), expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 0,
                }
            },
            capacity: 16,
            id: "pic_row_0",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 0) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 0) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 1,
                }
            },
            capacity: 16,
            id: "pic_row_1",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 1) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 1) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 2,
                }
            },
            capacity: 16,
            id: "pic_row_2",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 2) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 2) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 3,
                }
            },
            capacity: 16,
            id: "pic_row_3",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 3) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 3) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 4,
                }
            },
            capacity: 16,
            id: "pic_row_4",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 4) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 4) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 5,
                }
            },
            capacity: 16,
            id: "pic_row_5",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 5) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 5) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 6,
                }
            },
            capacity: 16,
            id: "pic_row_6",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 6) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 6) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 7,
                }
            },
            capacity: 16,
            id: "pic_row_7",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 7) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 7) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 8,
                }
            },
            capacity: 16,
            id: "pic_row_8",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 8) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 8) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: true,
                    line: 9,
                }
            },
            capacity: 16,
            id: "pic_row_9",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ row_clue_left(model.level_index()) },
            top: ${ row_clue_top(model.level_index(), 9) },
            width: ${ row_clue_width(model.level_index()) },
            height: ${ cell_size(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 9) },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    }
}

#[compose]
fn compose_column_clues(
    model: model!(PictureModel),
    expedition: model!(ExpeditionUiModel),
) -> Entity {
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 0,
                }
            },
            capacity: 16,
            id: "pic_col_0",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 0) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 16) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 1,
                }
            },
            capacity: 16,
            id: "pic_col_1",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 1) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 17) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 2,
                }
            },
            capacity: 16,
            id: "pic_col_2",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 2) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 18) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 3,
                }
            },
            capacity: 16,
            id: "pic_col_3",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 3) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 19) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 4,
                }
            },
            capacity: 16,
            id: "pic_col_4",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 4) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 20) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 5,
                }
            },
            capacity: 16,
            id: "pic_col_5",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 5) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 21) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 6,
                }
            },
            capacity: 16,
            id: "pic_col_6",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 6) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 22) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 7,
                }
            },
            capacity: 16,
            id: "pic_col_7",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 7) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 23) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 8,
                }
            },
            capacity: 16,
            id: "pic_col_8",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 8) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 24) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    };
    ui! {
        Text (
            text: ${
                ClueLabel {
                    level: model.level_index(),
                    row: false,
                    line: 9,
                }
            },
            capacity: 16,
            id: "pic_col_9",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: ${ column_clue_left(model.level_index(), 9) },
            top: ${ column_clue_top(model.level_index()) },
            width: ${ cell_size(model.level_index()) },
            height: ${ column_clue_height(model.level_index()) },
            font_size: 10,
            text_color: ${ clue_color(model.matched_lines(), 25) },
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Center)
                .with_vertical_align(TextVerticalAlign::End)
                .with_line_height(CLUE_LINE)
        )
    }
}

#[compose]
fn compose_clues(model: model!(PictureModel), expedition: model!(ExpeditionUiModel)) -> Entity {
    ui!(compose_row_clues(model, expedition));
    ui!(compose_column_clues(model, expedition))
}

#[compose]
fn compose_tools(model: model!(PictureModel), expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        Button (
            text: tr!(en : "■ Fill", zh : "■ 填色"),
            id: "picture_fill",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: 404,
            top: 40,
            width: 70,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ if model.tool() == PictureTool::Fill { MINT } else { CELL } },
            pressed_color: MINT,
            text_color: ${ if model.tool() == PictureTool::Fill { BG } else { TEXT } },
            border_radius: 7
        ) on Tap { model.set_tool(PictureTool::Fill); }
    };
    ui! {
        Button (
            text: tr!(en : "× Mark", zh : "× 标空"),
            id: "picture_mark",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: 404,
            top: 76,
            width: 70,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ if model.tool() == PictureTool::Mark { LAVENDER } else { CELL } },
            pressed_color: LAVENDER,
            text_color: ${ if model.tool() == PictureTool::Mark { BG } else { TEXT } },
            border_radius: 7
        ) on Tap { model.set_tool(PictureTool::Mark); }
    };
    ui! {
        Button (
            "↺",
            id: "picture_undo",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: 404,
            top: 120,
            width: 33,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 20,
            normal_color: CELL,
            pressed_color: LAVENDER,
            pressed_text_color: BG,
            text_color: ${ if model.history_len() > 0 { TEXT } else { MUTED } },
            border_radius: 7
        ) on Tap { model.undo(); }
    };
    ui! {
        Button (
            "↻",
            id: "picture_restart",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: 441,
            top: 120,
            width: 33,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 20,
            normal_color: CELL,
            pressed_color: APRICOT,
            pressed_text_color: BG,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.restart(); }
    };
    ui! {
        Button (
            text: tr!(en : "? Hint", zh : "? 提示"),
            id: "picture_hint",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: 404,
            top: 156,
            width: 70,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CELL,
            pressed_color: APRICOT,
            pressed_text_color: BG,
            text_color: APRICOT,
            border_radius: 7
        ) on Tap { model.reveal_hint(); }
    };
    ui! {
        Button (
            text: tr!(en : "Check", zh : "检查"),
            id: "picture_check",
            visible: ${ playing(expedition.panel(), model.modal()) },
            position: Position::Absolute,
            left: 404,
            top: 192,
            width: 70,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CELL,
            pressed_color: MINT,
            pressed_text_color: BG,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.check(); }
    };
    ui! {
        Text (
            text: ${
                StatusLabel {
                    message: model.message(),
                    size: level_size(model.level_index()),
                }
            },
            capacity: 64,
            id: "picture_status",
            position: Position::Absolute,
            left: 10,
            top: 296,
            width: 462,
            height: 18,
            font_size: 10,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    }
}

#[compose]
fn compose_banner(model: model!(PictureModel), expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        Column (
            id: "picture_banner",
            visible: ${
                expedition.banner() && expedition.panel() == ExpeditionPanel::None
                    && model.modal() == ExpeditionModal::None
            },
            position: Position::Absolute,
            left: ${ banner_origin(model.level_index()).0 },
            top: ${ banner_origin(model.level_index()).1 },
            width: 160,
            height: 60,
            padding: Padding::all(8),
            row_gap: 2,
            align: AlignItems::Center,
            bg_color: PANEL,
            border_color: APRICOT,
            border_width: 1,
            border_radius: 9
        ) [
            IgnoreHitTest,
        ] {
            Text (
                text: ${
                    LevelNumber {
                        level: model.level_index(),
                        unit: ATLAS,
                    }
                },
                capacity: 16,
                id: "picture_banner_title",
                height: 26,
                font_size: 20,
                text_color: APRICOT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                text: ${ chapter_name(model.level_index()) },
                capacity: 24,
                height: 14,
                font_size: 10,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
fn compose_result_modal(
    model: model!(PictureModel),
    expedition: model!(ExpeditionUiModel),
) -> Entity {
    ui! {
        View (
            id: "picture_result",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: Color::rgba(7, 13, 22, 210),
            visible: ${ model.modal() != ExpeditionModal::None }
        ) [
            TouchAction::None,
        ] {
            Column (
                position: Position::Absolute,
                left: 86,
                top: 62,
                width: 308,
                height: 196,
                padding: Padding::all(14),
                row_gap: 6,
                align: AlignItems::Center,
                bg_color: PANEL,
                border_color: APRICOT,
                border_width: 1,
                border_radius: 10
            ) {
                Text (
                    text: ${ result_title(model.modal(), model.level_index(), CLEARED) },
                    capacity: 24,
                    id: "picture_result_title",
                    width: 280,
                    height: 26,
                    font_size: 20,
                    text_color: APRICOT,
                    paragraph: ParagraphStyle::label()
                )
                View (id: "picture_result_stars", width: 108, height: 34) [
                    ResultStars::new(APRICOT, CELL),
                ]
                Text (
                    text: ${
                        ResultStats {
                            hints: model.hints(),
                            size: level_size(model.level_index()),
                        }
                    },
                    capacity: 32,
                    id: "picture_result_stats",
                    width: 280,
                    height: 14,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: tr!(en : "No hints earns ★★★", zh : "不用提示完成得 ★★★"),
                    width: 280,
                    height: 14,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Row (width: 280, height: 32, column_gap: 8) {
                    Button (
                        text: tr!(en : "↻ Retry", zh : "↻ 重来"),
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 32,
                        font_size: 10,
                        normal_color: CELL,
                        pressed_color: APRICOT,
                        pressed_text_color: BG,
                        text_color: TEXT,
                        border_radius: 7
                    ) on Tap { model.restart(); }
                    Button (
                        text: tr!(en : "☰ Map", zh : "☰ 航图"),
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 32,
                        font_size: 10,
                        normal_color: CELL,
                        pressed_color: APRICOT,
                        pressed_text_color: BG,
                        text_color: TEXT,
                        border_radius: 7
                    ) on Tap { expedition.open(model.level_index()); }
                    Button (
                        text: ${ result_next(model.modal(), NEXT) },
                        capacity: 20,
                        id: "picture_next",
                        size: ButtonSize::Compact,
                        grow: 1.3,
                        height: 32,
                        font_size: 10,
                        normal_color: APRICOT,
                        pressed_color: LAVENDER,
                        text_color: BG,
                        border_radius: 7
                    ) on Tap { next(&model, &expedition); }
                }
            }
        } on Tap { }
    }
}

#[compose]
fn compose_map_panel(model: model!(PictureModel), expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        View (
            id: "picture_map",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: OVERLAY,
            visible: ${ expedition.panel() == ExpeditionPanel::Map }
        ) [
            TouchAction::None,
        ] {
            Column (
                position: Position::Absolute,
                left: 5,
                top: 36,
                width: 470,
                height: 248,
                padding: Padding::all(12),
                row_gap: 6,
                bg_color: PANEL,
                border_color: GRID,
                border_width: 1,
                border_radius: 8
            ) {
                Row (height: 26, align: AlignItems::Center) {
                    Text (
                        text: MAP_TITLE,
                        grow: 1.0,
                        height: 22,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Button (
                        "×",
                        size: ButtonSize::Compact,
                        width: 28,
                        height: 25,
                        font_size: 10,
                        normal_color: CELL,
                        pressed_color: APRICOT,
                        pressed_text_color: BG,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.close(); }
                }
                Text (
                    text: ${
                        MapSummary {
                            cleared: model.progress().completed_count(),
                            chapter: expedition.chapter(),
                        }
                    },
                    capacity: 48,
                    id: "picture_map_summary",
                    height: 14,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Row (height: 26, column_gap: 7) {
                    View (id: "picture_map_chapter_0", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 0)
                    }
                    View (id: "picture_map_chapter_1", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 1)
                    }
                    View (id: "picture_map_chapter_2", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 2)
                    }
                    View (id: "picture_map_chapter_3", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 3)
                    }
                    View (id: "picture_map_chapter_4", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 4)
                    }
                    View (id: "picture_map_chapter_5", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 5)
                    }
                }
                Text (
                    text: ${ CHAPTER_NAMES[usize::from(expedition.chapter())] },
                    capacity: 24,
                    id: "picture_map_chapter_name",
                    height: 16,
                    font_size: 10,
                    text_color: APRICOT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${ CHAPTER_MECHANICS[usize::from(expedition.chapter())] },
                    capacity: 64,
                    id: "picture_map_chapter_mechanic",
                    height: 14,
                    font_size: 10,
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
                    text: MAP_FOOTER,
                    height: 16,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
        } on Tap { }
    }
}

#[compose]
fn rule_line(line: Phrase, color: Color) -> Entity {
    ui! {
        Text (
            text: line,
            height: 17,
            font_size: 10,
            text_color: color,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    }
}

#[compose]
fn compose_rules_panel(expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        View (
            id: "picture_rules",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: OVERLAY,
            visible: ${ expedition.panel() == ExpeditionPanel::Rules }
        ) [
            TouchAction::None,
        ] {
            Column (
                position: Position::Absolute,
                left: 12,
                top: 44,
                width: 456,
                height: 232,
                padding: Padding::all(14),
                row_gap: 7,
                bg_color: PANEL,
                border_color: APRICOT,
                border_width: 1,
                border_radius: 9
            ) {
                Row (height: 26, align: AlignItems::Center) {
                    Text (
                        text: tr!(en : "ATLAS RESTORATION · RULES", zh : "星图修复局 · 规则"),
                        grow: 1.0,
                        height: 22,
                        font_size: 10,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Button (
                        "×",
                        size: ButtonSize::Compact,
                        width: 28,
                        height: 25,
                        font_size: 10,
                        normal_color: CELL,
                        pressed_color: APRICOT,
                        pressed_text_color: BG,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.close(); }
                }
                rule_line (
                    tr!(
                        en : "01  The numbers on an edge are the runs of filled cells in that line.", zh
                        : "01  边缘数字表示这一行或列中连续填色段的长度。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en : "02  2 1 means two filled cells, at least one gap, then one more.", zh :
                        "02  例如 2 1：填两格，至少空一格，再填一格。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en : "03  Pick Fill or Mark and drag to paint; one undo takes back a stroke.", zh
                        :
                        "03  选填色或标空，按住拖动可以连着画；一次撤销撤回一整笔。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en : "04  Cells in a gold frame are given; empty cells need no mark.", zh :
                        "04  金框格子是给定的，不能修改；空格不必全部标叉。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en : "05  A line's numbers dim once it matches them; Check frames wrong cells.",
                        zh :
                        "05  一行填对后，它的数字不再高亮；检查会框出填错的格子。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en :
                        "Keys  arrows/WASD select · Space fill · X mark · Z undo · H hint · C check",
                        zh :
                        "键盘  方向键/WASD 选格 · 空格 填色 · X 标空 · Z 撤销 · H 提示 · C 检查"
                    ),
                    MUTED
                )
            }
        } on Tap { }
    }
}

#[compose]
fn compose_briefing_panel(
    model: model!(PictureModel),
    expedition: model!(ExpeditionUiModel),
) -> Entity {
    ui! {
        View (
            id: "picture_briefing",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: OVERLAY,
            visible: ${ expedition.panel() == ExpeditionPanel::Briefing }
        ) [
            TouchAction::None,
        ] {
            Column (
                position: Position::Absolute,
                left: 32,
                top: 58,
                width: 416,
                height: 204,
                padding: Padding::all(16),
                row_gap: 10,
                align: AlignItems::Center,
                bg_color: PANEL,
                border_color: APRICOT,
                border_width: 1,
                border_radius: 9
            ) {
                Text (
                    text: ${
                        ChapterTitle {
                            chapter: model.level_index() / 6,
                            name: chapter_name(model.level_index()),
                        }
                    },
                    capacity: 48,
                    id: "picture_briefing_title",
                    width: 384,
                    height: 24,
                    font_size: 10,
                    text_color: APRICOT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${ CHAPTER_MECHANICS[chapter_of(model.level_index())] },
                    capacity: 64,
                    id: "picture_briefing_mechanic",
                    width: 384,
                    height: 20,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: tr!(
                        en : "Start with the rows and columns that have big numbers.", zh :
                        "数字大的行和列最好先填。"
                    ),
                    width: 384,
                    height: 17,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: tr!(
                        en : "Runs need a gap between them; gold-framed cells are given.", zh :
                        "两段之间至少空一格；金框格子是给定的。"
                    ),
                    width: 384,
                    height: 17,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Button (
                    text: START_CHAPTER,
                    size: ButtonSize::Compact,
                    width: 190,
                    height: 31,
                    font_size: 10,
                    normal_color: APRICOT,
                    pressed_color: LAVENDER,
                    text_color: BG,
                    border_radius: 7
                ) on Tap { expedition.close(); }
            }
        } on Tap { }
    }
}

#[compose]
fn summary_tab(expedition: model!(ExpeditionUiModel), chapter: u8) -> Entity {
    ui! {
        Button (
            text: ${ format_args!("0{}", chapter + 1) },
            capacity: 4,
            size: ButtonSize::Compact,
            grow: 1.0,
            height: 26,
            font_size: 10,
            normal_color: CELL,
            pressed_color: APRICOT,
            pressed_text_color: BG,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { expedition.open(chapter * 6); }
    }
}

#[compose]
fn compose_summary_panel(
    model: model!(PictureModel),
    expedition: model!(ExpeditionUiModel),
) -> Entity {
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
        ] {
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
                    text: tr!(
                        en : "EXPEDITION LOG · ATLAS RESTORATION", zh :
                        "远征档案 · 星图修复局"
                    ),
                    height: 25,
                    font_size: 10,
                    text_color: APRICOT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: tr!(
                        en : "All 36 atlases restored. Tap a chapter to play it again.", zh :
                        "36 幅图谱全部复原。点章节编号可以重新挑战。"
                    ),
                    height: 16,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: model.progress(),
                            chapter: 0,
                            name: CHAPTER_NAMES[0],
                        }
                    },
                    capacity: 48,
                    id: "picture_summary_line_0",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: model.progress(),
                            chapter: 1,
                            name: CHAPTER_NAMES[1],
                        }
                    },
                    capacity: 48,
                    id: "picture_summary_line_1",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: model.progress(),
                            chapter: 2,
                            name: CHAPTER_NAMES[2],
                        }
                    },
                    capacity: 48,
                    id: "picture_summary_line_2",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: model.progress(),
                            chapter: 3,
                            name: CHAPTER_NAMES[3],
                        }
                    },
                    capacity: 48,
                    id: "picture_summary_line_3",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: model.progress(),
                            chapter: 4,
                            name: CHAPTER_NAMES[4],
                        }
                    },
                    capacity: 48,
                    id: "picture_summary_line_4",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: model.progress(),
                            chapter: 5,
                            name: CHAPTER_NAMES[5],
                        }
                    },
                    capacity: 48,
                    id: "picture_summary_line_5",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Row (height: 27, column_gap: 5) {
                    summary_tab (expedition, 0)
                    summary_tab (expedition, 1)
                    summary_tab (expedition, 2)
                    summary_tab (expedition, 3)
                    summary_tab (expedition, 4)
                    summary_tab (expedition, 5)
                }
            }
        } on Tap { }
    }
}

#[compose]
fn build_widgets(model: model!(PictureModel), expedition: model!(ExpeditionUiModel)) {
    ui! {
        View (id: "picture_surface", width: 480, height: 320, clip_children: true) [
            PictureSurface {
                model: model.clone(),
                expedition: expedition.clone(),
            },
            PictureTrack::default(),
            TouchAction::None,
        ] {
            compose_header (model, expedition)
            compose_clues (model, expedition)
            compose_tools (model, expedition)
            compose_banner (model, expedition)
            compose_result_modal (model, expedition)
            compose_map_panel (model, expedition)
            compose_rules_panel (expedition)
            compose_briefing_panel (model, expedition)
            compose_summary_panel (model, expedition)
        } on Tap(|event: event!(Tap), rows: query!(PictureSurface, optional ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.map(|rect| rect.0);
            surface_tap(surface, rect, event.x, event.y)
        }) on DragStart(|event: event!(DragStart), rows: query!(PictureSurface, optional ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.map(|rect| rect.0);
            surface_drag_start(surface, rect, event.x, event.y)
        }) on DragMove(|event: event!(DragMove), rows: query!(PictureSurface, optional ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.map(|rect| rect.0);
            surface_drag_move(surface, rect, event.x, event.y)
        }) on DragEnd(|event: event!(DragEnd), rows: query!(PictureSurface)| {
            let Some(surface) = rows.get(event.target) else {
                return false;
            };
            surface_drag_end(surface, false)
        }) on DragCancel(|event: event!(DragCancel), rows: query!(PictureSurface)| {
            let Some(surface) = rows.get(event.target) else {
                return false;
            };
            surface_drag_end(surface, true)
        })
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(PlayFontPlugin);
    #[cfg(feature = "persistence")]
    install_persistence(app);
    let model = app.add_model(PictureModel::default()).finish();
    let expedition = app.add_model(ExpeditionUiModel::default()).finish();
    install_expedition_motion(app, &expedition);
    app.add_system(direct_picture::system(model.clone(), expedition.clone()));
    app.on_event(
        Key::EVENT,
        keyboard::callback(model.clone(), expedition.clone()),
    );
    app.with_widget(surface_render::view());
    app.compose(parent, build_widgets(model, expedition));
}
