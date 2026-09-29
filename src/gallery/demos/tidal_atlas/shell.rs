use super::input::surface_gesture;
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::{modal_render, surface_render};
use super::state::{TideModalSurface, TideSurface};
use super::style::{ACCENT, BACKGROUND, CONTROL, DARK, MUTED, PANEL, PAPER, TEXT};
use crate::gallery::play::font::PlayFontPlugin;
use crate::gallery::play::tidal::{
    IslandResult, Perk, TideMessage, TideModal, TideModel, TidePreview,
};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

fn label() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

fn control_color(active: bool, enabled: bool) -> Color {
    if active {
        PAPER
    } else if enabled {
        PANEL
    } else {
        Color::rgb(29, 57, 63)
    }
}

fn control_text_color(active: bool, enabled: bool) -> Color {
    if active {
        DARK
    } else if enabled {
        TEXT
    } else {
        MUTED
    }
}

struct PreviewText(Option<TidePreview>);

impl fmt::Display for PreviewText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(preview) = self.0 else {
            return Ok(());
        };
        write!(
            out,
            "{} · 本块 {} · 全岛净增 {:+}",
            ["低地", "平地", "高地"][usize::from(preview.terrain)],
            preview.score,
            preview.delta
        )
    }
}

struct HarvestText {
    values: [u16; 4],
    len: u8,
}

struct ModalDetailText(HarvestText);

impl fmt::Display for ModalDetailText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("四季 ")?;
        self.0.fmt(out)
    }
}

impl fmt::Display for HarvestText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, value) in self.values[..usize::from(self.len)].iter().enumerate() {
            if index != 0 {
                out.write_str(" / ")?;
            }
            write!(out, "{value}")?;
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
        if self.index < usize::from(self.chapter) {
            write!(
                out,
                "0{}  {}  {}",
                self.index + 1,
                island_name(self.index),
                self.results[self.index].score
            )
        } else if self.index == usize::from(self.chapter) {
            write!(
                out,
                "0{}  {}  航行中",
                self.index + 1,
                island_name(self.index)
            )
        } else {
            write!(
                out,
                "0{}  {}  未抵达",
                self.index + 1,
                island_name(self.index)
            )
        }
    }
}

fn message_text(message: TideMessage) -> &'static str {
    match message {
        TideMessage::Ready => "选一张地块，再点相邻海域。每 6 次落子结算一次。",
        TideMessage::Placed(_) => "地块已落位，候选与下一季预估已更新。",
        TideMessage::Harvest(_, _) => "季节结算完成；继续扩建群岛。",
        TideMessage::Rerolled => "换一手地块；不会消耗落子回合。",
        TideMessage::Undone => "已撤销；候选、积分与随机状态完整恢复。",
        TideMessage::Settled(true) => "委托达成，追加 45 分。",
        TideMessage::Settled(false) => "本岛结算；未完成委托不影响继续远航。",
        TideMessage::Complete => "四岛航行完成。",
    }
}

fn island_name(index: usize) -> &'static str {
    ["浅湾", "外海", "浮岬", "远境"][index]
}

#[compose(bind(model))]
fn compose_header(model: TideModel) -> Entity {
    ui! {
        Text (
            "潮汐群岛",
            position: Position::Absolute,
            left: 14,
            top: 7,
            width: 105,
            height: 22,
            font_size: 15,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            "TIDAL ATLAS / 004096",
            position: Position::Absolute,
            left: 123,
            top: 10,
            width: 190,
            height: 16,
            font_size: 8,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("第 {} / 4 岛", model.chapter() + 1) },
            text_capacity: 20,
            id: "tide_chapter",
            position: Position::Absolute,
            left: 14,
            top: 39,
            width: 90,
            height: 16,
            font_size: 9,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("落子 {} / 24", model.turn()) },
            text_capacity: 20,
            id: "tide_turn",
            position: Position::Absolute,
            left: 115,
            top: 39,
            width: 90,
            height: 16,
            font_size: 9,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("累计 {}", model.cumulative_score()) },
            text_capacity: 20,
            id: "tide_total",
            position: Position::Absolute,
            left: 171,
            top: 39,
            width: 85,
            height: 16,
            font_size: 9,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    }
}

