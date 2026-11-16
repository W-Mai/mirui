use super::geometry::target_box;
use super::input::{board_tap, keyboard, swipe_end};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::surface_render;
use super::state::{
    StationTarget, TwinHintService, TwinSurface, TwinTrack, direct_twin, next, select_map_level,
};
use super::style::{APRICOT, BG, FLOOR, GRID, LAVENDER, MINT, MUTED, PANEL, TEXT, WALL};
use crate::core::i18n::Phrase;
use crate::gallery::play::expeditions::{
    BoardMotion, ChapterLine, ChapterTitle, Direction4, ExpeditionModal, ExpeditionPanel,
    ExpeditionUiModel, HintText, LevelNumber, LevelTile, MAP_FOOTER, MAP_TITLE, MapSummary,
    ResultStars, START_CHAPTER, StepLine, SwipeStart, chinese, install_expedition_motion,
    result_next, result_title, swipe_cancel, swipe_start, twin_level,
};
use crate::gallery::play::font::PlayFontPlugin;
use crate::gallery::play::twin::{
    CHAPTER_MECHANICS, CHAPTER_NAMES, CHAPTER_TIPS, TwinMessage, TwinModel, mapped,
};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign, TextVerticalAlign};
use core::fmt;

const STATION: Phrase = tr!(en: "STATION", zh: "站");
const CLEARED: Phrase = tr!(en: "STATION CLEAR", zh: "本站完成");
const NEXT: Phrase = tr!(en: "Next →", zh: "下一站 →");
const OVERLAY: Color = Color::rgba(5, 12, 20, 244);

fn level_par(level: u8) -> u8 {
    twin_level(level).map_or(0, |level| level.par())
}

fn level_mode(level: u8) -> u8 {
    twin_level(level).map_or(0, |level| level.mode())
}

fn chapter_name(level: u8) -> Phrase {
    CHAPTER_NAMES[usize::from(level / 6).min(5)]
}

fn chapter_of(level: u8) -> usize {
    usize::from(level / 6).min(5)
}

/// Where B goes for the keys ↑ → ↓ ← under the level's rule, lined up with
/// the row for A.
struct BRoute(u8);

impl fmt::Display for BRoute {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("B")?;
        for direction in Direction4::ALL {
            write!(output, "  {}", mapped(direction, self.0).arrow())?;
        }
        Ok(())
    }
}

struct StatusText(TwinMessage);

impl fmt::Display for StatusText {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let phrase = match self.0 {
            TwinMessage::Ready => tr!(
                en: "Park A and B on their rings at once",
                zh: "让 A 和 B 同时停在各自的圆环上"
            ),
            TwinMessage::Blocked => tr!(en: "Both beacons are blocked", zh: "两边都被挡住了"),
            TwinMessage::KeyCollected => tr!(
                en: "A has the key: the gates are open",
                zh: "A 取到钥片，闸门打开了"
            ),
            TwinMessage::Undone => tr!(en: "Move undone", zh: "已撤回一步"),
            TwinMessage::Hint(direction, remaining) => {
                return write!(
                    output,
                    "{}",
                    HintText {
                        direction,
                        remaining
                    }
                );
            }
            TwinMessage::Complete => tr!(en: "Both beacons are home", zh: "A 和 B 同时到站"),
        };
        output.write_str(phrase.get())
    }
}

struct ResultStats {
    steps: u16,
    par: u8,
}

impl fmt::Display for ResultStats {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(output, "{} 步 · 最少 {} 步", self.steps, self.par)
        } else {
            write!(output, "{} moves · par {}", self.steps, self.par)
        }
    }
}

/// The pad key a hint points to lights up.
fn pad_color(message: TwinMessage, direction: Direction4) -> Color {
    match message {
        TwinMessage::Hint(hinted, _) if hinted == direction => APRICOT,
        _ => FLOOR,
    }
}

fn tab_color(active: bool) -> Color {
    if active { MINT } else { FLOOR }
}

fn tab_text_color(active: bool) -> Color {
    if active { BG } else { TEXT }
}

