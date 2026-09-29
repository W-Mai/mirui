use super::input::{FoldKeyboardPlugin, move_model};
use super::render::surface_view;
use super::state::{FoldHintService, FoldSurface, next, select_map_level};
use super::style::{APRICOT, BG, FLOOR, GRID, LAVENDER, MINT, MUTED, PANEL, TEXT};
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, fold_level,
};
use crate::gallery::play::fold::{
    CHAPTER_MECHANICS, CHAPTER_NAMES, FoldMessage, FoldModel, FoldProgress, HullPose,
};
use crate::gallery::play::font::register_play_font;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

fn level_par(level: u8) -> u8 {
    fold_level(level).map_or(0, |level| level.par())
}

fn next_label(modal: ExpeditionModal, level: u8, unlocked: u8) -> &'static str {
    match modal {
        ExpeditionModal::Result => "下一海域",
        ExpeditionModal::Final => "再次启航",
        ExpeditionModal::None if level < unlocked => "换一关",
        ExpeditionModal::None => "第一关",
    }
}

fn result_title(modal: ExpeditionModal, level: u8) -> &'static str {
    if modal == ExpeditionModal::Final {
        "远征完成"
    } else if level % 6 == 5 {
        "章节完成"
    } else {
        "方舟已归港"
    }
}

fn chapter_color(active: bool) -> Color {
    if active { LAVENDER } else { FLOOR }
}

fn chapter_text_color(active: bool) -> Color {
    if active { BG } else { TEXT }
}

fn level_color(active: bool) -> Color {
    if active { LAVENDER } else { FLOOR }
}

fn level_text_color(active: bool) -> Color {
    if active { BG } else { TEXT }
}

struct StatusText(FoldMessage);

impl fmt::Display for StatusText {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            FoldMessage::Ready => output.write_str("翻滚船体，让它直立停靠终点"),
            FoldMessage::Unsupported => output.write_str("船体失去支撑；这一步没有执行"),
            FoldMessage::Seal => output.write_str("航标封印已收集"),
            FoldMessage::Bridge(true) => output.write_str("潮桥已经升起"),
            FoldMessage::Bridge(false) => output.write_str("潮桥已经收回"),
            FoldMessage::Undone => output.write_str("已撤回上一段翻滚"),
            FoldMessage::Hint(direction, remaining) => {
                write!(
                    output,
                    "提示 {} · 最短还需 {remaining} 步",
                    direction.label()
                )
            }
            FoldMessage::Complete => output.write_str("船体已直立归港"),
        }
    }
}

struct StepText {
    pose: HullPose,
    steps: u16,
    par: u8,
}

impl fmt::Display for StepText {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pose = match self.pose {
            HullPose::Upright => "UPRIGHT",
            HullPose::Horizontal => "HORIZONTAL",
            HullPose::Vertical => "VERTICAL",
        };
        write!(
            output,
            "{pose}   STEP {:02}   PAR {:02}",
            self.steps, self.par
        )
    }
}

struct MapSummary {
    progress: FoldProgress,
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
    progress: FoldProgress,
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
    progress: FoldProgress,
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
fn map_level_button(game: FoldModel, expedition: ExpeditionUiModel, slot: u8) -> Entity {
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
                ) [IgnoreHitTest] {
                    Text (
                        text: ${ LockedLevelLabel(expedition.chapter() * 6 + slot) },
                        text_capacity: 8,
                        grow: 1.0,
                        height: 49,
                        font_size: 10,
                        text_color: Color::rgb(70, 91, 103),
                        paragraph: ParagraphStyle::label()
                    )
                }
            } else {
                Button (
                    text: ${ MapLevelLabel {
                        progress: game.progress(),
                        current: game.level_index(),
                        level: expedition.chapter() * 6 + slot,
                    } },
                    text_capacity: 10,
                    grow: 1.0,
                    height: 57,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: ${ level_color(expedition.chapter() * 6 + slot == game.level_index()) },
                    pressed_color: LAVENDER,
                    text_color: ${ level_text_color(expedition.chapter() * 6 + slot == game.level_index()) },
                    border_radius: 7
                ) on Tap { select_map_level(&game, &expedition, slot); }
            }
        }
    }
}

