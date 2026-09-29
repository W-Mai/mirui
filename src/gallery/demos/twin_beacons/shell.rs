use super::input::{TwinKeyboardPlugin, move_model};
use super::render::surface_view;
use super::state::{TwinHintService, TwinSurface, next, select_map_level};
use super::style::{APRICOT, BG, FLOOR, GRID, LAVENDER, MINT, MUTED, PANEL, TEXT};
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, twin_level,
};
use crate::gallery::play::font::register_play_font;
#[cfg(feature = "persistence")]
use crate::gallery::play::twin::TwinModelHandle;
use crate::gallery::play::twin::{
    CHAPTER_MECHANICS, CHAPTER_NAMES, TwinMessage, TwinModel, TwinProgress,
};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

fn level_par(level: u8) -> u8 {
    twin_level(level).map_or(0, |level| level.par())
}

fn next_label(modal: ExpeditionModal, level: u8, unlocked: u8) -> &'static str {
    match modal {
        ExpeditionModal::Result => "下一站",
        ExpeditionModal::Final => "再航行",
        ExpeditionModal::None if level < unlocked => "换一站",
        ExpeditionModal::None => "第一站",
    }
}

fn result_title(modal: ExpeditionModal, level: u8) -> &'static str {
    if modal == ExpeditionModal::Final {
        "远征完成"
    } else if level % 6 == 5 {
        "章节完成"
    } else {
        "两座信标已同步"
    }
}

fn chapter_color(active: bool) -> Color {
    if active { MINT } else { FLOOR }
}

fn chapter_text_color(active: bool) -> Color {
    if active { BG } else { TEXT }
}

fn level_color(active: bool) -> Color {
    if active { MINT } else { FLOOR }
}

fn level_text_color(active: bool) -> Color {
    if active { BG } else { TEXT }
}

struct StatusText(TwinMessage);

impl fmt::Display for StatusText {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            TwinMessage::Ready => output.write_str("双站同步，单次输入驱动两边"),
            TwinMessage::Blocked => output.write_str("路径受阻；另一座信标也保持原位"),
            TwinMessage::KeyCollected => output.write_str("访问密钥已同步，闸门开放"),
            TwinMessage::Undone => output.write_str("已撤回上一条指令"),
            TwinMessage::Hint(direction, remaining) => write!(
                output,
                "提示 {} · 最短还需 {remaining} 步",
                direction.label()
            ),
            TwinMessage::Complete => output.write_str("两座信标已同时归位"),
        }
    }
}

struct StepText {
    steps: u16,
    par: u8,
    hints: u16,
}

impl fmt::Display for StepText {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "STEP {:02}   PAR {:02}   HINT {}",
            self.steps, self.par, self.hints
        )
    }
}

struct MapSummary {
    progress: TwinProgress,
    chapter: u8,
}

impl fmt::Display for MapSummary {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "COMPLETE {:02} / 36   CHAPTER {:02} / 06",
            self.progress.completed_count(),
            self.chapter + 1
        )
    }
}

struct MapLevelLabel {
    progress: TwinProgress,
    current: u8,
    level: u8,
}

impl fmt::Display for MapLevelLabel {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.progress.completed(self.level) {
            write!(
                output,
                "{:02}\n{} STAR",
                self.level + 1,
                self.progress.stars(self.level)
            )
        } else if self.level == self.current {
            write!(output, "{:02}\nPLAY", self.level + 1)
        } else {
            write!(output, "{:02}\nOPEN", self.level + 1)
        }
    }
}

struct LockedLevelLabel(u8);

impl fmt::Display for LockedLevelLabel {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(output, "{:02}\nLOCK", self.0 + 1)
    }
}

struct ResultStats {
    stars: u8,
    steps: u16,
    par: u8,
}

impl fmt::Display for ResultStats {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "星级 {} / 3   完成 {:02} 步   最短 {:02} 步",
            self.stars, self.steps, self.par
        )
    }
}

struct SummaryLine {
    progress: TwinProgress,
    chapter: u8,
}