#[compose(bind(model))]
fn compose_forecast(model: TideModel) -> Entity {
    ui! {
        Text (
            "NEXT HARVEST",
            position: Position::Absolute,
            left: 247,
            top: 67,
            width: 92,
            height: 11,
            font_size: 7,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!(
                "{} · {}",
                if model.forecast().tide == crate::gallery::play::tidal::TideLevel::High {
                    "涨潮"
                } else {
                    "退潮"
                },
                model.forecast().weather.name()
            ) },
            text_capacity: 24,
            id: "tide_forecast",
            position: Position::Absolute,
            left: 247,
            top: 80,
            width: 120,
            height: 18,
            font_size: 12,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("预计 +{}", model.forecast_score()) },
            text_capacity: 20,
            id: "tide_estimate",
            position: Position::Absolute,
            left: 366,
            top: 80,
            width: 90,
            height: 18,
            font_size: 11,
            text_color: ACCENT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${ format_args!(
                "再落 {} 块结算 · 进阶参考线 {}",
                if model.settled() { 0 } else { 6 - model.turn() % 6 },
                model.target()
            ) },
            text_capacity: 56,
            id: "tide_until",
            position: Position::Absolute,
            left: 247,
            top: 98,
            width: 209,
            height: 11,
            font_size: 7,
            text_color: MUTED,
            paragraph: label()
        )
    }
}

#[compose(bind(model))]
fn compose_offers(model: TideModel) -> Entity {
    ui! {
        Text (
            "选择地块",
            position: Position::Absolute,
            left: 237,
            top: 116,
            width: 90,
            height: 15,
            font_size: 9,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("换牌 {}", model.rerolls()) },
            text_capacity: 16,
            id: "tide_offer_count",
            position: Position::Absolute,
            left: 390,
            top: 116,
            width: 76,
            height: 15,
            font_size: 8,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Button (
            text: ${ model.offer_tiles()[0].name() },
            text_capacity: 12,
            id: "tide_offer_0",
            position: Position::Absolute,
            left: 237,
            top: 135,
            width: 73,
            height: 58,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.choice() == 0, !model.settled() && !model.complete()) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(model.choice() == 0, !model.settled() && !model.complete()) },
            border_radius: 4
        ) on Tap { model.select_offer(0); }
    };
    ui! {
        Button (
            text: ${ model.offer_tiles()[1].name() },
            text_capacity: 12,
            id: "tide_offer_1",
            position: Position::Absolute,
            left: 315,
            top: 135,
            width: 73,
            height: 58,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.choice() == 1, !model.settled() && !model.complete()) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(model.choice() == 1, !model.settled() && !model.complete()) },
            border_radius: 4
        ) on Tap { model.select_offer(1); }
    };
    ui! {
        Button (
            text: ${ model.offer_tiles()[2].name() },
            text_capacity: 12,
            id: "tide_offer_2",
            position: Position::Absolute,
            left: 393,
            top: 135,
            width: 73,
            height: 58,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.choice() == 2, !model.settled() && !model.complete()) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(model.choice() == 2, !model.settled() && !model.complete()) },
            border_radius: 4
        ) on Tap { model.select_offer(2); }
    }
}

#[compose(bind(model))]
fn compose_tile_info(model: TideModel) -> Entity {
    ui! {
        Text (
            text: ${ format_args!(
                "{} · {}",
                if model.selected().is_some() { "已落位" } else { "待落位" },
                model.display_tile().name()
            ) },
            text_capacity: 32,
            id: "tide_info_title",
            position: Position::Absolute,
            left: 237,
            top: 199,
            width: 220,
            height: 15,
            font_size: 9,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ model.display_tile().description() },
            text_capacity: 64,
            id: "tide_info_desc",
            position: Position::Absolute,
            left: 237,
            top: 214,
            width: 220,
            height: 14,
            font_size: 8,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ model.display_tile().rule() },
            text_capacity: 64,
            id: "tide_info_rule",
            position: Position::Absolute,
            left: 237,
            top: 228,
            width: 220,
            height: 14,
            font_size: 8,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ PreviewText(model.preview_data()) },
            text_capacity: 96,
            id: "tide_preview",
            position: Position::Absolute,
            left: 237,
            top: 244,
            width: 229,
            height: 12,
            font_size: 7,
            text_color: ACCENT,
            paragraph: label()
        )
    }
}

