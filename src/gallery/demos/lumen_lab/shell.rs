use super::board::{BOARD_HEIGHT, BOARD_WIDTH, LumenBoard, board_tap};
use super::stars::{LumenStars, StarRow};
use super::style::{
    ACCENT, BACKGROUND, BOARD, CONTROL, CONTROL_DISABLED, CONTROL_PRESSED, DIVIDER, HEADER, HINT,
    MUTED, PANEL, PANEL_BORDER, SCRIM, SUCCESS, TEXT, TEXT_DISABLED, padding,
};
use crate::core::i18n::{Locale, Phrase, locale};
use crate::gallery::play::lumen::{
    BeamStatus, LEVEL_COUNT, LEVELS, LumenModel, LumenNote, MAX_STARS, SET_SIZE, SETS,
};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

/// Where the board sits; the result card covers the same box.
const BOARD_LEFT: i32 = 6;
const BOARD_TOP: i32 = 34;
/// The side panel, right of the board.
const SIDE_LEFT: i32 = 392;
const SIDE_WIDTH: i32 = 82;

fn status_label(status: BeamStatus) -> Phrase {
    match status {
        BeamStatus::Lit => tr!(en: "Lit", zh: "接通"),
        BeamStatus::LampsDark => tr!(en: "A lamp is dark", zh: "还有灯没亮"),
        BeamStatus::Wall => tr!(en: "Hit a wall", zh: "光撞墙了"),
        BeamStatus::Edge => tr!(en: "Off the board", zh: "光出界了"),
        BeamStatus::Loop => tr!(en: "Runs in a loop", zh: "光在绕圈"),
    }
}

fn status_color(status: BeamStatus) -> Color {
    match status {
        BeamStatus::Lit => SUCCESS,
        BeamStatus::LampsDark => HINT,
        _ => TEXT,
    }
}

/// The puzzle's number and name.
struct LevelTitle(usize);

impl fmt::Display for LevelTitle {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(output, "{:02} {}", self.0 + 1, LEVELS[self.0].name)
    }
}

/// The line beside the puzzle name: its tip, or a reply to the last tap.
struct NoteText {
    level: usize,
    note: LumenNote,
}

impl fmt::Display for NoteText {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let phrase = match self.note {
            LumenNote::Tip => LEVELS[self.level].subtitle,
            LumenNote::Hint(_) => tr!(en: "Turn the marked mirror.", zh: "转动高亮的镜子。"),
            LumenNote::Fixed => tr!(en: "That mirror is fixed.", zh: "这面镜子是固定的。"),
        };
        write!(output, "{phrase}")
    }
}

fn note_color(note: LumenNote) -> Color {
    if note == LumenNote::Tip { MUTED } else { HINT }
}

/// Rotatable mirrors on the beam, of all on the board.
struct MirrorCount((u8, u8));

impl fmt::Display for MirrorCount {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (hit, all) = self.0;
        write!(output, "{} {hit}/{all}", tr!(en: "Mirrors", zh: "镜子"))
    }
}

/// Lamps the beam lights; nothing on a board without lamps.
struct LampCount((u8, u8));

impl fmt::Display for LampCount {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (lit, all) = self.0;
        if all == 0 {
            return Ok(());
        }
        write!(output, "{} {lit}/{all}", tr!(en: "Lamps", zh: "灯"))
    }
}

/// How the solve went against par.
struct TurnsLine {
    moves: u16,
    par: u8,
}

impl fmt::Display for TurnsLine {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { moves, par } = *self;
        match locale() {
            Locale::ZhCn => write!(output, "转动 {moves} 次 · 最少 {par} 次"),
            _ if moves == 1 => write!(output, "1 turn · par {par}"),
            _ => write!(output, "{moves} turns · par {par}"),
        }
    }
}

/// Under the result: how to do better, or the stars of the whole lab.
struct ResultTip {
    earned: u8,
    total: u8,
}

