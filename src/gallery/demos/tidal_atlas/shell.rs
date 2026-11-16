use super::input::{keyboard, surface_tap, tide_tick_system};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::{modal_render, surface_render};
use super::state::{TideModalSurface, TideSurface};
use super::style::{
    ACCENT, BACKGROUND, CELL, CLEAR, CONTROL, DARK, GAIN_TOP, MUTED, PANEL_W, PANEL_X, PAPER,
    RACK_PITCH, RACK_Y, TEXT, cell_origin, center, eighths, end, label,
};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::i18n::{Locale, Phrase, locale};
use crate::gallery::play::font::GameFontPlugin;
use crate::gallery::play::tidal::{
    Goal, IslandResult, Perk, TideMessage, TideModal, TideModel, TidePreview, TideRecord, Tile,
};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, Text};
use core::fmt;

const ISLANDS: [Phrase; 4] = [
    tr!(en: "Shoal Bay", zh: "浅湾"),
    tr!(en: "Outer Sea", zh: "外海"),
    tr!(en: "Drift Cape", zh: "浮岬"),
    tr!(en: "Far Reach", zh: "远境"),
];

const TERRAIN: [Phrase; 3] = [
    tr!(en: "Low ground", zh: "低地"),
    tr!(en: "Flat", zh: "平地"),
    tr!(en: "High ground", zh: "高地"),
];

fn chinese() -> bool {
    locale() == Locale::ZhCn
}

fn control_color(enabled: bool) -> Color {
    if enabled {
        CONTROL
    } else {
        Color::rgb(29, 57, 63)
    }
}

fn control_text_color(enabled: bool) -> Color {
    if enabled { TEXT } else { MUTED }
}

/// "ISLAND 1/4 · TILE 3/24".
struct Progress {
    chapter: u8,
    turn: u8,
}

impl fmt::Display for Progress {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (island, turn) = (self.chapter + 1, self.turn);
        if chinese() {
            write!(out, "第 {island}/4 岛 · 落子 {turn}/24")
        } else {
            write!(out, "ISLAND {island}/4 · TILE {turn}/24")
        }
    }
}

/// The harvest ahead: tide and weather, or that the island is done.
struct ForecastText {
    settled: bool,
    forecast: crate::gallery::play::tidal::Forecast,
}

impl fmt::Display for ForecastText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.settled {
            write!(out, "{}", tr!(en: "Island scored", zh: "本岛已结算"))
        } else {
            write!(
                out,
                "{} · {}",
                self.forecast.tide.name(),
                self.forecast.weather.name()
            )
        }
    }
}

/// Tiles left before the next harvest.
struct UntilText {
    settled: bool,
    turn: u8,
}

impl fmt::Display for UntilText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.settled {
            return Ok(());
        }
        let left = 6 - self.turn % 6;
        if chinese() {
            write!(out, "再落 {left} 块")
        } else {
            write!(out, "IN {left}")
        }
    }
}

/// The coming harvest, blank once the island has no harvest left.
struct EstimateText {
    settled: bool,
    score: u16,
}

impl fmt::Display for EstimateText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.settled {
            Ok(())
        } else {
            write!(out, "+{}", self.score)
        }
    }
}

/// The tile the info lines describe: in hand, or picked on the board.
struct InfoTitle {
    placed: bool,
    tile: Tile,
}

impl fmt::Display for InfoTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.tile.name();
        match (chinese(), self.placed) {
            (true, true) => write!(out, "已落位 · {name}"),
            (true, false) => write!(out, "待落位 · {name}"),
            (false, true) => write!(out, "{name} · placed"),
            (false, false) => write!(out, "{name} · in hand"),
        }
    }
}

/// What the previewed spot gives the tile in hand.
struct PreviewText(Option<TidePreview>);

impl fmt::Display for PreviewText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(preview) = self.0 else {
            return Ok(());
        };
        write!(
            out,
            "{} · {} +{}",
            TERRAIN[usize::from(preview.terrain)],
            tr!(en: "tile", zh: "本块"),
            preview.score
        )
    }
}

/// The place button, with what the island's next harvest gains.
struct PlaceText(Option<TidePreview>);

impl fmt::Display for PlaceText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let place = tr!(en: "Place", zh: "落子");
        match self.0 {
            Some(preview) => write!(out, "{place} {:+}", preview.delta),
            None => write!(out, "{place}"),
        }
    }
}

struct GoalText(Goal);