#[compose(bind(model))]
fn compose_actions(model: TideModel) -> Entity {
    ui! {
        Button (
            "换牌",
            id: "tide_reroll",
            position: Position::Absolute,
            left: 237,
            top: 252,
            width: 75,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            visible: ${ model.pending().is_none() && !model.settled() && !model.complete() },
            normal_color: ${ control_color(false, model.rerolls() > 0 && !model.settled() && !model.complete()) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.rerolls() > 0 && !model.settled() && !model.complete()) },
            border_radius: 5
        ) on Tap { model.reroll(); }
    };
    ui! {
        Button (
            "撤销",
            id: "tide_undo",
            position: Position::Absolute,
            left: 318,
            top: 252,
            width: 66,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            visible: ${ model.pending().is_none() && !model.settled() && !model.complete() },
            normal_color: ${ control_color(false, model.can_undo() && !model.complete()) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.can_undo() && !model.complete()) },
            border_radius: 5
        ) on Tap { model.undo(); }
    };
    ui! {
        Button (
            "航行图",
            id: "tide_voyage",
            visible: ${ model.pending().is_none() && !model.settled() && !model.complete() },
            position: Position::Absolute,
            left: 390,
            top: 252,
            width: 76,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            text_color: TEXT,
            border_radius: 5
        ) on Tap { model.set_modal(TideModal::Voyage); }
    };
    ui! {
        Button (
            "确认落子",
            id: "tide_place",
            visible: ${ model.pending().is_some() },
            position: Position::Absolute,
            left: 237,
            top: 258,
            width: 151,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ACCENT,
            pressed_color: PAPER,
            text_color: DARK,
            border_radius: 5
        ) on Tap {
            if let Some(index) = model.pending() {
                model.place(index, model.choice());
            }
        }
    };
    ui! {
        Button (
            text: ${ if model.pending().is_some() {
                "取消"
            } else if model.complete() {
                "航行完成 · 查看总览"
            } else {
                "本岛结算 · 选择新学说"
            } },
            text_capacity: 40,
            id: "tide_cancel_result",
            visible: ${ model.pending().is_some() || model.settled() || model.complete() },
            position: Position::Absolute,
            left: 397,
            top: 258,
            width: 69,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            text_color: TEXT,
            border_radius: 5
        ) on Tap {
            if model.settled() || model.complete() {
                model.set_modal(TideModal::Result);
            } else {
                model.cancel_preview();
            }
        }
    }
}

#[compose(bind(model))]
fn compose_footer(model: TideModel) -> Entity {
    ui! {
        Text (
            text: ${ model.goal().name },
            text_capacity: 24,
            id: "tide_goal",
            position: Position::Absolute,
            left: 14,
            top: 274,
            width: 140,
            height: 15,
            font_size: 9,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{} / {}", model.goal_progress(), model.goal().count) },
            text_capacity: 8,
            id: "tide_goal_progress",
            position: Position::Absolute,
            left: 164,
            top: 274,
            width: 53,
            height: 15,
            font_size: 8,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${ message_text(model.message()) },
            text_capacity: 96,
            id: "tide_message",
            position: Position::Absolute,
            left: 14,
            top: 300,
            width: 452,
            height: 14,
            font_size: 7,
            text_color: MUTED,
            paragraph: label()
        )
    }
}

#[compose(bind(model))]
fn compose_modal_header(model: TideModel) -> Entity {
    ui! {
        Text (
            text: ${ if model.modal() == TideModal::Voyage {
                "四岛航行图"
            } else if model.complete() {
                "四岛远航 / 完成"
            } else {
                "本岛结算"
            } },
            text_capacity: 32,
            id: "tide_modal_title",
            position: Position::Absolute,
            left: 35,
            top: 53,
            width: 370,
            height: 24,
            font_size: 15,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ if model.modal() == TideModal::Voyage {
                "96 次落子 / 16 次季节结算 / 3 次学说选择"
            } else {
                "评级不锁关。选择学说，继续前往下一座岛。"
            } },
            text_capacity: 128,
            id: "tide_modal_subtitle",
            position: Position::Absolute,
            left: 35,
            top: 77,
            width: 385,
            height: 15,
            font_size: 8,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Button (
            "×",
            position: Position::Absolute,
            left: 421,
            top: 50,
            width: 26,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 14,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            text_color: TEXT,
            border_radius: 5
        ) on Tap { model.set_modal(TideModal::None); }
    }
}