#[compose(bind(game))]
fn compose_hud(game: FoldModel) -> Entity {
    ui! {
        Text (
            "FOLDING ARK",
            position: Position::Absolute,
            left: 15,
            top: 7,
            width: 210,
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
            id: "fold_chapter",
            position: Position::Absolute,
            left: 228,
            top: 10,
            width: 146,
            height: 16,
            font_size: 8,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("F{:02} / 36", game.level_index() + 1) },
            text_capacity: 8,
            id: "fold_level",
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
            "BRIDGE / SEALS",
            position: Position::Absolute,
            left: 369,
            top: 61,
            width: 84,
            height: 16,
            font_size: 7,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            "Roll across the chart.\nFragile tiles reject\nan upright hull.",
            position: Position::Absolute,
            left: 330,
            top: 98,
            width: 122,
            height: 50,
            font_size: 7,
            text_color: MUTED,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Start)
        )
    }
}

#[compose(bind(game))]
fn compose_controls(game: FoldModel, hints: FoldHintService) -> Entity {
    ui! {
            Button (
                "↑",
                position: Position::Absolute,
                left: 367,
                top: 145,
                width: 40,
                height: 31,
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
                left: 325,
                top: 180,
                width: 40,
                height: 31,
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
                left: 367,
                top: 180,
                width: 40,
                height: 31,
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
                left: 409,
                top: 180,
                width: 40,
                height: 31,
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
                left: 328,
                top: 217,
                width: 58,
                height: 26,
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
                left: 392,
                top: 217,
                width: 61,
                height: 26,
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
fn compose_footer(game: FoldModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            Text (
                text: ${ StatusText(game.message()) },
                text_capacity: 48,
                id: "fold_status",
                position: Position::Absolute,
                left: 16,
                top: 260,
                width: 231,
                height: 18,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
    };
    ui! {
            Text (
                text: ${ StepText {
                    pose: game.pose(),
                    steps: game.steps(),
                    par: level_par(game.level_index()),
                } },
                text_capacity: 40,
                id: "fold_steps",
                position: Position::Absolute,
                left: 16,
                top: 280,
                width: 231,
                height: 14,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
    };
    ui! {
            Button (
                "CHAPTERS",
                position: Position::Absolute,
                left: 252,
                top: 270,
                width: 56,
                height: 34,
                size: ButtonSize::Compact,
                font_size: 6,
                normal_color: FLOOR,
                pressed_color: LAVENDER,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { expedition.open(game.level_index()); }
    };
    ui! {
            Button (
                "RULES",
                position: Position::Absolute,
                left: 314,
                top: 270,
                width: 45,
                height: 34,
                size: ButtonSize::Compact,
                font_size: 6,
                normal_color: FLOOR,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { expedition.open_rules(); }
    };
    ui! {
            Button (
                "RESET",
                position: Position::Absolute,
                left: 365,
                top: 270,
                width: 45,
                height: 34,
                size: ButtonSize::Compact,
                font_size: 6,
                normal_color: FLOOR,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { game.restart(); }
    };
    ui! {
            Button (
                text: ${ next_label(game.modal(), game.level_index(), game.unlocked()) },
                text_capacity: 16,
                id: "fold_next",
                position: Position::Absolute,
                left: 416,
                top: 270,
                width: 50,
                height: 34,
                size: ButtonSize::Compact,
                font_size: 6,
                normal_color: MINT,
                pressed_color: LAVENDER,
                text_color: BG,
                border_radius: 7
            ) on Tap { next(&game, &expedition); }
    }
}

#[compose(bind(game, expedition))]
fn compose_result_overlay(game: FoldModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            View (
                id: "fold_result",
                visible: ${ game.modal() != ExpeditionModal::None },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 244)
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
                    border_color: LAVENDER,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        text: ${ result_title(game.modal(), game.level_index()) },
                        text_capacity: 20,
                        id: "fold_result_title",
                        height: 28,
                        font_size: 15,
                        text_color: LAVENDER,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        text: ${ ResultStats {
                            stars: game.progress().stars(game.level_index()),
                            steps: game.steps(),
                            par: level_par(game.level_index()),
                        } },
                        text_capacity: 48,
                        id: "fold_result_stats",
                        height: 22,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "下一片海域已经开放；当前最优记录会保留。",
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
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { expedition.open(game.level_index()); }
                        Button (
                            "继续远征 →",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: LAVENDER,
                            pressed_color: MINT,
                            text_color: BG,
                            border_radius: 7
                        ) on Tap { next(&game, &expedition); }
                    }
                }
            }
    }
}

#[compose(bind(game, expedition))]
fn compose_map_overlay(game: FoldModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            View (
                id: "fold_map",
                visible: ${ expedition.panel() == ExpeditionPanel::Map },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 244)
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
                        text: ${ MapSummary {
                            progress: game.progress(),
                            chapter: expedition.chapter(),
                        } },
                        text_capacity: 40,
                        id: "fold_map_summary",
                        height: 14,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 26, column_gap: 7) {
                        Button (
                            "01",
                            id: "fold_map_chapter_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ chapter_color(expedition.chapter() == 0) },
                            pressed_color: LAVENDER,
                            text_color: ${ chapter_text_color(expedition.chapter() == 0) },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(0); }
                        Button (
                            "02",
                            id: "fold_map_chapter_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ chapter_color(expedition.chapter() == 1) },
                            pressed_color: LAVENDER,
                            text_color: ${ chapter_text_color(expedition.chapter() == 1) },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(1); }
                        Button (
                            "03",
                            id: "fold_map_chapter_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ chapter_color(expedition.chapter() == 2) },
                            pressed_color: LAVENDER,
                            text_color: ${ chapter_text_color(expedition.chapter() == 2) },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(2); }
                        Button (
                            "04",
                            id: "fold_map_chapter_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ chapter_color(expedition.chapter() == 3) },
                            pressed_color: LAVENDER,
                            text_color: ${ chapter_text_color(expedition.chapter() == 3) },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(3); }
                        Button (
                            "05",
                            id: "fold_map_chapter_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ chapter_color(expedition.chapter() == 4) },
                            pressed_color: LAVENDER,
                            text_color: ${ chapter_text_color(expedition.chapter() == 4) },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(4); }
                        Button (
                            "06",
                            id: "fold_map_chapter_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: ${ chapter_color(expedition.chapter() == 5) },
                            pressed_color: LAVENDER,
                            text_color: ${ chapter_text_color(expedition.chapter() == 5) },
                            border_radius: 6
                        ) on Tap { expedition.select_chapter(5); }
                    }
                    Text (
                        text: ${ CHAPTER_NAMES[usize::from(expedition.chapter())] },
                        text_capacity: 24,
                        id: "fold_map_chapter_name",
                        height: 16,
                        font_size: 12,
                        text_color: LAVENDER,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ CHAPTER_MECHANICS[usize::from(expedition.chapter())] },
                        text_capacity: 48,
                        id: "fold_map_chapter_mechanic",
                        height: 14,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 58, column_gap: 7) {
                        View (id: "fold_map_level_0", grow: 1.0, height: 57) {
                            map_level_button (game, expedition, 0)
                        }
                        View (id: "fold_map_level_1", grow: 1.0, height: 57) {
                            map_level_button (game, expedition, 1)
                        }
                        View (id: "fold_map_level_2", grow: 1.0, height: 57) {
                            map_level_button (game, expedition, 2)
                        }
                        View (id: "fold_map_level_3", grow: 1.0, height: 57) {
                            map_level_button (game, expedition, 3)
                        }
                        View (id: "fold_map_level_4", grow: 1.0, height: 57) {
                            map_level_button (game, expedition, 4)
                        }
                        View (id: "fold_map_level_5", grow: 1.0, height: 57) {
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
                id: "fold_rules",
                visible: ${ expedition.panel() == ExpeditionPanel::Rules },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 244)
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
                    border_color: LAVENDER,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Row (height: 26, align: AlignItems::Center) {
                        Text (
                            "FOLDING ARK · RULES",
                            grow: 1.0,
                            height: 22,
                            font_size: 12,
                            text_color: LAVENDER,
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
                        "01  用方向键翻滚：直立占一格，横卧或纵卧占两格。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "02  所有占用格都要有支撑；裂纹板不能承受直立船体。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "03  经过金色徽记即可收集，收齐后才能直立归港。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "04  直立压 P 会切换桥梁；平躺经过不会触发。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "05  危险移动会被拦住，不计步数，可以直接改走其他方向。",
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
fn compose_briefing_overlay(game: FoldModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            View (
                id: "fold_briefing",
                visible: ${ expedition.panel() == ExpeditionPanel::Briefing },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 244)
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
                    border_color: LAVENDER,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        text: ${ format_args!(
                            "第 {} 章 · {}",
                            game.level_index() / 6 + 1,
                            CHAPTER_NAMES[usize::from(game.level_index() / 6)],
                        ) },
                        text_capacity: 32,
                        id: "fold_briefing_title",
                        height: 24,
                        font_size: 13,
                        text_color: LAVENDER,
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
                        id: "fold_briefing_mechanic",
                        height: 20,
                        font_size: 11,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "薄冰可以躺着经过，但不能直立；开关只响应直立船体。",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "收齐全部金色徽记，再直立到达菱形归港点。",
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
                        normal_color: LAVENDER,
                        pressed_color: MINT,
                        text_color: BG,
                        border_radius: 7
                    ) on Tap { expedition.close(); }
                }
            }
    }
}

#[compose(bind(game, expedition))]
fn compose_summary_overlay(game: FoldModel, expedition: ExpeditionUiModel) -> Entity {
    ui! {
            View (
                id: "fold_summary",
                visible: ${ expedition.panel() == ExpeditionPanel::Summary },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 248)
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
                    border_color: LAVENDER,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "远征档案 · 折叠方舟",
                        height: 25,
                        font_size: 13,
                        text_color: LAVENDER,
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
                        text: ${ SummaryLine { progress: game.progress(), chapter: 0 } },
                        text_capacity: 48,
                        id: "fold_summary_line_0",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ SummaryLine { progress: game.progress(), chapter: 1 } },
                        text_capacity: 48,
                        id: "fold_summary_line_1",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ SummaryLine { progress: game.progress(), chapter: 2 } },
                        text_capacity: 48,
                        id: "fold_summary_line_2",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ SummaryLine { progress: game.progress(), chapter: 3 } },
                        text_capacity: 48,
                        id: "fold_summary_line_3",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ SummaryLine { progress: game.progress(), chapter: 4 } },
                        text_capacity: 48,
                        id: "fold_summary_line_4",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        text: ${ SummaryLine { progress: game.progress(), chapter: 5 } },
                        text_capacity: 48,
                        id: "fold_summary_line_5",
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
                            pressed_color: LAVENDER,
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
                            pressed_color: LAVENDER,
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
                            pressed_color: LAVENDER,
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
                            pressed_color: LAVENDER,
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
                            pressed_color: LAVENDER,
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
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { expedition.open(30); }
                    }
                }
            }
    }
}