impl fmt::Display for SummaryLine {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut completed = 0;
        let mut stars = 0;
        for slot in 0..6 {
            let level = self.chapter * 6 + slot;
            let level_stars = self.progress.stars(level);
            if level_stars != 0 {
                completed += 1;
                stars += level_stars;
            }
        }
        write!(
            output,
            "0{}  {}   {completed}/6   {stars}/18 ★",
            self.chapter + 1,
            CHAPTER_NAMES[usize::from(self.chapter)]
        )
    }
}

#[compose(bind(game, expedition))]
fn map_level_button(game: TwinModel, expedition: ExpeditionUiModel, slot: u8) -> Entity {
    ui! {
        View (grow: 1.0, height: 57) {
            if ${ expedition.chapter() * 6 + slot > game.unlocked() } {
                View (
                    grow: 1.0,
                    height: 57,
                    padding: Padding::all(4),
                    bg_color: BG,
                    border_radius: 7,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) [
                    IgnoreHitTest,
                ] {
                    Text (
                        text: ${ LockedLevelLabel(expedition.chapter() * 6 + slot) },
                        text_capacity: 8,
                        grow: 1.0,
                        height: 49,
                        font_size: 10,
                        text_color: Color::rgb(67, 91, 101),
                        paragraph: ParagraphStyle::label()
                    )
                }
            } else {
                Button (
                    text: ${
                        MapLevelLabel {
                            progress: game.progress(),
                            current: game.level_index(),
                            level: expedition.chapter() * 6 + slot,
                        }
                    },
                    text_capacity: 10,
                    grow: 1.0,
                    height: 57,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: ${ level_color(expedition.chapter() * 6 + slot == game.level_index()) },
                    pressed_color: MINT,
                    text_color: ${ level_text_color(expedition.chapter() * 6 + slot == game.level_index()) },
                    border_radius: 7
                ) on Tap { select_map_level(&game, &expedition, slot); }
            }
        }
    }
}

#[compose(bind(game))]
fn compose_hud(game: TwinModel) -> Entity {
    ui! {
        Text (
            "TWIN BEACONS",
            position: Position::Absolute,
            left: 15,
            top: 7,
            width: 220,
            height: 22,
            font_size: 15,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ CHAPTER_NAMES[usize::from(game.level_index() / 6)] },
            text_capacity: 24,
            id: "twin_chapter",
            position: Position::Absolute,
            left: 238,
            top: 10,
            width: 136,
            height: 16,
            font_size: 8,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("T{:02} / 36", game.level_index() + 1) },
            text_capacity: 8,
            id: "twin_level",
            position: Position::Absolute,
            left: 383,
            top: 9,
            width: 81,
            height: 17,
            font_size: 9,
            text_color: MINT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            "BEACON A",
            position: Position::Absolute,
            left: 18,
            top: 44,
            width: 128,
            height: 13,
            font_size: 7,
            text_color: MINT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            "BEACON B",
            position: Position::Absolute,
            left: 165,
            top: 44,
            width: 128,
            height: 13,
            font_size: 7,
            text_color: LAVENDER,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            "SYNC ROUTE",
            position: Position::Absolute,
            left: 331,
            top: 62,
            width: 118,
            height: 14,
            font_size: 8,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            "One command\nTwo mirrored routes",
            position: Position::Absolute,
            left: 329,
            top: 94,
            width: 124,
            height: 29,
            font_size: 7,
            text_color: MUTED,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Start)
        )
    }
}

