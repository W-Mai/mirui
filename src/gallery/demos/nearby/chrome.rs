use super::data::{Page, Snapshot};
use super::widgets::label;
use super::{
    model::DisplayState,
    pixel::{Scenery, Scenery::Header, backdrop},
    style::*,
    widgets::{control_key, control_label, paragraph},
};
use crate::prelude::*;
use crate::ui::OffscreenRender;
use crate::ui::widgets::Text;
use crate::ui::widgets::TextAlign::{Center, End, Start};
use alloc::{format, string::ToString};

#[compose]
pub(super) fn header(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let page = view.page;
    let step = if page == Page::Lobby {
        "附近房间".into()
    } else if page == Page::Mode {
        "选择模式".into()
    } else if matches!(page, Page::Games | Page::Candidates) {
        "选择游戏".into()
    } else {
        format!("步骤 {} / 2", if page == Page::Choice { 1 } else { 2 })
    };
    ui! {
        Row (
            height: ${
                if state.launcher().active() || page == Page::Games {
                    metrics.launcher_header_height()
                } else {
                    metrics.header_height()
                }
            },
            shrink: 0.0,
            column_gap: metrics.gap + 4,
            padding: metrics.header_padding(),
            bg_color: DEEP,
            align: AlignItems::Center
        ) {
            backdrop (Header)
            View (
                width: metrics.logo_size(),
                height: metrics.logo_size(),
                shrink: 0.0
            ) [
                Scenery::Logo,
                StaticCache,
                OffscreenRender::default(),
            ]
            Column (
                grow: 1.0,
                min_width: 0,
                justify: JustifyContent::Center,
                clip_children: true
            ) [
                StaticCache,
                OffscreenRender::default(),
            ] {
                label (
                    "附近联机",
                    metrics.title,
                    HEADER_TEXT,
                    Start
                )
                Text (
                    text: ${ state.subtitle() },
                    font: font(metrics.small),
                    font_size: metrics.small,
                    text_color: HEADER_DETAIL,
                    height: Dimension::Content,
                    paragraph: paragraph(Start, 1)
                ) [
                    FONT_PAINT,
                ]
            }
            Text (
                text: ${
                    if page == Page::Games {
                        state
                            .library_counter()
                            .map(|(position, total)| format!("{position} / {total}"))
                            .unwrap_or_default()
                    } else {
                        step.clone()
                    }
                },
                width: if metrics.compact { 76 } else { 96 },
                height: if metrics.compact { 26 } else { 34 },
                shrink: 0.0,
                bg_color: DEEP,
                font: font(metrics.small),
                font_size: metrics.small,
                text_color: HEADER_DETAIL,
                paragraph: paragraph(Center, 1)
            ) [
                FONT_PAINT,
            ]
        }
    }
}

#[compose]
pub(super) fn footer(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: metrics.footer_height() + metrics.control_gap() + 1,
            shrink: 0.0,
            row_gap: metrics.control_gap()
        ) {
            View (height: 1, shrink: 0.0, bg_color: DIVIDER) {
                View (
                    height: Dimension::percent(100),
                    width: ${
                        Dimension::Percent(
                            Fixed::from_int(
                                i32::from((state.launcher().hold_progress() * 1000).to_int() as u16),
                            ) / Fixed::from_int(10),
                        )
                    },
                    bg_color: ACCENT
                )
            }
            if ${ (state.launcher().hold_progress() * 1000).to_int() as u16 > 0 } {
                Row (
                    height: metrics.footer_height(),
                    shrink: 0.0,
                    align: AlignItems::Center,
                    justify: JustifyContent::Center
                ) {
                    label (
                        "继续按住 X，打开附近联机",
                        metrics.small,
                        FOREGROUND,
                        Center
                    )
                }
            } else {
                footer_controls (view, state, metrics)
            }
        }
    }
}

#[compose]
fn footer_controls(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let confirm = state.clone();
    let back = state.clone();
    ui! {
        Row (
            height: metrics.footer_height(),
            shrink: 0.0,
            column_gap: metrics.control_gap(),
            align: AlignItems::Center
        ) {
            if ${ state.confirm_visible() } {
                control_key ("A", metrics, true)
                control_label (move || confirm.confirm_label().to_string(), metrics)
            }
            if ${
                !state.launcher().active()
                    || (state.launcher().screen() != super::launcher::Screen::Favorites
                        && !state.presentation().busy)
            } {
                control_key ("B", metrics, false)
                control_label (move || back.back().to_string(), metrics)
            }
            if matches!(view.page, Page::Rooms | Page::Lobby) {
                control_key ("X", metrics, false)
                control_label (|| "刷新".into(), metrics)
            }
            if ${ state.can_switch_theme() } {
                control_key ("Y", metrics, false)
                control_label (|| "主题".into(), metrics)
            }
            Text (
                text: ${ state.hint() },
                grow: 1.0,
                min_width: 0,
                height: Dimension::Content,
                font: font(metrics.small),
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(End, 1),
                clip_children: true
            ) [
                FONT_PAINT,
            ]
            if ${ state.launcher().active() && !state.presentation().busy } {
                control_key ("SELECT", metrics, false)
                control_label (|| "联机".into(), metrics)
            }
            if ${
                state.launcher().active() && !state.presentation().busy
                    && state.launcher().screen() != super::launcher::Screen::Search
            } {
                control_key ("START", metrics, false)
                control_label (|| "设置".into(), metrics)
            }
        }
    }
}