impl fmt::Display for GoalText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wanted = if self.0.tile == Tile::Sea {
            tr!(en: "tile kinds", zh: "地块种类")
        } else {
            self.0.tile.name()
        };
        write!(out, "{} · {wanted}", self.0.name)
    }
}

struct MessageText(TideMessage);

impl fmt::Display for MessageText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = match self.0 {
            TideMessage::Ready => {
                tr!(en: "Tap sea next to land to place.", zh: "点岛边的海放下地块。")
            }
            TideMessage::Placed(tile) if chinese() => {
                return write!(out, "{}已落位。", tile.name());
            }
            TideMessage::Placed(tile) => return write!(out, "{} placed.", tile.name()),
            TideMessage::Harvest(season, gained) if chinese() => {
                return write!(out, "第 {season} 季结算 +{gained} 分。");
            }
            TideMessage::Harvest(season, gained) => {
                return write!(out, "Harvest {season}: +{gained}");
            }
            TideMessage::Rerolled => tr!(en: "New hand, no tile used.", zh: "已换牌，不占落子。"),
            TideMessage::Undone => tr!(en: "Undone.", zh: "已撤销上一步。"),
            TideMessage::Settled(true) => {
                tr!(en: "Commission met: +45", zh: "委托达成，加 45 分。")
            }
            TideMessage::Settled(false) => {
                tr!(en: "Commission missed.", zh: "本岛结算，委托未完成。")
            }
            TideMessage::Complete => tr!(en: "Voyage complete.", zh: "四岛航行完成。"),
            TideMessage::Shore => tr!(en: "Tiles go next to land.", zh: "地块要放在岛边。"),
            TideMessage::Spot => tr!(en: "Tap sea next to land first.", zh: "先点岛边的一格海。"),
        };
        write!(out, "{words}")
    }
}

/// The season harvests of this island, or the four island scores once the
/// voyage is over.
struct ModalDetailText {
    complete: bool,
    values: [u16; 4],
    len: u8,
}

impl fmt::Display for ModalDetailText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let heading = if self.complete {
            tr!(en: "Islands", zh: "四岛")
        } else {
            tr!(en: "Harvests", zh: "四季")
        };
        write!(out, "{heading} ")?;
        for (index, value) in self.values[..usize::from(self.len)].iter().enumerate() {
            if index != 0 {
                out.write_str(" / ")?;
            }
            write!(out, "{value}")?;
        }
        Ok(())
    }
}

/// The best voyage total, and whether the voyage just finished set it.
struct BestText {
    best: u16,
    fresh: bool,
}

impl fmt::Display for BestText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let best = tr!(en: "BEST", zh: "最高");
        if self.best == 0 {
            return write!(out, "{best} —");
        }
        write!(out, "{best} {}", self.best)?;
        if self.fresh {
            write!(out, " · {}", tr!(en: "★ New best", zh: "★ 新高"))?;
        }
        Ok(())
    }
}

struct PerkText(Perk);

impl fmt::Display for PerkText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}\n{}", self.0.name(), self.0.description())
    }
}

struct VoyageRowText {
    index: usize,
    chapter: u8,
    results: [IslandResult; 4],
}

impl fmt::Display for VoyageRowText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "0{}  {}  ", self.index + 1, ISLANDS[self.index])?;
        match self.index.cmp(&usize::from(self.chapter)) {
            core::cmp::Ordering::Less => write!(out, "{}", self.results[self.index].score),
            core::cmp::Ordering::Equal => write!(out, "{}", tr!(en: "sailing", zh: "航行中")),
            core::cmp::Ordering::Greater => write!(out, "{}", tr!(en: "ahead", zh: "未抵达")),
        }
    }
}

/// The score that pops over a tile at a harvest.
struct Gain(u8);

impl fmt::Display for Gain {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            Ok(())
        } else {
            write!(out, "+{}", self.0)
        }
    }
}

fn result_label(complete: bool, chapter: u8) -> Phrase {
    if complete {
        tr!(en: "Voyage totals ▶", zh: "航行总览 ▶")
    } else if chapter == 3 {
        tr!(en: "Finish voyage ▶", zh: "完成航行 ▶")
    } else {
        tr!(en: "Pick a lore ▶", zh: "选择学说 ▶")
    }
}

fn modal_title(modal: TideModal, complete: bool) -> Phrase {
    if modal == TideModal::Voyage {
        tr!(en: "Voyage chart", zh: "四岛航行图")
    } else if complete {
        tr!(en: "Voyage complete", zh: "远航完成")
    } else {
        tr!(en: "Island scored", zh: "本岛结算")
    }
}