#[compose]
fn map_level_button(
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
    slot: u8,
) -> Entity {
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
                        text_color: Color::rgb(67, 91, 101),
                        paragraph: ParagraphStyle::label()
                    )
                }
            } else {
                Button (
                    text: ${
                        LevelTile {
                            level: expedition.chapter() * 6 + slot,
                            stars: game.progress().stars(expedition.chapter() * 6 + slot),
                            current: expedition.chapter() * 6 + slot == game.level_index(),
                            locked: false,
                        }
                    },
                    capacity: 16,
                    grow: 1.0,
                    height: 57,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: ${ tab_color(expedition.chapter() * 6 + slot == game.level_index()) },
                    pressed_color: MINT,
                    text_color: ${ tab_text_color(expedition.chapter() * 6 + slot == game.level_index()) },
                    border_radius: 7
                ) on Tap { select_map_level(&game, &expedition, slot); }
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
            pressed_color: MINT,
            text_color: ${ tab_text_color(expedition.chapter() == chapter) },
            border_radius: 6
        ) on Tap { expedition.select_chapter(chapter); }
    }
}

#[compose]
fn compose_header(game: model!(TwinModel), expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        Text (
            text: tr!(en : "TWIN BEACONS", zh : "双子信标"),
            id: "twin_title",
            position: Position::Absolute,
            left: 12,
            top: 7,
            width: 84,
            height: 14,
            font_size: 10,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("T{:02}", game.level_index() + 1) },
            capacity: 4,
            id: "twin_level",
            position: Position::Absolute,
            left: 98,
            top: 7,
            width: 28,
            height: 14,
            font_size: 10,
            text_color: MINT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ chapter_name(game.level_index()) },
            capacity: 24,
            id: "twin_chapter",
            position: Position::Absolute,
            left: 128,
            top: 7,
            width: 100,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${
                StepLine {
                    steps: game.steps(),
                    par: level_par(game.level_index()),
                    hints: game.hints(),
                }
            },
            capacity: 48,
            id: "twin_steps",
            position: Position::Absolute,
            left: 230,
            top: 7,
            width: 176,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Button (
            "ⓘ",
            id: "twin_rules_button",
            position: Position::Absolute,
            left: 414,
            top: 3,
            width: 28,
            height: 22,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: FLOOR,
            pressed_color: APRICOT,
            pressed_text_color: WALL,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { expedition.open_rules(); }
    };
    ui! {
        Button (
            "☰",
            id: "twin_map_button",
            position: Position::Absolute,
            left: 446,
            top: 3,
            width: 28,
            height: 22,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: FLOOR,
            pressed_color: APRICOT,
            pressed_text_color: WALL,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { expedition.open(game.level_index()); }
    }
}

#[compose]
fn compose_routes(game: model!(TwinModel)) -> Entity {
    ui! {
        Text (
            "A  ↑  →  ↓  ←",
            id: "twin_route_a",
            position: Position::Absolute,
            left: 14,
            top: 31,
            width: 110,
            height: 13,
            font_size: 10,
            text_color: MINT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ BRoute(level_mode(game.level_index())) },
            capacity: 24,
            id: "twin_route_b",
            position: Position::Absolute,
            left: 253,
            top: 31,
            width: 110,
            height: 13,
            font_size: 10,
            text_color: LAVENDER,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    }
}