impl fmt::Display for ResultTip {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.earned < MAX_STARS {
            write!(
                output,
                "{}",
                tr!(en: "At par, no hint: ★★★", zh: "最少次数、不看提示：★★★")
            )
        } else {
            write!(
                output,
                "{} ★ {} / {}",
                tr!(en: "All puzzles", zh: "全部谜题"),
                self.total,
                LEVEL_COUNT * usize::from(MAX_STARS)
            )
        }
    }
}

fn next_label(has_next: bool) -> Phrase {
    if has_next {
        tr!(en: "Next ▶", zh: "下一关 ▶")
    } else {
        tr!(en: "☰ Puzzles", zh: "☰ 选关")
    }
}

/// Stars as a row of three, filled for those earned.
struct StarText(u8);

impl fmt::Display for StarText {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        for star in 0..MAX_STARS {
            output.write_str(if star < self.0 { "★" } else { "☆" })?;
        }
        Ok(())
    }
}

/// A puzzle tile: its number over its best stars.
struct TileLabel {
    level: usize,
    stars: u8,
}

impl fmt::Display for TileLabel {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(output, "{:02}\n{}", self.level + 1, StarText(self.stars))
    }
}

fn control_color(enabled: bool, active: bool) -> Color {
    if active {
        ACCENT
    } else if enabled {
        CONTROL
    } else {
        CONTROL_DISABLED
    }
}

fn control_text_color(enabled: bool, active: bool) -> Color {
    if active {
        BACKGROUND
    } else if enabled {
        TEXT
    } else {
        TEXT_DISABLED
    }
}

fn tile_text_color(current: bool, stars: u8) -> Color {
    if current {
        BACKGROUND
    } else if stars == MAX_STARS {
        ACCENT
    } else if stars != 0 {
        SUCCESS
    } else {
        TEXT
    }
}