fn modal_subtitle(modal: TideModal, complete: bool, chapter: u8) -> Phrase {
    if modal == TideModal::Voyage {
        tr!(
            en: "24 tiles and four harvests per island; pick a lore between islands.",
            zh: "每岛落 24 块、结算四季，两岛之间选一项学说。"
        )
    } else if complete {
        tr!(en: "Total of all four islands.", zh: "四座岛的总分。")
    } else if chapter == 3 {
        tr!(en: "The last island is scored.", zh: "最后一座岛已结算。")
    } else {
        tr!(
            en: "Pick a lore to take to the next island.",
            zh: "选一项学说，带去下一座岛。"
        )
    }
}

/// A lore can be picked: an island other than the last is scored.
fn picking(modal: TideModal, settled: bool, chapter: u8, complete: bool) -> bool {
    modal == TideModal::Result && settled && chapter != 3 && !complete
}

#[compose]
fn compose_header(model: model!(TideModel)) -> Entity {
    ui! {
        Text (
            tr!(en : "TIDAL ATLAS", zh : "潮汐群岛"),
            position: Position::Absolute,
            left: PANEL_X,
            top: 9,
            width: 96,
            height: 14,
            font_size: 10,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Button (
            "↺",
            id: "tide_undo",
            position: Position::Absolute,
            left: 414,
            top: 4,
            width: 28,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.can_undo()) },
            pressed_color: ACCENT,
            pressed_text_color: DARK,
            text_color: ${ control_text_color(model.can_undo()) },
            border_radius: 4
        ) on Tap { model.undo(); }
    };
    ui! {
        Button (
            "☰",
            id: "tide_voyage",
            position: Position::Absolute,
            left: 446,
            top: 4,
            width: 28,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: DARK,
            text_color: TEXT,
            border_radius: 4
        ) on Tap { model.set_modal(TideModal::Voyage); }
    };
    ui! {
        Text (
            text: ${
                Progress {
                    chapter: model.chapter(),
                    turn: model.turn(),
                }
            },
            capacity: 40,
            id: "tide_chapter",
            position: Position::Absolute,
            left: PANEL_X,
            top: 32,
            width: PANEL_W,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{}", model.score()) },
            capacity: 8,
            id: "tide_score",
            position: Position::Absolute,
            left: PANEL_X,
            top: 49,
            width: 70,
            height: 24,
            font_size: 20,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{} {}", tr!(en : "TARGET", zh : "目标"), model.target()) },
            capacity: 24,
            id: "tide_target",
            position: Position::Absolute,
            left: 384,
            top: 46,
            width: 90,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: end()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{} {}", tr!(en : "TOTAL", zh : "累计"), model.cumulative_score()) },
            capacity: 24,
            id: "tide_total",
            position: Position::Absolute,
            left: 384,
            top: 60,
            width: 90,
            height: 14,
            font_size: 10,
            text_color: TEXT,
            paragraph: end()
        )
    }
}

#[compose]
fn compose_forecast(model: model!(TideModel)) -> Entity {
    ui! {
        Text (
            tr!(en : "NEXT HARVEST", zh : "下季结算"),
            position: Position::Absolute,
            left: PANEL_X + 6,
            top: 81,
            width: 84,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${
                UntilText {
                    settled: model.settled(),
                    turn: model.turn(),
                }
            },
            capacity: 24,
            id: "tide_until",
            position: Position::Absolute,
            left: PANEL_X + 90,
            top: 81,
            width: 66,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: end()
        )
    };
    ui! {
        Text (
            text: ${
                ForecastText {
                    settled: model.settled(),
                    forecast: model.forecast(),
                }
            },
            capacity: 40,
            id: "tide_forecast",
            position: Position::Absolute,
            left: PANEL_X + 6,
            top: 95,
            width: 116,
            height: 14,
            font_size: 10,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${
                EstimateText {
                    settled: model.settled(),
                    score: model.forecast_score(),
                }
            },
            capacity: 8,
            id: "tide_estimate",
            position: Position::Absolute,
            left: PANEL_X + 122,
            top: 95,
            width: 34,
            height: 14,
            font_size: 10,
            text_color: ACCENT,
            paragraph: end()
        )
    }
}