#[compose]
fn pad_key(game: model!(TwinModel), direction: Direction4, left: i32) -> Entity {
    ui! {
        Button (
            text: direction.arrow(),
            position: Position::Absolute,
            left: left,
            top: 279,
            width: 32,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 20,
            normal_color: ${ pad_color(game.message(), direction) },
            pressed_color: MINT,
            pressed_text_color: WALL,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { game.move_direction(direction); }
    }
}

#[compose]
fn compose_controls(game: model!(TwinModel), hints: TwinHintService) -> Entity {
    ui! {
        Button (
            "↺",
            id: "twin_undo",
            position: Position::Absolute,
            left: 10,
            top: 279,
            width: 34,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 20,
            normal_color: FLOOR,
            pressed_color: LAVENDER,
            pressed_text_color: WALL,
            text_color: ${ if game.steps() > 0 { TEXT } else { MUTED } },
            border_radius: 7
        ) on Tap { game.undo(); }
    };
    ui! {
        Button (
            "↻",
            id: "twin_restart",
            position: Position::Absolute,
            left: 48,
            top: 279,
            width: 34,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 20,
            normal_color: FLOOR,
            pressed_color: APRICOT,
            pressed_text_color: WALL,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { game.restart(); }
    };
    ui! {
        Button (
            "?",
            id: "twin_hint",
            position: Position::Absolute,
            left: 86,
            top: 279,
            width: 34,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 20,
            normal_color: FLOOR,
            pressed_color: APRICOT,
            pressed_text_color: WALL,
            text_color: APRICOT,
            border_radius: 7
        ) on Tap { hints.request(&game); }
    };
    ui! {
        Text (
            text: ${ StatusText(game.message()) },
            capacity: 64,
            id: "twin_status",
            position: Position::Absolute,
            left: 128,
            top: 276,
            width: 196,
            height: 36,
            font_size: 10,
            text_color: TEXT,
            paragraph: ParagraphStyle::default()
                .with_align(TextAlign::Start)
                .with_vertical_align(TextVerticalAlign::Center)
        )
    };
    ui!(pad_key(game, Direction4::Left, 330));
    ui!(pad_key(game, Direction4::Up, 366));
    ui!(pad_key(game, Direction4::Down, 402));
    ui!(pad_key(game, Direction4::Right, 438))
}

#[compose]
fn compose_banner(game: model!(TwinModel), expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        Column (
            id: "twin_banner",
            visible: ${
                expedition.banner() && expedition.panel() == ExpeditionPanel::None
                    && game.modal() == ExpeditionModal::None
            },
            position: Position::Absolute,
            left: 140,
            top: 118,
            width: 200,
            height: 60,
            padding: Padding::all(8),
            row_gap: 2,
            align: AlignItems::Center,
            bg_color: PANEL,
            border_color: MINT,
            border_width: 1,
            border_radius: 9
        ) [
            IgnoreHitTest,
        ] {
            Text (
                text: ${
                    LevelNumber {
                        level: game.level_index(),
                        unit: STATION,
                    }
                },
                capacity: 16,
                id: "twin_banner_title",
                height: 26,
                font_size: 20,
                text_color: MINT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                text: ${ chapter_name(game.level_index()) },
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
fn compose_result_overlay(
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
) -> Entity {
    ui! {
        View (
            id: "twin_result",
            visible: ${ game.modal() != ExpeditionModal::None },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: Color::rgba(5, 12, 20, 210)
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
                border_color: MINT,
                border_width: 1,
                border_radius: 10
            ) {
                Text (
                    text: ${ result_title(game.modal(), game.level_index(), CLEARED) },
                    capacity: 24,
                    id: "twin_result_title",
                    width: 280,
                    height: 26,
                    font_size: 20,
                    text_color: MINT,
                    paragraph: ParagraphStyle::label()
                )
                View (id: "twin_result_stars", width: 108, height: 34) [
                    ResultStars::new(APRICOT, FLOOR),
                ]
                Text (
                    text: ${
                        ResultStats {
                            steps: game.steps(),
                            par: level_par(game.level_index()),
                        }
                    },
                    capacity: 32,
                    id: "twin_result_stats",
                    width: 280,
                    height: 14,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: tr!(
                        en : "Par with no hints earns ★★★", zh :
                        "不用提示、按最少步数到站得 ★★★"
                    ),
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
                        normal_color: FLOOR,
                        pressed_color: APRICOT,
                        pressed_text_color: WALL,
                        text_color: TEXT,
                        border_radius: 7
                    ) on Tap { game.restart(); }
                    Button (
                        text: tr!(en : "☰ Map", zh : "☰ 航图"),
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 32,
                        font_size: 10,
                        normal_color: FLOOR,
                        pressed_color: MINT,
                        pressed_text_color: WALL,
                        text_color: TEXT,
                        border_radius: 7
                    ) on Tap { expedition.open(game.level_index()); }
                    Button (
                        text: ${ result_next(game.modal(), NEXT) },
                        capacity: 20,
                        id: "twin_next",
                        size: ButtonSize::Compact,
                        grow: 1.3,
                        height: 32,
                        font_size: 10,
                        normal_color: MINT,
                        pressed_color: LAVENDER,
                        text_color: BG,
                        border_radius: 7
                    ) on Tap { next(&game, &expedition); }
                }
            }
        } on Tap { }
    }
}

#[compose]
fn compose_map_overlay(game: model!(TwinModel), expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        View (
            id: "twin_map",
            visible: ${ expedition.panel() == ExpeditionPanel::Map },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: OVERLAY
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
                        normal_color: FLOOR,
                        pressed_color: APRICOT,
                        pressed_text_color: WALL,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.close(); }
                }
                Text (
                    text: ${
                        MapSummary {
                            cleared: game.progress().completed_count(),
                            chapter: expedition.chapter(),
                        }
                    },
                    capacity: 48,
                    id: "twin_map_summary",
                    height: 14,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Row (height: 26, column_gap: 7) {
                    View (id: "twin_map_chapter_0", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 0)
                    }
                    View (id: "twin_map_chapter_1", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 1)
                    }
                    View (id: "twin_map_chapter_2", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 2)
                    }
                    View (id: "twin_map_chapter_3", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 3)
                    }
                    View (id: "twin_map_chapter_4", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 4)
                    }
                    View (id: "twin_map_chapter_5", grow: 1.0, height: 25) {
                        chapter_tab (expedition, 5)
                    }
                }
                Text (
                    text: ${ CHAPTER_NAMES[usize::from(expedition.chapter())] },
                    capacity: 24,
                    id: "twin_map_chapter_name",
                    height: 16,
                    font_size: 10,
                    text_color: MINT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${ CHAPTER_MECHANICS[usize::from(expedition.chapter())] },
                    capacity: 64,
                    id: "twin_map_chapter_mechanic",
                    height: 14,
                    font_size: 10,
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
fn compose_rules_overlay(expedition: model!(ExpeditionUiModel)) -> Entity {
    ui! {
        View (
            id: "twin_rules",
            visible: ${ expedition.panel() == ExpeditionPanel::Rules },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: OVERLAY
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
                border_color: MINT,
                border_width: 1,
                border_radius: 9
            ) {
                Row (height: 26, align: AlignItems::Center) {
                    Text (
                        text: tr!(en : "TWIN BEACONS · RULES", zh : "双子信标 · 规则"),
                        grow: 1.0,
                        height: 22,
                        font_size: 10,
                        text_color: MINT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Button (
                        "×",
                        size: ButtonSize::Compact,
                        width: 28,
                        height: 25,
                        font_size: 10,
                        normal_color: FLOOR,
                        pressed_color: APRICOT,
                        pressed_text_color: WALL,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { expedition.close(); }
                }
                rule_line (
                    tr!(
                        en : "01  Arrows or a swipe move A and B together; a blocked one stays put.", zh
                        : "01  方向键或滑动同时移动 A 和 B；撞墙的一位会停住。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en :
                        "02  Both must rest on their rings at once; one that arrives early goes on.", zh
                        :
                        "02  A 和 B 要同时停在各自的圆环上；先到的一方还会继续走。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en : "03  Tap a board to move its beacon one cell toward the tap.", zh :
                        "03  点格子也能走：信标朝点的方向移动一格。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en :
                        "04  Later chapters change B's directions; the row above board B shows them.", zh
                        :
                        "04  后面的章节里 B 的方向会变，B 板上方的箭头会标出来。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en : "05  Only A takes the key; from the next move on, both gates open.", zh :
                        "05  钥片只有 A 能取；取到后，下一步起两边闸门都能通过。"
                    ),
                    TEXT
                )
                rule_line (
                    tr!(
                        en : "Keys  arrows/WASD move · Z undo · R restart · H hint · Enter next", zh
                        :
                        "键盘  方向键/WASD 移动 · Z 撤销 · R 重来 · H 提示 · Enter 继续"
                    ),
                    MUTED
                )
            }
        } on Tap { }
    }
}

#[compose]
fn compose_briefing_overlay(
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
) -> Entity {
    ui! {
        View (
            id: "twin_briefing",
            visible: ${ expedition.panel() == ExpeditionPanel::Briefing },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            bg_color: OVERLAY
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
                border_color: MINT,
                border_width: 1,
                border_radius: 9
            ) {
                Text (
                    text: ${
                        ChapterTitle {
                            chapter: game.level_index() / 6,
                            name: chapter_name(game.level_index()),
                        }
                    },
                    capacity: 40,
                    id: "twin_briefing_title",
                    width: 384,
                    height: 24,
                    font_size: 10,
                    text_color: MINT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${ CHAPTER_MECHANICS[chapter_of(game.level_index())] },
                    capacity: 64,
                    id: "twin_briefing_mechanic",
                    width: 384,
                    height: 20,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${ CHAPTER_TIPS[chapter_of(game.level_index())][0] },
                    capacity: 64,
                    id: "twin_briefing_tip_0",
                    width: 384,
                    height: 17,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${ CHAPTER_TIPS[chapter_of(game.level_index())][1] },
                    capacity: 64,
                    id: "twin_briefing_tip_1",
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
                    normal_color: MINT,
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
            normal_color: FLOOR,
            pressed_color: MINT,
            pressed_text_color: WALL,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { expedition.open(chapter * 6); }
    }
}

#[compose]
fn compose_summary_overlay(
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
) -> Entity {
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
                border_color: MINT,
                border_width: 1,
                border_radius: 9
            ) {
                Text (
                    text: tr!(en : "EXPEDITION LOG · TWIN BEACONS", zh : "远征档案 · 双子信标"),
                    height: 25,
                    font_size: 10,
                    text_color: MINT,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: tr!(
                        en : "All 36 stations done. Tap a chapter to play it again.", zh :
                        "36 站全部完成。点章节编号可以重新挑战。"
                    ),
                    height: 16,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: game.progress(),
                            chapter: 0,
                            name: CHAPTER_NAMES[0],
                        }
                    },
                    capacity: 48,
                    id: "twin_summary_line_0",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: game.progress(),
                            chapter: 1,
                            name: CHAPTER_NAMES[1],
                        }
                    },
                    capacity: 48,
                    id: "twin_summary_line_1",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: game.progress(),
                            chapter: 2,
                            name: CHAPTER_NAMES[2],
                        }
                    },
                    capacity: 48,
                    id: "twin_summary_line_2",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: game.progress(),
                            chapter: 3,
                            name: CHAPTER_NAMES[3],
                        }
                    },
                    capacity: 48,
                    id: "twin_summary_line_3",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: game.progress(),
                            chapter: 4,
                            name: CHAPTER_NAMES[4],
                        }
                    },
                    capacity: 48,
                    id: "twin_summary_line_4",
                    height: 17,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${
                        ChapterLine {
                            progress: game.progress(),
                            chapter: 5,
                            name: CHAPTER_NAMES[5],
                        }
                    },
                    capacity: 48,
                    id: "twin_summary_line_5",
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
fn board_target(
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
    station: u8,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: target_box(usize::from(station)).0,
            top: target_box(usize::from(station)).1,
            width: target_box(usize::from(station)).2,
            height: target_box(usize::from(station)).3
        ) [
            StationTarget {
                game: game.clone(),
                expedition: expedition.clone(),
                station,
            },
            TouchAction::None,
        ] on Tap(board_tap::callback())
    }
}