#[compose(bind(game, expedition))]
fn build_widgets(game: FoldModel, expedition: ExpeditionUiModel, hints: FoldHintService) {
    ui! {
        View (id: "fold_surface", width: 480, height: 320, clip_children: true) [
            FoldSurface {
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
    let game = app.add_model(FoldModel::default());
    let expedition = app.add_model(ExpeditionUiModel::default());
    let hints = FoldHintService::new();
    #[cfg(feature = "persistence")]
    install_persistence(app, game.clone());
    app.add_plugin(FoldKeyboardPlugin::new(
        game.clone(),
        expedition.clone(),
        hints.clone(),
    ));
    app.compose(parent, |cx| build_widgets(cx, game, expedition, hints));
}

#[cfg(feature = "persistence")]
fn install_persistence<B, F>(
    app: &mut App<B, F>,
    game: <FoldModel as crate::core::model::Model>::Handle,
) where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::model::ModelHandle;
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let save_game = game.clone();
    let restore_game = game;
    let plugin = PersistencePlugin::new(gallery_storage("mirui_folding_ark.bin"))
        .bytes(
            "folding_ark/save",
            move |_world| Some(ModelHandle::read(&save_game, FoldModel::encode_vec)),
            move |_world, bytes| {
                if let Ok(restored) = FoldModel::decode(bytes) {
                    restore_game.restore(restored);
                }
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