/// One tile in hand; the surface paints it, the button picks it.
#[compose]
fn rack_slot(model: model!(TideModel), slot: u8) -> Entity {
    ui! {
        Button (
            "",
            visible: ${ !model.settled() && !model.complete() },
            position: Position::Absolute,
            left: PANEL_X + i32::from(slot) * RACK_PITCH,
            top: RACK_Y - 4,
            width: CELL,
            height: CELL + 4,
            size: ButtonSize::Compact,
            normal_color: CLEAR,
            pressed_color: Color::rgba(223, 186, 118, 70),
            text_color: CLEAR,
            border_radius: 4
        ) on Tap { model.select_offer(slot); }
    }
}

#[compose]
fn compose_rack(model: model!(TideModel)) -> Entity {
    ui! {
        View (
            id: "tide_offers",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            rack_slot (model, 0)
            rack_slot (model, 1)
            rack_slot (model, 2)
        }
    }
}

#[compose]
fn compose_info(model: model!(TideModel)) -> Entity {
    ui! {
        Text (
            text: ${
                InfoTitle {
                    placed: model.selected().is_some(),
                    tile: model.display_tile(),
                }
            },
            capacity: 48,
            id: "tide_info_title",
            position: Position::Absolute,
            left: PANEL_X,
            top: 172,
            width: PANEL_W,
            height: 14,
            font_size: 10,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ model.display_tile().description() },
            capacity: 48,
            id: "tide_info_desc",
            position: Position::Absolute,
            left: PANEL_X,
            top: 186,
            width: PANEL_W,
            height: 14,
            font_size: 10,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ model.display_tile().rule() },
            capacity: 48,
            id: "tide_info_rule",
            position: Position::Absolute,
            left: PANEL_X,
            top: 200,
            width: PANEL_W,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ PreviewText(model.preview_data()) },
            capacity: 40,
            id: "tide_preview",
            position: Position::Absolute,
            left: PANEL_X,
            top: 214,
            width: PANEL_W,
            height: 14,
            font_size: 10,
            text_color: ACCENT,
            paragraph: label()
        )
    }
}

#[compose]
fn compose_actions(model: model!(TideModel)) -> Entity {
    ui! {
        Button (
            text: ${ format_args!("{} · {}", tr!(en : "Redraw", zh : "换牌"), model.rerolls()) },
            capacity: 24,
            id: "tide_reroll",
            visible: ${ !model.settled() && !model.complete() },
            position: Position::Absolute,
            left: PANEL_X,
            top: 232,
            width: 72,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.rerolls() > 0) },
            pressed_color: ACCENT,
            pressed_text_color: DARK,
            text_color: ${ control_text_color(model.rerolls() > 0) },
            border_radius: 4
        ) on Tap { model.reroll(); }
    };
    ui! {
        Button (
            text: ${ PlaceText(model.preview_data()) },
            capacity: 24,
            id: "tide_place",
            visible: ${ !model.settled() && !model.complete() },
            position: Position::Absolute,
            left: PANEL_X + 76,
            top: 232,
            width: PANEL_W - 76,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ if model.pending().is_some() { ACCENT } else { control_color(false) } },
            pressed_color: PAPER,
            pressed_text_color: DARK,
            text_color: ${ if model.pending().is_some() { DARK } else { MUTED } },
            border_radius: 4
        ) on Tap { model.place_pending(); }
    };
    ui! {
        Button (
            text: ${ result_label(model.complete(), model.chapter()) },
            capacity: 32,
            id: "tide_result",
            visible: ${ model.settled() || model.complete() },
            position: Position::Absolute,
            left: PANEL_X,
            top: 232,
            width: PANEL_W,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ACCENT,
            pressed_color: PAPER,
            pressed_text_color: DARK,
            text_color: DARK,
            border_radius: 4
        ) on Tap { model.set_modal(TideModal::Result); }
    };
    ui! {
        Text (
            text: ${ GoalText(model.goal()) },
            capacity: 48,
            id: "tide_goal",
            position: Position::Absolute,
            left: PANEL_X,
            top: 266,
            width: 124,
            height: 14,
            font_size: 10,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{}/{}", model.goal_progress(), model.goal().count) },
            capacity: 8,
            id: "tide_goal_progress",
            position: Position::Absolute,
            left: PANEL_X + 124,
            top: 266,
            width: 36,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: end()
        )
    };
    ui! {
        Text (
            text: ${ MessageText(model.message()) },
            capacity: 64,
            id: "tide_message",
            position: Position::Absolute,
            left: PANEL_X,
            top: 291,
            width: PANEL_W,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    }
}

