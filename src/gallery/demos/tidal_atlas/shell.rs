use super::input::surface_gesture;
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::{modal_view, surface_view};
use super::state::{TideModalSurface, TideNodes, TideSurface, perk_offer};
use super::style::{ACCENT, BACKGROUND, CONTROL, DARK, MUTED, PANEL, PAPER, TEXT};
use crate::gallery::play::font::register_play_font;
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::{ReplayKind, TidalReplayLog};
use crate::gallery::play::tidal::{TideCommand, TideModal, TideModel};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn label() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

#[compose]
pub(super) fn build_widgets() {
    ui! {
        TideSurface (id: "tide_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
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
            Text (
                "第 1 / 4 岛",
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
            Text (
                "落子 0 / 24",
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
            Text (
                "累计 0",
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
            Text (
                "退潮 · 晴朗",
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
            Text (
                "预计 +0",
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
            Text (
                "再落 6 块结算",
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
            Text (
                "换牌 3",
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
            Button (
                "森林",
                id: "tide_offer_0",
                position: Position::Absolute,
                left: 237,
                top: 135,
                width: 73,
                height: 58,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: PAPER,
                pressed_color: ACCENT,
                text_color: DARK,
                border_radius: 4
            ) on Tap { TideNodes::update(ctx.world, |model| model.select_offer(0)); }
            Button (
                "梯田",
                id: "tide_offer_1",
                position: Position::Absolute,
                left: 315,
                top: 135,
                width: 73,
                height: 58,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: PANEL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { TideNodes::update(ctx.world, |model| model.select_offer(1)); }
            Button (
                "港湾",
                id: "tide_offer_2",
                position: Position::Absolute,
                left: 393,
                top: 135,
                width: 73,
                height: 58,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: PANEL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { TideNodes::update(ctx.world, |model| model.select_offer(2)); }
            Text (
                "待落位",
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
            Text (
                "基础规则",
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
            Text (
                "相邻规则",
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
            Text (
                "",
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
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 5
            ) on Tap { TideNodes::dispatch(ctx.world, TideCommand::Reroll); }
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
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 5
            ) on Tap { TideNodes::dispatch(ctx.world, TideCommand::Undo); }
            Button (
                "航行图",
                id: "tide_voyage",
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
            ) on Tap { TideNodes::update(ctx.world, |model| model.set_modal(TideModal::Voyage)); }
            Button (
                "确认落子",
                id: "tide_place",
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
            ) on Tap { TideNodes::dispatch_pending(ctx.world); }
            Button (
                "取消",
                id: "tide_cancel_result",
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
                let show_result = ctx
                    .world
                    .resource::<TideModel>()
                    .is_some_and(|model| model.settled() || model.complete());
                TideNodes::update(
                    ctx.world,
                    |model| {
                        if show_result {
                            model.set_modal(TideModal::Result)
                        } else {
                            model.cancel_preview()
                        }
                    },
                );
            }
            Text (
                "委托",
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
            Text (
                "0 / 4",
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
            Text (
                "准备远航",
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
            TideModalSurface (
                id: "tide_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                clip_children: true
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Text (
                    "本岛结算",
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
                Text (
                    "",
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
                ) on Tap { TideNodes::update(ctx.world, |model| model.set_modal(TideModal::None)); }
                Text (
                    "0",
                    id: "tide_modal_score",
                    position: Position::Absolute,
                    left: 35,
                    top: 111,
                    width: 130,
                    height: 45,
                    font_size: 34,
                    text_color: ACCENT,
                    paragraph: label()
                )
                Text (
                    "",
                    id: "tide_modal_detail",
                    position: Position::Absolute,
                    left: 180,
                    top: 120,
                    width: 250,
                    height: 24,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label()
                )
                Button (
                    "学说一",
                    id: "tide_perk_0",
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
                ) on Tap {
                    let perk = perk_offer(ctx.world, 0);
                    TideNodes::dispatch(
                        ctx.world,
                        TideCommand::Continue {
                            perk: Some(perk),
                        },
                    );
                }
                Button (
                    "学说二",
                    id: "tide_perk_1",
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
                ) on Tap {
                    let perk = perk_offer(ctx.world, 1);
                    TideNodes::dispatch(
                        ctx.world,
                        TideCommand::Continue {
                            perk: Some(perk),
                        },
                    );
                }
                Button (
                    "学说三",
                    id: "tide_perk_2",
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
                ) on Tap {
                    let perk = perk_offer(ctx.world, 2);
                    TideNodes::dispatch(
                        ctx.world,
                        TideCommand::Continue {
                            perk: Some(perk),
                        },
                    );
                }
                Button (
                    "完成四岛航行",
                    id: "tide_finish",
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
                ) on Tap {
                    TideNodes::dispatch(
                        ctx.world,
                        TideCommand::Continue {
                            perk: None,
                        },
                    );
                }
                Text (
                    "",
                    id: "tide_voyage_0",
                    position: Position::Absolute,
                    left: 45,
                    top: 112,
                    width: 390,
                    height: 28,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: label()
                )
                Text (
                    "",
                    id: "tide_voyage_1",
                    position: Position::Absolute,
                    left: 45,
                    top: 148,
                    width: 390,
                    height: 28,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: label()
                )
                Text (
                    "",
                    id: "tide_voyage_2",
                    position: Position::Absolute,
                    left: 45,
                    top: 184,
                    width: 390,
                    height: 28,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: label()
                )
                Text (
                    "",
                    id: "tide_voyage_3",
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
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    register_play_font(&mut app.world);
    let model = TideModel::default();
    app.world.insert_resource(model);
    #[cfg(feature = "persistence")]
    app.world
        .insert_resource(TidalReplayLog::new(ReplayKind::Tidal, 4096));
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_view()).with_widget(modal_view());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Tidal Atlas node");
    let nodes = TideNodes {
        surface: find("tide_surface"),
        chapter: find("tide_chapter"),
        turn: find("tide_turn"),
        total: find("tide_total"),
        forecast: find("tide_forecast"),
        estimate: find("tide_estimate"),
        until: find("tide_until"),
        offer_count: find("tide_offer_count"),
        offers: [
            find("tide_offer_0"),
            find("tide_offer_1"),
            find("tide_offer_2"),
        ],
        info_title: find("tide_info_title"),
        info_desc: find("tide_info_desc"),
        info_rule: find("tide_info_rule"),
        preview: find("tide_preview"),
        actions: [
            find("tide_reroll"),
            find("tide_undo"),
            find("tide_voyage"),
            find("tide_place"),
            find("tide_cancel_result"),
        ],
        goal: find("tide_goal"),
        goal_progress: find("tide_goal_progress"),
        message: find("tide_message"),
        modal: find("tide_modal"),
        modal_title: find("tide_modal_title"),
        modal_subtitle: find("tide_modal_subtitle"),
        modal_score: find("tide_modal_score"),
        modal_detail: find("tide_modal_detail"),
        perk_buttons: [
            find("tide_perk_0"),
            find("tide_perk_1"),
            find("tide_perk_2"),
        ],
        finish: find("tide_finish"),
        voyage_rows: [
            find("tide_voyage_0"),
            find("tide_voyage_1"),
            find("tide_voyage_2"),
            find("tide_voyage_3"),
        ],
    };
    app.world.insert_resource(nodes);
    TideNodes::sync(&mut app.world);
}
