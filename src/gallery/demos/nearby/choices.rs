use super::data::{Page, Snapshot};
use super::input::SelectionTarget;
use super::model::DisplayState;
use super::pixel::Scenery;
use super::pixel::{ChoiceSprite, Stage::Walker, art};
use super::style::*;
use super::widgets::{label, paragraph, shadow};
use crate::prelude::*;
use crate::ui::widgets::{Text, TextAlign};
use TextAlign::Center;
#[compose]
pub(super) fn choice(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let page = view.page;
    let network_only = view.network_only;
    let cards = if page == Page::Mode {
        [
            (0, "仅组网", "连接附近的掌机"),
            (1, "选择游戏", "选择要一起玩的游戏"),
        ]
    } else if network_only {
        [
            (0, "创建房间", "邀请好友连接你的掌机"),
            (1, "加入房间", "寻找附近好友的房间"),
        ]
    } else {
        [
            (0, "创建房间", "邀请好友加入你的房间"),
            (1, "加入房间", "寻找附近好友的房间"),
        ]
    };
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Row (grow: 1.0, min_height: 0, column_gap: metrics.choice_gap()) {
                walk cards.into_iter() with card {
                    Column (
                        grow: 1.0,
                        min_width: 0,
                        min_height: 0,
                        transform: ${ state.card_motion().card_transform(card.0) }
                    ) {
                        Column (
                            grow: 1.0,
                            min_width: 0,
                            min_height: 0,
                            padding: metrics.card_padding(),
                            row_gap: metrics.gap,
                            bg_color: ${ state.card_motion().card_color(card.0) }
                        ) [
                            SelectionTarget {
                                scope: (page == Page::Mode).then_some(Page::Mode),
                                index: card.0,
                            },
                        ] {
                            View (
                                grow: 1.0,
                                min_height: metrics.choice_art_height(),
                                clip_children: true
                            ) [
                                Scenery::Landscape,
                                ChoiceSprite {
                                    state: state.clone(),
                                    index: card.0,
                                    mode: page == Page::Mode,
                                },
                            ] {
                                Text (
                                    if card.0 == 0 { "+" } else { "2" },
                                    position: Position::Absolute,
                                    left: 6,
                                    top: 6,
                                    width: 24,
                                    height: 20,
                                    font: font(metrics.small),
                                    font_size: metrics.small,
                                    bg_color: BACKGROUND,
                                    text_color: SECONDARY,
                                    paragraph: paragraph(Center, 1)
                                ) [
                                    crate::ui::dirty::PaintInflate::uniform(Fixed::ONE),
                                ]
                            }
                            label (card.1, metrics.title, FOREGROUND, Center)
                            if !metrics.tiny {
                                Text (
                                    card.2,
                                    width: Dimension::percent(100),
                                    min_width: 0,
                                    height: Dimension::Content,
                                    font: font(metrics.small),
                                    font_size: metrics.small,
                                    text_color: SECONDARY,
                                    paragraph: paragraph(Center, 2)
                                ) [
                                    crate::ui::dirty::PaintInflate::uniform(Fixed::ONE),
                                ]
                            }
                        } on HoverChanged(super::input::hover::callback(state, card.0)) on Tap { state.choose_connection(card.0); }
                        shadow (metrics)
                    }
                }
            }
            if ${ page == Page::Mode && state.current_game() } {
                Row (
                    height: 36,
                    shrink: 0.0,
                    bg_color: PANEL,
                    padding: Padding::all(6),
                    transform: ${ state.card_motion().card_transform(2) }
                ) [
                    SelectionTarget {
                        scope: Some(Page::Mode),
                        index: 2,
                    },
                ] {
                    label ("+ 创建当前游戏的房间", metrics.body, FOREGROUND, Center)
                } on Tap { state.choose_connection(2); }
            }
            art (state, Walker, metrics.walker_height())
        }
    }
}