/// The score a tile gave at the harvest, rising over it.
#[compose]
fn cell_gain(model: model!(TideModel), index: usize) -> Entity {
    ui! {
        Text (
            text: ${ Gain(model.gains()[index]) },
            capacity: 4,
            visible: ${ model.gain_lift() > 0 && model.gains()[index] > 0 },
            position: Position::Absolute,
            left: cell_origin(index).0,
            top: ${ cell_origin(index).1 + GAIN_TOP - i32::from(model.gain_lift()) },
            width: CELL,
            height: 14,
            font_size: 10,
            text_color: ${ eighths(ACCENT, model.gain_alpha()) },
            paragraph: center()
        )
    }
}

#[compose]
fn gain_row(model: model!(TideModel), row: usize) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            cell_gain (model, row * 6)
            cell_gain (model, row * 6 + 1)
            cell_gain (model, row * 6 + 2)
            cell_gain (model, row * 6 + 3)
            cell_gain (model, row * 6 + 4)
            cell_gain (model, row * 6 + 5)
        }
    }
}

#[compose]
fn compose_gains(model: model!(TideModel)) -> Entity {
    ui! {
        View (
            id: "tide_gains",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            gain_row (model, 0)
            gain_row (model, 1)
            gain_row (model, 2)
            gain_row (model, 3)
            gain_row (model, 4)
            gain_row (model, 5)
        }
    }
}

#[compose]
fn compose_modal_header(model: model!(TideModel)) -> Entity {
    ui! {
        Text (
            text: ${ modal_title(model.modal(), model.complete()) },
            capacity: 32,
            id: "tide_modal_title",
            position: Position::Absolute,
            left: 35,
            top: 52,
            width: 370,
            height: 24,
            font_size: 20,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ modal_subtitle(model.modal(), model.complete(), model.chapter()) },
            capacity: 128,
            id: "tide_modal_subtitle",
            position: Position::Absolute,
            left: 35,
            top: 77,
            width: 385,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Button (
            "×",
            id: "tide_modal_close",
            position: Position::Absolute,
            left: 421,
            top: 50,
            width: 26,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: BACKGROUND,
            text_color: TEXT,
            border_radius: 5
        ) on Tap { model.set_modal(TideModal::None); }
    }
}

#[compose]
fn perk_card(model: model!(TideModel), slot: usize, left: i32) -> Entity {
    ui! {
        Button (
            text: ${ PerkText(model.perk_offers()[slot]) },
            capacity: 80,
            visible: ${ picking(model.modal(), model.settled(), model.chapter(), model.complete()) },
            position: Position::Absolute,
            left: left,
            top: 181,
            width: 132,
            height: 86,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: BACKGROUND,
            pressed_color: ACCENT,
            pressed_text_color: BACKGROUND,
            text_color: TEXT,
            border_radius: 5
        ) on Tap { model.continue_voyage(Some(model.perk_offers()[slot])); }
    }
}