#[compose(bind(model))]
fn compose_modal_result(model: TideModel) -> Entity {
    ui! {
        Text (
            text: ${ format_args!("{}", model.score()) },
            text_capacity: 6,
            id: "tide_modal_score",
            visible: ${ model.modal() == TideModal::Result },
            position: Position::Absolute,
            left: 35,
            top: 111,
            width: 130,
            height: 45,
            font_size: 34,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ ModalDetailText(HarvestText {
                values: model.harvests(),
                len: model.harvest_len(),
            }) },
            text_capacity: 48,
            id: "tide_modal_detail",
            visible: ${ model.modal() == TideModal::Result },
            position: Position::Absolute,
            left: 180,
            top: 120,
            width: 250,
            height: 24,
            font_size: 8,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Button (
            text: ${ PerkText(model.perk_offers()[0]) },
            text_capacity: 64,
            id: "tide_perk_0",
            visible: ${
                model.modal() == TideModal::Result
                    && model.settled()
                    && model.chapter() != 3
                    && !model.complete()
            },
            position: Position::Absolute,
            left: 35,
            top: 181,
            width: 132,
            height: 86,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: BACKGROUND,
            pressed_color: ACCENT,
            text_color: TEXT,
            border_radius: 5
        ) on Tap { model.continue_voyage(Some(model.perk_offers()[0])); }
    };
    ui! {
        Button (
            text: ${ PerkText(model.perk_offers()[1]) },
            text_capacity: 64,
            id: "tide_perk_1",
            visible: ${
                model.modal() == TideModal::Result
                    && model.settled()
                    && model.chapter() != 3
                    && !model.complete()
            },
            position: Position::Absolute,
            left: 174,
            top: 181,
            width: 132,
            height: 86,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: BACKGROUND,
            pressed_color: ACCENT,
            text_color: TEXT,
            border_radius: 5
        ) on Tap { model.continue_voyage(Some(model.perk_offers()[1])); }
    };
    ui! {
        Button (
            text: ${ PerkText(model.perk_offers()[2]) },
            text_capacity: 64,
            id: "tide_perk_2",
            visible: ${
                model.modal() == TideModal::Result
                    && model.settled()
                    && model.chapter() != 3
                    && !model.complete()
            },
            position: Position::Absolute,
            left: 313,
            top: 181,
            width: 132,
            height: 86,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: BACKGROUND,
            pressed_color: ACCENT,
            text_color: TEXT,
            border_radius: 5
        ) on Tap { model.continue_voyage(Some(model.perk_offers()[2])); }
    };
    ui! {
        Button (
            "完成四岛航行",
            id: "tide_finish",
            visible: ${
                model.modal() == TideModal::Result
                    && model.chapter() == 3
                    && model.settled()
                    && !model.complete()
            },
            position: Position::Absolute,
            left: 35,
            top: 225,
            width: 410,
            height: 42,
            size: ButtonSize::Compact,
            font_size: 11,
            normal_color: ACCENT,
            pressed_color: PAPER,
            text_color: DARK,
            border_radius: 6
        ) on Tap { model.continue_voyage(None); }
    }
}

#[compose(bind(model))]
fn compose_modal_voyage(model: TideModel) -> Entity {
    ui! {
        Text (
            text: ${ VoyageRowText {
                index: 0,
                chapter: model.chapter(),
                results: model.results(),
            } },
            text_capacity: 32,
            id: "tide_voyage_0",
            visible: ${ model.modal() == TideModal::Voyage },
            position: Position::Absolute,
            left: 45,
            top: 112,
            width: 390,
            height: 28,
            font_size: 12,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ VoyageRowText {
                index: 1,
                chapter: model.chapter(),
                results: model.results(),
            } },
            text_capacity: 32,
            id: "tide_voyage_1",
            visible: ${ model.modal() == TideModal::Voyage },
            position: Position::Absolute,
            left: 45,
            top: 148,
            width: 390,
            height: 28,
            font_size: 12,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ VoyageRowText {
                index: 2,
                chapter: model.chapter(),
                results: model.results(),
            } },
            text_capacity: 32,
            id: "tide_voyage_2",
            visible: ${ model.modal() == TideModal::Voyage },
            position: Position::Absolute,
            left: 45,
            top: 184,
            width: 390,
            height: 28,
            font_size: 12,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ VoyageRowText {
                index: 3,
                chapter: model.chapter(),
                results: model.results(),
            } },
            text_capacity: 32,
            id: "tide_voyage_3",
            visible: ${ model.modal() == TideModal::Voyage },
            position: Position::Absolute,
            left: 45,
            top: 220,
            width: 390,
            height: 28,
            font_size: 12,
            text_color: TEXT,
            paragraph: label()
        )
    }
}

#[compose(bind(model))]
fn compose_modal(model: TideModel) -> Entity {
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
        ] on Tap { }
        {
            compose_modal_header (model)
            compose_modal_result (model)
            compose_modal_voyage (model)
        }
    }
}

#[compose(bind(model))]
pub(super) fn build_widgets(model: TideModel) {
    ui! {
        View (id: "tide_surface", width: 480, height: 320, clip_children: true) [
            TideSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] on Tap { surface_gesture(&ctx); }
        {
            compose_header (model)
            compose_forecast (model)
            compose_offers (model)
            compose_tile_info (model)
            compose_actions (model)
            compose_footer (model)
            compose_modal (model)
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(PlayFontPlugin);
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    let model = app.add_model(TideModel::default());
    #[cfg(feature = "persistence")]
    install_persistence(app, model.clone());
    app.compose(parent, |cx| build_widgets(cx, model));
}