#[compose(bind(game))]
fn compose_controls(game: TwinModel, hints: TwinHintService) -> Entity {
    ui! {
        Button (
            "↑",
            position: Position::Absolute,
            left: 366,
            top: 132,
            width: 40,
            height: 32,
            size: ButtonSize::Compact,
            font_size: 13,
            normal_color: FLOOR,
            pressed_color: MINT,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { move_model(&game, Direction4::Up); }
    };
    ui! {
        Button (
            "←",
            position: Position::Absolute,
            left: 324,
            top: 168,
            width: 40,
            height: 32,
            size: ButtonSize::Compact,
            font_size: 13,
            normal_color: FLOOR,
            pressed_color: MINT,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { move_model(&game, Direction4::Left); }
    };
    ui! {
        Button (
            "↓",
            position: Position::Absolute,
            left: 366,
            top: 168,
            width: 40,
            height: 32,
            size: ButtonSize::Compact,
            font_size: 13,
            normal_color: FLOOR,
            pressed_color: MINT,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { move_model(&game, Direction4::Down); }
    };
    ui! {
        Button (
            "→",
            position: Position::Absolute,
            left: 408,
            top: 168,
            width: 40,
            height: 32,
            size: ButtonSize::Compact,
            font_size: 13,
            normal_color: FLOOR,
            pressed_color: MINT,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { move_model(&game, Direction4::Right); }
    };
    ui! {
        Button (
            "UNDO",
            position: Position::Absolute,
            left: 327,
            top: 214,
            width: 58,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: FLOOR,
            pressed_color: LAVENDER,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { game.undo(); }
    };
    ui! {
        Button (
            "HINT",
            position: Position::Absolute,
            left: 391,
            top: 214,
            width: 62,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: FLOOR,
            pressed_color: APRICOT,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { hints.request(&game); }
    }
}

#[compose(bind(game, expedition))]
fn compose_footer(game: TwinModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
        Text (
            text: ${ StatusText(game.message()) },
            text_capacity: 48,
            id: "twin_status",
            position: Position::Absolute,
            left: 16,
            top: 216,
            width: 282,
            height: 21,
            font_size: 8,
            text_color: TEXT,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${
                StepText {
                    steps: game.steps(),
                    par: level_par(game.level_index()),
                    hints: game.hints(),
                }
            },
            text_capacity: 40,
            id: "twin_steps",
            position: Position::Absolute,
            left: 16,
            top: 245,
            width: 282,
            height: 14,
            font_size: 7,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Button (
            "RESTART",
            position: Position::Absolute,
            left: 16,
            top: 276,
            width: 66,
            height: 29,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: FLOOR,
            pressed_color: APRICOT,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { game.restart(); }
    };
    ui! {
        Button (
            "CHAPTERS",
            position: Position::Absolute,
            left: 88,
            top: 276,
            width: 73,
            height: 29,
            size: ButtonSize::Compact,
            font_size: 7,
            normal_color: FLOOR,
            pressed_color: APRICOT,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { expedition.open(game.level_index()); }
    };
    ui! {
        Button (
            "RULES",
            position: Position::Absolute,
            left: 167,
            top: 276,
            width: 55,
            height: 29,
            size: ButtonSize::Compact,
            font_size: 7,
            normal_color: FLOOR,
            pressed_color: APRICOT,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { expedition.open_rules(); }
    };
    ui! {
        Button (
            text: ${ next_label(game.modal(), game.level_index(), game.unlocked()) },
            text_capacity: 16,
            id: "twin_next",
            position: Position::Absolute,
            left: 228,
            top: 276,
            width: 70,
            height: 29,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: MINT,
            pressed_color: LAVENDER,
            text_color: BG,
            border_radius: 7
        ) on Tap { next(&game, &expedition); }
    };
    ui! {
        Text (
            "WASD · Z UNDO · H HINT",
            position: Position::Absolute,
            left: 318,
            top: 278,
            width: 146,
            height: 21,
            font_size: 7,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    }
}

#[compose(bind(game, expedition))]
fn compose_result_overlay(game: TwinModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
        View (
            id: "twin_result",
            visible: ${ game.modal() != ExpeditionModal::None },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: Color::rgba(5, 12, 20, 244)
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
                border_color: MINT,
                border_width: 1,
                border_radius: 9
            ) {
                Text (
                    text: ${ result_title(game.modal(), game.level_index()) },
                    text_capacity: 24,
                    id: "twin_result_title",
                    height: 28,
                    font_size: 15,
                    text_color: MINT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${
                        ResultStats {
                            stars: game.progress().stars(game.level_index()),
                            steps: game.steps(),
                            par: level_par(game.level_index()),
                        }
                    },
                    text_capacity: 48,
                    id: "twin_result_stats",
                    height: 22,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "下一条航路已经开放；当前最优记录会保留。",
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
                        normal_color: FLOOR,
                        pressed_color: APRICOT,
                        text_color: TEXT,
                        border_radius: 7
                    ) on Tap { game.restart(); }
                    Button (
                        "查看航图",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 33,
                        font_size: 9,
                        normal_color: FLOOR,
                        pressed_color: MINT,
                        text_color: TEXT,
                        border_radius: 7
                    ) on Tap { expedition.open(game.level_index()); }
                    Button (
                        "继续远征 →",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 33,
                        font_size: 9,
                        normal_color: MINT,
                        pressed_color: LAVENDER,
                        text_color: BG,
                        border_radius: 7
                    ) on Tap { next(&game, &expedition); }
                }
            }
        }
    }
}

#[compose(bind(game, expedition))]
fn compose_map_overlay(game: TwinModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
        View (
            id: "twin_map",
            visible: ${ expedition.panel() == ExpeditionPanel::Map },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: Color::rgba(5, 12, 20, 244)
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
                        normal_color: FLOOR,
                        pressed_color: APRICOT,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.close(); }
                }
                Text (
                    text: ${
                        MapSummary {
                            progress: game.progress(),
                            chapter: expedition.chapter(),
                        }
                    },
                    text_capacity: 40,
                    id: "twin_map_summary",
                    height: 14,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Row (height: 26, column_gap: 7) {
                    Button (
                        "01",
                        id: "twin_map_chapter_0",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 25,
                        font_size: 9,
                        normal_color: ${ chapter_color(expedition.chapter() == 0) },
                        pressed_color: MINT,
                        text_color: ${ chapter_text_color(expedition.chapter() == 0) },
                        border_radius: 6
                    ) on Tap { expedition.select_chapter(0); }
                    Button (
                        "02",
                        id: "twin_map_chapter_1",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 25,
                        font_size: 9,
                        normal_color: ${ chapter_color(expedition.chapter() == 1) },
                        pressed_color: MINT,
                        text_color: ${ chapter_text_color(expedition.chapter() == 1) },
                        border_radius: 6
                    ) on Tap { expedition.select_chapter(1); }
                    Button (
                        "03",
                        id: "twin_map_chapter_2",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 25,
                        font_size: 9,
                        normal_color: ${ chapter_color(expedition.chapter() == 2) },
                        pressed_color: MINT,
                        text_color: ${ chapter_text_color(expedition.chapter() == 2) },
                        border_radius: 6
                    ) on Tap { expedition.select_chapter(2); }
                    Button (
                        "04",
                        id: "twin_map_chapter_3",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 25,
                        font_size: 9,
                        normal_color: ${ chapter_color(expedition.chapter() == 3) },
                        pressed_color: MINT,
                        text_color: ${ chapter_text_color(expedition.chapter() == 3) },
                        border_radius: 6
                    ) on Tap { expedition.select_chapter(3); }
                    Button (
                        "05",
                        id: "twin_map_chapter_4",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 25,
                        font_size: 9,
                        normal_color: ${ chapter_color(expedition.chapter() == 4) },
                        pressed_color: MINT,
                        text_color: ${ chapter_text_color(expedition.chapter() == 4) },
                        border_radius: 6
                    ) on Tap { expedition.select_chapter(4); }
                    Button (
                        "06",
                        id: "twin_map_chapter_5",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 25,
                        font_size: 9,
                        normal_color: ${ chapter_color(expedition.chapter() == 5) },
                        pressed_color: MINT,
                        text_color: ${ chapter_text_color(expedition.chapter() == 5) },
                        border_radius: 6
                    ) on Tap { expedition.select_chapter(5); }
                }
                Text (
                    text: ${ CHAPTER_NAMES[usize::from(expedition.chapter())] },
                    text_capacity: 24,
                    id: "twin_map_chapter_name",
                    height: 16,
                    font_size: 12,
                    text_color: MINT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${ CHAPTER_MECHANICS[usize::from(expedition.chapter())] },
                    text_capacity: 48,
                    id: "twin_map_chapter_mechanic",
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Row (height: 58, column_gap: 7) {
                    View (id: "twin_map_level_0", grow: 1.0, height: 57) {
                        map_level_button (game, expedition, 0)
                    }
                    View (id: "twin_map_level_1", grow: 1.0, height: 57) {
                        map_level_button (game, expedition, 1)
                    }
                    View (id: "twin_map_level_2", grow: 1.0, height: 57) {
                        map_level_button (game, expedition, 2)
                    }
                    View (id: "twin_map_level_3", grow: 1.0, height: 57) {
                        map_level_button (game, expedition, 3)
                    }
                    View (id: "twin_map_level_4", grow: 1.0, height: 57) {
                        map_level_button (game, expedition, 4)
                    }
                    View (id: "twin_map_level_5", grow: 1.0, height: 57) {
                        map_level_button (game, expedition, 5)
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
fn compose_rules_overlay(expedition: ExpeditionUiModel) -> Entity {
    ui! {
        View (
            id: "twin_rules",
            visible: ${ expedition.panel() == ExpeditionPanel::Rules },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: Color::rgba(5, 12, 20, 244)
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
                border_color: MINT,
                border_width: 1,
                border_radius: 9
            ) {
                Row (height: 26, align: AlignItems::Center) {
                    Text (
                        "TWIN BEACONS · RULES",
                        grow: 1.0,
                        height: 22,
                        font_size: 12,
                        text_color: MINT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Button (
                        "×",
                        size: ButtonSize::Compact,
                        width: 28,
                        height: 25,
                        font_size: 12,
                        normal_color: FLOOR,
                        pressed_color: APRICOT,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.close(); }
                }
                Text (
                    "01  方向键同时移动两位探索者，撞墙的一位会停住。",
                    height: 20,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "02  必须同时停在各自的双圆环信标；到站后仍会移动。",
                    height: 20,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "03  后续章节会改变 B 站方向，屏幕右侧会完整标注。",
                    height: 20,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "04  钥片由 A 取得；取到后的下一步起可通过闸门。",
                    height: 20,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "05  用墙壁制造错位，再让两位探索者汇合。",
                    height: 20,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
        }
    }
}

#[compose(bind(game, expedition))]
fn compose_briefing_overlay(game: TwinModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
        View (
            id: "twin_briefing",
            visible: ${ expedition.panel() == ExpeditionPanel::Briefing },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: Color::rgba(5, 12, 20, 244)
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
                border_color: MINT,
                border_width: 1,
                border_radius: 9
            ) {
                Text (
                    text: ${
                        format_args!(
                            "第 {} 章 · {}", game.level_index() / 6 + 1, CHAPTER_NAMES[usize::from(game
                            .level_index() / 6)],
                        )
                    },
                    text_capacity: 32,
                    id: "twin_briefing_title",
                    height: 24,
                    font_size: 13,
                    text_color: MINT,
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
                    text: ${ CHAPTER_MECHANICS[usize::from(game.level_index() / 6)] },
                    text_capacity: 48,
                    id: "twin_briefing_mechanic",
                    height: 20,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "留意 B 站的新指令方向；撞墙可制造相对位移。",
                    height: 17,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "出现钥片时，先让 A 取得，再穿过两站闸门。",
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
                    normal_color: MINT,
                    pressed_color: LAVENDER,
                    text_color: BG,
                    border_radius: 7
                ) on Tap { expedition.close(); }
            }
        }
    }
}

#[compose(bind(game, expedition))]
fn compose_summary_overlay(game: TwinModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
        View (
            id: "twin_summary",
            visible: ${ expedition.panel() == ExpeditionPanel::Summary },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: Color::rgba(5, 12, 20, 248)
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
                border_color: MINT,
                border_width: 1,
                border_radius: 9
            ) {
                Text (
                    "远征档案 · 双子信标",
                    height: 25,
                    font_size: 13,
                    text_color: MINT,
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
                    text: ${
                        SummaryLine {
                            progress: game.progress(),
                            chapter: 0,
                        }
                    },
                    text_capacity: 48,
                    id: "twin_summary_line_0",
                    height: 17,
                    font_size: 9,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        SummaryLine {
                            progress: game.progress(),
                            chapter: 1,
                        }
                    },
                    text_capacity: 48,
                    id: "twin_summary_line_1",
                    height: 17,
                    font_size: 9,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        SummaryLine {
                            progress: game.progress(),
                            chapter: 2,
                        }
                    },
                    text_capacity: 48,
                    id: "twin_summary_line_2",
                    height: 17,
                    font_size: 9,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        SummaryLine {
                            progress: game.progress(),
                            chapter: 3,
                        }
                    },
                    text_capacity: 48,
                    id: "twin_summary_line_3",
                    height: 17,
                    font_size: 9,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        SummaryLine {
                            progress: game.progress(),
                            chapter: 4,
                        }
                    },
                    text_capacity: 48,
                    id: "twin_summary_line_4",
                    height: 17,
                    font_size: 9,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        SummaryLine {
                            progress: game.progress(),
                            chapter: 5,
                        }
                    },
                    text_capacity: 48,
                    id: "twin_summary_line_5",
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
                        normal_color: FLOOR,
                        pressed_color: MINT,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.open(0); }
                    Button (
                        "02",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 26,
                        font_size: 7,
                        normal_color: FLOOR,
                        pressed_color: MINT,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.open(6); }
                    Button (
                        "03",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 26,
                        font_size: 7,
                        normal_color: FLOOR,
                        pressed_color: MINT,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.open(12); }
                    Button (
                        "04",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 26,
                        font_size: 7,
                        normal_color: FLOOR,
                        pressed_color: MINT,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.open(18); }
                    Button (
                        "05",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 26,
                        font_size: 7,
                        normal_color: FLOOR,
                        pressed_color: MINT,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.open(24); }
                    Button (
                        "06",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 26,
                        font_size: 7,
                        normal_color: FLOOR,
                        pressed_color: MINT,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.open(30); }
                }
            }
        }
    }
}

#[compose(bind(game, expedition))]
fn build_widgets(game: TwinModel, expedition: ExpeditionUiModel, hints: TwinHintService) {
    ui! {
        View (id: "twin_surface", width: 480, height: 320, clip_children: true) [
            TwinSurface {
                game: game.clone(),
                expedition: expedition.clone(),
            },
        ] {
            compose_hud (game)
            compose_controls (game, hints)
            compose_footer (game, expedition)
            compose_result_overlay (game, expedition)
            compose_map_overlay (game, expedition)
            compose_rules_overlay (expedition)
            compose_briefing_overlay (game, expedition)
            compose_summary_overlay (game, expedition)
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    register_play_font(&mut app.world);
    app.with_widget(surface_view());
    let game = app.add_model(TwinModel::default());
    let expedition = app.add_model(ExpeditionUiModel::default());
    let hints = TwinHintService::new();
    #[cfg(feature = "persistence")]
    install_persistence(app, game.clone());
    app.add_plugin(TwinKeyboardPlugin::new(
        game.clone(),
        expedition.clone(),
        hints.clone(),
    ));
    app.compose(parent, |cx| build_widgets(cx, game, expedition, hints));
}

#[cfg(feature = "persistence")]
fn install_persistence<B, F>(app: &mut App<B, F>, game: TwinModelHandle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::model::ModelHandle;
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let save_game = game.clone();
    let restore_game = game;
    let plugin = PersistencePlugin::new(gallery_storage("mirui_twin_beacons.bin"))
        .bytes(
            "twin_beacons/save",
            move |_world| Some(ModelHandle::read(&save_game, TwinModel::encode_vec)),
            move |_world, bytes| {
                if let Ok(restored) = TwinModel::decode(bytes) {
                    restore_game.restore(restored);
                }
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