#[compose]
fn compose_modal_result(model: model!(TideModel), record: model!(TideRecord)) -> Entity {
    ui! {
        Text (
            text: ${
                format_args!(
                    "{}", if model.complete() { model.cumulative_score() } else { model.score() }
                )
            },
            capacity: 6,
            id: "tide_modal_score",
            visible: ${ model.modal() == TideModal::Result },
            position: Position::Absolute,
            left: 35,
            top: 108,
            width: 130,
            height: 36,
            font_size: 30,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${
                if model.complete() {
                    ModalDetailText {
                        complete: true,
                        values: model.results().map(|result| result.score),
                        len: 4,
                    }
                } else {
                    ModalDetailText {
                        complete: false,
                        values: model.harvests(),
                        len: model.harvest_len(),
                    }
                }
            },
            capacity: 48,
            id: "tide_modal_detail",
            visible: ${ model.modal() == TideModal::Result },
            position: Position::Absolute,
            left: 180,
            top: 112,
            width: 250,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${
                BestText {
                    best: record.best(),
                    fresh: record.fresh(),
                }
            },
            capacity: 40,
            id: "tide_modal_best",
            visible: ${ model.modal() == TideModal::Result && model.complete() },
            position: Position::Absolute,
            left: 180,
            top: 128,
            width: 250,
            height: 14,
            font_size: 10,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        View (
            id: "tide_perks",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            perk_card (model, 0, 35)
            perk_card (model, 1, 174)
            perk_card (model, 2, 313)
        }
    };
    ui! {
        Button (
            tr!(en : "Finish the voyage", zh : "完成四岛航行"),
            id: "tide_finish",
            visible: ${
                model.modal() == TideModal::Result && model.chapter() == 3 && model.settled()
                    && !model.complete()
            },
            position: Position::Absolute,
            left: 35,
            top: 225,
            width: 410,
            height: 42,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ACCENT,
            pressed_color: PAPER,
            text_color: DARK,
            border_radius: 6
        ) on Tap {
            model.continue_voyage(None);
            record.note(model.cumulative_score());
        }
    };
    ui! {
        Button (
            tr!(en : "↻ New voyage", zh : "↻ 新的航行"),
            id: "tide_new_voyage",
            visible: ${ model.modal() == TideModal::Result && model.complete() },
            position: Position::Absolute,
            left: 35,
            top: 225,
            width: 410,
            height: 42,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ACCENT,
            pressed_color: PAPER,
            text_color: DARK,
            border_radius: 6
        ) on Tap {
            model.new_voyage();
            record.clear_fresh();
        }
    }
}

#[compose]
fn voyage_row(model: model!(TideModel), index: usize) -> Entity {
    ui! {
        Text (
            text: ${
                VoyageRowText {
                    index,
                    chapter: model.chapter(),
                    results: model.results(),
                }
            },
            capacity: 40,
            position: Position::Absolute,
            left: 45,
            top: 106 + 24 * index as i32,
            width: 390,
            height: 20,
            font_size: 10,
            text_color: ${ if index == usize::from(model.chapter()) { ACCENT } else { TEXT } },
            paragraph: label()
        )
    }
}

#[compose]
fn compose_modal_voyage(model: model!(TideModel), record: model!(TideRecord)) -> Entity {
    ui! {
        View (
            id: "tide_chart",
            visible: ${ model.modal() == TideModal::Voyage },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            voyage_row (model, 0)
            voyage_row (model, 1)
            voyage_row (model, 2)
            voyage_row (model, 3)
            Text (
                text: ${
                    BestText {
                        best: record.best(),
                        fresh: false,
                    }
                },
                capacity: 40,
                id: "tide_chart_best",
                position: Position::Absolute,
                left: 45,
                top: 212,
                width: 390,
                height: 14,
                font_size: 10,
                text_color: ACCENT,
                paragraph: label()
            )
            Text (
                tr!(
                    en : "Tap sea next to land to see its score, tap again to place.", zh :
                    "点岛边的海看得分，再点一次落子。"
                ),
                position: Position::Absolute,
                left: 45,
                top: 236,
                width: 390,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: label()
            )
            Text (
                tr!(
                    en : "Keys: 1-3 tile  arrows move  Enter place  Z undo  R redraw", zh :
                    "按键：1-3 选块  方向键移动  回车落子  Z 撤销  R 换牌"
                ),
                position: Position::Absolute,
                left: 45,
                top: 252,
                width: 390,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: label()
            )
        }
    }
}

#[compose]
fn compose_modal(model: model!(TideModel), record: model!(TideRecord)) -> Entity {
    ui! {
        View (
            id: "tide_modal",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            clip_children: true,
            visible: ${ model.modal() != TideModal::None }
        ) [
            TideModalSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] {
            compose_modal_header (model)
            compose_modal_result (model, record)
            compose_modal_voyage (model, record)
        } on Tap { }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(TideModel), record: model!(TideRecord)) {
    ui! {
        View (id: "tide_surface", width: 480, height: 320, clip_children: true) [
            TideSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] {
            View (
                id: "tide_panel",
                visible: ${ model.modal() == TideModal::None },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320
            ) {
                compose_header (model)
                compose_forecast (model)
                compose_rack (model)
                compose_info (model)
                compose_actions (model)
                compose_gains (model)
            }
            compose_modal (model, record)
        } on Tap(|event: event!(Tap), rows: query!(TideSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_tap(surface, rect, event.x, event.y)
        })
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(GameFontPlugin);
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    #[cfg(feature = "persistence")]
    install_persistence(app);
    let model = app.add_model(TideModel::default()).finish();
    let record = app.add_model(TideRecord::default()).finish();
    app.add_system(tide_tick_system::system(model.clone()));
    app.on_event(
        Key::EVENT,
        keyboard::callback(model.clone(), record.clone()),
    );
    app.compose(parent, build_widgets(model, record));
}