#[compose]
fn build_widgets(
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
    hints: TwinHintService,
) {
    ui! {
        View (id: "twin_surface", width: 480, height: 320, clip_children: true) [
            TwinSurface {
                game: game.clone(),
                expedition: expedition.clone(),
            },
            TwinTrack::default(),
            BoardMotion::default(),
            SwipeStart::default(),
            TouchAction::None,
        ] {
            board_target (game, expedition, 0)
            board_target (game, expedition, 1)
            compose_header (game, expedition)
            compose_routes (game)
            compose_controls (game, hints)
            compose_banner (game, expedition)
            compose_result_overlay (game, expedition)
            compose_map_overlay (game, expedition)
            compose_rules_overlay (expedition)
            compose_briefing_overlay (game, expedition)
            compose_summary_overlay (game, expedition)
        } on DragStart(swipe_start::callback()) on DragEnd(swipe_end::callback()) on DragCancel(swipe_cancel::callback())
    };
}

fn setup_with_models<B, F, Capture>(app: &mut App<B, F>, parent: Entity, capture: Capture)
where
    B: Surface,
    F: RendererFactory<B>,
    Capture: FnOnce(
        &<TwinModel as crate::core::model::Model>::Handle,
        &<ExpeditionUiModel as crate::core::model::Model>::Handle,
    ),
{
    app.add_plugin(PlayFontPlugin);
    app.with_widget(surface_render::view());
    #[cfg(feature = "persistence")]
    install_persistence(app);
    let game = app.add_model(TwinModel::default()).finish();
    let expedition = app.add_model(ExpeditionUiModel::default()).finish();
    capture(&game, &expedition);
    install_expedition_motion(app, &expedition);
    app.add_system(direct_twin::system(game.clone(), expedition.clone()));
    let hints = TwinHintService::new();
    app.add_resource(hints.clone());
    app.on_event(
        Key::EVENT,
        keyboard::callback(game.clone(), expedition.clone()),
    );
    app.compose(parent, build_widgets(game, expedition, hints));
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    setup_with_models(app, parent, |_, _| {});
}

#[cfg(test)]
pub(super) fn setup_test_app<B, F>(
    app: &mut App<B, F>,
    parent: Entity,
) -> (
    <TwinModel as crate::core::model::Model>::Handle,
    <ExpeditionUiModel as crate::core::model::Model>::Handle,
)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let mut handles = None;
    setup_with_models(app, parent, |game, expedition| {
        handles = Some((game.clone(), expedition.clone()));
    });
    handles.expect("Twin models are captured during setup")
}