#[compose]
fn compose_header(model: model!(LumenModel)) -> Entity {
    ui! {
        Row (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 30,
            padding: padding(5, 12),
            align: AlignItems::Center,
            column_gap: 8,
            bg_color: HEADER
        ) {
            View (width: 10, height: 10, bg_color: ACCENT, border_radius: 5)
            Text (
                tr!(en : "LUMEN LAB", zh : "光路实验室"),
                id: "lumen_title",
                width: 56,
                height: 20,
                font_size: 10,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${ LevelTitle(model.level_index()) },
                capacity: 32,
                id: "lumen_level_name",
                width: 112,
                height: 20,
                font_size: 10,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${
                    NoteText {
                        level: model.level_index(),
                        note: model.note(),
                    }
                },
                capacity: 64,
                id: "lumen_note",
                grow: 1.0,
                height: 20,
                font_size: 10,
                text_color: ${ note_color(model.note()) },
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
        }
    }
}

#[compose]
fn compose_board(model: model!(LumenModel)) -> Entity {
    ui! {
        View (
            id: "lumen_board_clip",
            position: Position::Absolute,
            left: BOARD_LEFT,
            top: BOARD_TOP,
            width: BOARD_WIDTH,
            height: BOARD_HEIGHT,
            clip_children: true
        ) {
            View (id: "lumen_board", width: BOARD_WIDTH, height: BOARD_HEIGHT) [
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
    }
}

#[compose]
fn compose_readout(model: model!(LumenModel)) -> Entity {
    ui! {
        Column (
            width: SIDE_WIDTH,
            height: 144,
            padding: padding(6, 8),
            bg_color: PANEL,
            border_color: PANEL_BORDER,
            border_width: 1,
            border_radius: 8
        ) {
            Text (
                tr!(en : "TURNS", zh : "转动"),
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${ format_args!("{:02}", model.moves().min(99)) },
                capacity: 2,
                id: "lumen_moves",
                height: 36,
                font_size: 30,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${ format_args!("{} {}", tr!(en : "PAR", zh : "最少"), model.par()) },
                capacity: 12,
                id: "lumen_par",
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            View (id: "lumen_live_stars", width: 66, height: 18) [
                LumenStars {
                    model: model.clone(),
                    row: StarRow::Live,
                },
            ]
            View (height: 1, bg_color: DIVIDER)
            Text (
                text: ${ status_label(model.beam_status()) },
                capacity: 24,
                id: "lumen_status",
                height: 26,
                font_size: 10,
                text_color: ${ status_color(model.beam_status()) },
                paragraph: ParagraphStyle::default()
            )
            Text (
                text: ${ MirrorCount(model.mirror_count()) },
                capacity: 16,
                id: "lumen_mirrors",
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${ LampCount(model.lamp_count()) },
                capacity: 16,
                id: "lumen_lamps",
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
        }
    }
}

#[compose]
fn compose_side(model: model!(LumenModel)) -> Entity {
    let control = style!(
        width: SIDE_WIDTH, height: 28, font_size: 10, border_radius: 7,
        pressed_color: CONTROL_PRESSED, pressed_text_color: BOARD
    );
    ui! {
        Column (
            position: Position::Absolute,
            left: SIDE_LEFT,
            top: BOARD_TOP,
            width: SIDE_WIDTH,
            height: BOARD_HEIGHT,
            row_gap: 4,
            visible: ${ !model.levels_open() }
        ) {
            compose_readout (model)
            Button (
                id: "lumen_undo",
                size: ButtonSize::Compact,
                text: tr!(en : "↺ Undo", zh : "↺ 撤销"),
                style: control.clone(),
                normal_color: ${ control_color(model.can_undo(), false) },
                text_color: ${ control_text_color(model.can_undo(), false) }
            ) on Tap { model.undo(); }
            Button (
                id: "lumen_hint",
                size: ButtonSize::Compact,
                text: tr!(en : "? Hint", zh : "? 提示"),
                style: control.clone(),
                normal_color: ${ control_color(!model.solved(), false) },
                text_color: ${ control_text_color(!model.solved(), false) }
            ) on Tap { model.reveal_hint(); }
            Button (
                id: "lumen_restart",
                size: ButtonSize::Compact,
                text: tr!(en : "↻ Restart", zh : "↻ 重来"),
                style: control.clone(),
                normal_color: CONTROL,
                text_color: TEXT
            ) on Tap { model.reset(); }
            Button (
                id: "lumen_levels",
                size: ButtonSize::Compact,
                text: tr!(en : "☰ Puzzles", zh : "☰ 选关"),
                style: control,
                normal_color: CONTROL,
                text_color: TEXT
            ) on Tap { model.open_levels(); }
        }
    }
}

#[compose]
fn compose_result(model: model!(LumenModel)) -> Entity {
    ui! {
        View (
            id: "lumen_result",
            position: Position::Absolute,
            left: BOARD_LEFT,
            top: BOARD_TOP,
            width: BOARD_WIDTH,
            height: BOARD_HEIGHT,
            visible: ${ model.won() },
            bg_color: SCRIM,
            border_radius: 10
        ) [
            TouchAction::None,
        ] {
            Column (
                position: Position::Absolute,
                left: 69,
                top: 61,
                width: 240,
                height: 150,
                padding: padding(10, 12),
                row_gap: 4,
                align: AlignItems::Center,
                bg_color: PANEL,
                border_color: ACCENT,
                border_width: 1,
                border_radius: 10
            ) {
                Text (
                    text: ${ LevelTitle(model.level_index()) },
                    capacity: 32,
                    id: "lumen_result_title",
                    width: 216,
                    height: 16,
                    font_size: 10,
                    text_color: ACCENT,
                    paragraph: ParagraphStyle::label()
                )
                View (id: "lumen_result_stars", width: 150, height: 40) [
                    LumenStars {
                        model: model.clone(),
                        row: StarRow::Result,
                    },
                ]
                Text (
                    text: ${
                        TurnsLine {
                            moves: model.moves(),
                            par: model.par(),
                        }
                    },
                    capacity: 40,
                    id: "lumen_result_turns",
                    width: 216,
                    height: 14,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${
                        ResultTip {
                            earned: model.earned(),
                            total: model.total_stars(),
                        }
                    },
                    capacity: 48,
                    id: "lumen_result_tip",
                    width: 216,
                    height: 14,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Row (width: 216, height: 30, column_gap: 8) {
                    Button (
                        id: "lumen_replay",
                        size: ButtonSize::Compact,
                        width: 84,
                        height: 30,
                        font_size: 10,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        pressed_text_color: BOARD,
                        text_color: TEXT,
                        border_radius: 7
                    ) [
                        Text::label(tr!(en : "↻ Replay", zh : "↻ 再来")),
                    ] on Tap { model.reset(); }
                    Button (
                        text: ${ next_label(model.has_next()) },
                        capacity: 16,
                        id: "lumen_next",
                        size: ButtonSize::Compact,
                        width: 124,
                        height: 30,
                        font_size: 10,
                        normal_color: ACCENT,
                        pressed_color: CONTROL_PRESSED,
                        text_color: BACKGROUND,
                        border_radius: 7
                    ) on Tap { model.next_level(); }
                }
            }
        } on Tap { model.finish_motion(); }
    }
}

#[compose]
fn level_tile(model: model!(LumenModel), level: usize) -> Entity {
    ui! {
        Button (
            text: ${
                TileLabel {
                    level,
                    stars: model.stars()[level],
                }
            },
            capacity: 16,
            size: ButtonSize::Compact,
            grow: 1.0,
            height: 54,
            font_size: 10,
            normal_color: ${ control_color(true, model.level_index() == level) },
            pressed_color: CONTROL_PRESSED,
            text_color: ${ tile_text_color(model.level_index() == level, model.stars()[level]) },
            border_radius: 7
        ) on Tap { model.select_level(level); }
    }
}

#[compose]
fn level_set(model: model!(LumenModel), set: usize) -> Entity {
    ui! {
        Row (height: 58, column_gap: 6, align: AlignItems::Center) {
            Column (width: 88, height: 54, row_gap: 2, justify: JustifyContent::Center) {
                Text (
                    SETS[set].name,
                    height: 16,
                    font_size: 10,
                    text_color: ACCENT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    SETS[set].rule,
                    height: 14,
                    font_size: 10,
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
                top: 22,
                width: 440,
                height: 276,
                padding: padding(10, 12),
                row_gap: 5,
                bg_color: PANEL,
                border_color: ACCENT,
                border_width: 1,
                border_radius: 10
            ) {
                Row (height: 24, align: AlignItems::Center, column_gap: 8) {
                    Text (
                        tr!(en : "PUZZLES", zh : "选关"),
                        grow: 1.0,
                        height: 20,
                        font_size: 10,
                        text_color: ACCENT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${
                            format_args!(
                                "★ {} / {}", model.total_stars(), LEVEL_COUNT * usize::from(MAX_STARS)
                            )
                        },
                        capacity: 12,
                        id: "lumen_levels_stars",
                        width: 90,
                        height: 18,
                        font_size: 10,
                        text_color: ACCENT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                    )
                    Button (
                        "×",
                        id: "lumen_levels_close",
                        size: ButtonSize::Compact,
                        width: 28,
                        height: 24,
                        font_size: 10,
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
                Text (
                    tr!(
                        en : "Keys: Z undo · H hint · R restart · Enter next · Esc list", zh :
                        "按键：Z 撤销 · H 提示 · R 重来 · Enter 下一关 · Esc 选关"
                    ),
                    height: 14,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
        } on Tap { }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(LumenModel)) {
    ui! {
        View (width: 480, height: 320, bg_color: BACKGROUND) {
            compose_header (model)
            compose_board (model)
            compose_side (model)
            compose_result (model)
            compose_level_modal (model)
        }
    };
}
