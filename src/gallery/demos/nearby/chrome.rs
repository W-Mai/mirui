use super::data::{Page, Snapshot};
use super::model::{DisplayState, TextField};
use super::style::*;
use crate::prelude::*;
use crate::ui::OffscreenRender;
use crate::ui::widgets::{Text, TextAlign};
use TextAlign::{Center, End, Start};
use alloc::{borrow::ToOwned, format};
#[compose]
pub(super) fn label(value: &str, size: u16, color: Color, align: TextAlign) -> Entity {
    ui! {
        Text (
            value.to_owned(),
            width: Dimension::percent(100),
            height: Dimension::Content,
            min_width: 0,
            min_height: i32::from(size) + 6,
            font: font(size),
            font_size: size,
            text_color: color,
            paragraph: paragraph(align, 1),
            clip_children: true
        )
    }
}

#[compose]
pub(super) fn live_label(
    state: model!(DisplayState),
    field: TextField,
    size: u16,
    color: Color,
    align: TextAlign,
) -> Entity {
    ui! {
        Text (
            text: ${ state.text(field) },
            width: Dimension::percent(100),
            height: Dimension::Content,
            min_width: 0,
            min_height: i32::from(size) + 6,
            font: font(size),
            font_size: size,
            text_color: ${
                if field == TextField::HostButton && !state.action_enabled() {
                    DISABLED
                } else {
                    color
                }
            },
            paragraph: paragraph(align, 1),
            clip_children: true
        )
    }
}

#[compose]
pub(super) fn header(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let subtitle = if view.page == Page::Mode {
        "连接附近的掌机".into()
    } else if view.network_only {
        "仅组网 · 局域网连接".into()
    } else if view.page == Page::Games {
        view.core.clone()
    } else {
        format!("当前游戏  {}", view.title)
    };
    let step = if view.page == Page::Mode {
        "选择模式".into()
    } else if view.page == Page::Games {
        "选择游戏".into()
    } else {
        format!("步骤 {} / 2", if view.page == Page::Choice { 1 } else { 2 })
    };
    ui! {
        Row (
            height: if metrics.compact { 44 } else { 68 },
            shrink: 0.0,
            column_gap: metrics.gap + 4,
            align: AlignItems::Center
        ) {
            Row (
                width: if metrics.compact { 30 } else { 42 },
                height: if metrics.compact { 30 } else { 42 },
                shrink: 0.0,
                bg_color: ${ state.stamp() },
                justify: JustifyContent::Center,
                align: AlignItems::Center,
                border_radius: 0
            ) {
                View (
                    width: 7,
                    height: 18,
                    bg_color: ACCENT
                )
                View (
                    width: 7,
                    height: 6,
                    bg_color: ACCENT
                )
                View (
                    width: 7,
                    height: 18,
                    bg_color: ACCENT
                )
            }
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
                    24,
                    FOREGROUND,
                    Start
                )
                label (
                    &subtitle,
                    metrics.small,
                    SECONDARY,
                    Start
                )
            }
            Text (
                step,
                width: if metrics.compact { 76 } else { 96 },
                height: if metrics.compact { 26 } else { 34 },
                shrink: 0.0,
                bg_color: PANEL,
                font: font(metrics.small),
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(Center, 1)
            )
        }
    }
}

#[compose]
fn keycap(value: &'static str, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Text (
            value.to_owned(),
            width: if metrics.compact { 20 } else { 25 },
            height: if metrics.compact { 20 } else { 25 },
            shrink: 0.0,
            bg_color: PANEL,
            font: font(metrics.small),
            font_size: metrics.small,
            text_color: SECONDARY,
            paragraph: paragraph(Center, 1)
        ) on Tap {
            if value == "B" {
                state.back_page();
            } else {
                state.refresh_rooms();
            };
        }
    }
}

#[compose]
pub(super) fn footer(view: &Snapshot, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Row (
            height: if metrics.compact { 20 } else { 32 },
            shrink: 0.0,
            column_gap: metrics.gap,
            align: AlignItems::Center
        ) {
            Text (
                text: ${ if state.confirm_visible() { "A" } else { "" } },
                width: if metrics.compact { 20 } else { 25 },
                height: if metrics.compact { 20 } else { 25 },
                shrink: 0.0,
                bg_color: ${ if state.confirm_visible() { PANEL } else { BACKGROUND } },
                font: font(metrics.small),
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(Center, 1)
            ) on Tap {
                if state.confirm_visible() {
                    state.confirm();
                }
            }
            Text (
                text: ${ state.confirm_label() },
                width: Dimension::Content,
                shrink: 0.0,
                height: Dimension::Content,
                font: font(metrics.small),
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(Start, 1)
            ) on Tap {
                if state.confirm_visible() {
                    state.confirm();
                }
            }
            keycap ("B", state, metrics)
            Text (
                text: ${ state.back() },
                max_width: if metrics.compact { 130 } else { 210 },
                min_width: 0,
                height: Dimension::Content,
                font: font(metrics.small),
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(Start, 1)
            ) on Tap { state.back_page(); }
            if view.page == Page::Rooms {
                keycap ("X", state, metrics)
                Text (
                    "刷新",
                    height: Dimension::Content,
                    font: font(metrics.small),
                    font_size: metrics.small,
                    text_color: SECONDARY,
                    paragraph: paragraph(Start, 1)
                ) on Tap { state.refresh_rooms(); }
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
            )
        }
    }
}
