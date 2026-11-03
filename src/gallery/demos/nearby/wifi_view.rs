use super::launcher::Launcher;
use super::wifi::Page;
use super::{
    actions::{ActionAnchor, ActionLayout},
    style::*,
    widgets::{control_key, control_label, shadow, text_block},
};
use crate::prelude::*;
use crate::ui::widgets::TextAlign::Start;
use crate::ui::widgets::{Image, Text};
use alloc::string::{String, ToString};
const TITLE_SIZE: u16 = 24;
const BODY_SIZE: u16 = 12;

fn label(state: &super::launcher::LauncherHandle, index: usize) -> String {
    match state.wifi_page() {
        Page::List => state
            .wifi_row(index)
            .map(|_| state.wifi_name(index))
            .unwrap_or_else(|| "添加网络".into()),
        Page::Off => "开启 Wi-Fi".into(),
        Page::Empty => if index == 0 { "刷新" } else { "添加网络" }.into(),
        Page::Security => if index == 0 {
            "密码网络"
        } else {
            "开放网络"
        }
        .into(),
        Page::Details => if index == 0 { "断开" } else { "忘记" }.into(),
        Page::Forget => if index == 0 { "取消" } else { "忘记" }.into(),
        Page::Failed => if index == 0 { "重试" } else { "返回" }.into(),
        Page::Success => "完成".into(),
        Page::Busy if state.connected() => "返回房间".into(),
        _ => "返回".into(),
    }
}
#[compose]
pub(super) fn panel(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            if ${ !state.wifi_text(1).is_empty() } {
                text_block (${ state.wifi_text(1) }, WARNING, BODY_SIZE, Start, 0, 2)
            }
            match ${ state.wifi_page() } {
                Page :: Password | Page :: Hidden => {
                    keyboard (state, metrics)
                }
                Page :: List => {
                    scan_label (state)
                    Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
                        walk ${ state.wifi_window() } with index by index {
                            row (state, index, true, metrics)
                        }
                    } on SizeChanged { state.wifi_height(event.new.height, metrics.wifi_row_extent(), metrics.gap); }
                }
                Page :: Connecting | Page :: Cancelling | Page :: Restoring => {
                    progress (state, metrics)
                }
                _ => {
                    options (state, metrics)
                }
            }
        }
    }
}
#[compose]
fn scan_label(state: model!(Launcher)) -> Entity {
    ui! {
        Row (height: 16, shrink: 0.0) {
            text_block (
                ${ if state.wifi_scanning() { "扫描中".into() } else { String::new() } },
                SECONDARY,
                BODY_SIZE,
                Start,
                0,
                1
            )
        }
    }
}
#[compose]
fn options(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            match ${ state.wifi_page() } {
                Page :: Info | Page :: Details | Page :: Success => {
                    facts (state, metrics)
                }
                Page :: Off => {
                    text_block (
                        "Wi-Fi 已关闭",
                        FOREGROUND,
                        TITLE_SIZE,
                        Start,
                        0,
                        2
                    )
                }
                Page :: Empty => {
                    text_block (
                        "还没发现网络",
                        FOREGROUND,
                        TITLE_SIZE,
                        Start,
                        0,
                        2
                    )
                }
                Page :: Busy => {
                    text_block (
                        "无线网络正在使用中",
                        FOREGROUND,
                        TITLE_SIZE,
                        Start,
                        0,
                        2
                    )
                }
                Page :: Forget => {
                    text_block (${ state.wifi_text(0) }, FOREGROUND, TITLE_SIZE, Start, 0, 2)
                    text_block (
                        "删除此网络保存的连接信息",
                        SECONDARY,
                        BODY_SIZE,
                        Start,
                        0,
                        2
                    )
                }
                _ => {
                }
            }
            View (grow: 1.0, min_height: 0)
            Row (
                height: metrics.wifi_row_height() + metrics.shadow_height(),
                shrink: 0.0,
                column_gap: metrics.gap,
                min_width: 0
            ) {
                row (state, 0, false, metrics)
                if ${
                    matches!(
                        state.wifi_page(), Page::Details | Page::Forget | Page::Security | Page::Empty |
                        Page::Failed
                    )
                } {
                    row (state, 1, false, metrics)
                }
            }
        }
    }
}

#[compose]
fn row(state: model!(Launcher), index: usize, list: bool, metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: if list { 0.0 } else { 1.0 },
            height: metrics.wifi_row_height() + metrics.shadow_height(),
            shrink: 0.0,
            min_width: 0
        ) {
            Row (
                height: metrics.wifi_row_height(),
                min_width: 0,
                align: AlignItems::Center,
                padding: Padding::all(metrics.gap),
                column_gap: metrics.gap,
                bg_color: ${ if state.menu_selection() == index { ACCENT_DIM } else { PANEL } },
                border_width: 2,
                border_color: ${ if state.menu_selection() == index { ACCENT } else { DIVIDER } },
                transform: ${
                    crate::types::Transform::translate(
                        Fixed::ZERO,
                        Fixed::from_int(
                            if state.menu_selection() == index {
                                super::motion::hop(state.selection_age(), 2.0)
                            } else {
                                0
                            },
                        ),
                    )
                }
            ) [
                ActionAnchor::Item {
                    index,
                    layout: if list { ActionLayout::WifiList } else { ActionLayout::WifiOptions },
                },
            ] {
                View (width: 3, height: 24, shrink: 0.0, bg_color: ACCENT)
                if list {
                    network_icon (state, index)
                }
                Text (
                    text: ${ label(&state, index) },
                    font: font(TITLE_SIZE),
                    font_size: TITLE_SIZE,
                    text_color: FOREGROUND,
                    grow: 1.0,
                    min_width: 0,
                    paragraph: super::widgets::paragraph(Start, 1)
                ) [
                    FONT_PAINT,
                ]
                if list {
                    Column (width: 36, shrink: 0.0, min_height: 12) {
                        text_block (
                            ${
                                state
                                    .wifi_row(index)
                                    .map(|r| {
                                        if r.connected {
                                            "已连接"
                                        } else if r.saved {
                                            "已保存"
                                        } else if r.security == super::wifi::Security::Open {
                                            "开放"
                                        } else {
                                            ""
                                        }
                                            .to_string()
                                    })
                                    .unwrap_or_default()
                            },
                            SECONDARY,
                            BODY_SIZE,
                            Start,
                            0,
                            1
                        )
                    }
                    if ${ state.wifi_row(index).is_some() } {
                        signal (state, index)
                    }
                }
            } on HoverChanged {
                if event.hovered {
                    state.select_action(index);
                }
            } on Tap {
                state.select_action(index);
                state.wifi_input(super::wifi::Action::Confirm);
            }
            shadow (metrics)
        }
    }
}
#[compose]
fn signal(state: model!(Launcher), index: usize) -> Entity {
    ui! {
        Row (width: 24, height: 20, align: AlignItems::FlexEnd, column_gap: 2, shrink: 0.0) {
            walk 0_usize..4 with bar {
                View (
                    width: 4,
                    height: (bar as i32 + 1) * 5,
                    bg_color: ${
                        if state.wifi_row(index).is_some_and(|r| usize::from(r.signal) > bar * 24) {
                            ACCENT
                        } else {
                            DIVIDER
                        }
                    },
                    transform: ${
                        if state.wifi_scanning() {
                            let scale = Fixed::from_ratio(
                                i32::from((state.wifi_pulse() + bar as u16) % 4) + 3,
                                6,
                            );
                            crate::types::Transform::translate(
                                    Fixed::ZERO,
                                    Fixed::from_int((bar as i32 + 1) * 5) * (Fixed::ONE - scale),
                                )
                                .compose(&crate::types::Transform::scale(Fixed::ONE, scale))
                        } else {
                            crate::types::Transform::IDENTITY
                        }
                    }
                )
            }
        }
    }
}

#[compose]
fn network_icon(state: model!(Launcher), index: usize) -> Entity {
    ui! {
        Column (width: 22, height: 22, shrink: 0.0) {
            Image (
                width: 22,
                height: 22,
                viewbox: 16,
                color: SECONDARY,
                src: ${ network_source(&state, index) }
            )
        }
    }
}
fn network_source(
    state: &super::launcher::LauncherHandle,
    index: usize,
) -> crate::ui::widgets::ImageSource {
    match network_glyph(state, index) {
        NetworkGlyph::Add => {
            path!(M 6 1 H 10 V 6 H 15 V 10 H 10 V 15 H 6 V 10 H 1 V 6 H 6 Z)
        }
        NetworkGlyph::Connected => {
            path!(
                M 1 7 H 5 V 11 H 8 V 7 H 11 V 3 H 15 V 8 H 12 V 12 H 9 V 15 H 5 V 12 H 1
                Z
            )
        }
        NetworkGlyph::Open => {
            path!(
                M 4 1 H 12 V 4 H 15 V 12 H 12 V 15 H 4 V 12 H 1 V 4 H 4 Z M 5 4 V 7 H 11
                V 4 Z M 5 10 V 12 H 11 V 10 Z
            )
        }
        NetworkGlyph::Locked => {
            path!(
                M 4 7 V 3 H 6 V 1 H 10 V 3 H 12 V 7 H 14 V 15 H 2 V 7 Z M 6 7 H 10 V 4 H
                6 Z M 7 10 V 13 H 9 V 10 Z
            )
        }
    }
    .into()
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum NetworkGlyph {
    Add,
    Connected,
    Open,
    Locked,
}
fn network_glyph(state: &super::launcher::LauncherHandle, index: usize) -> NetworkGlyph {
    match state.wifi_row(index) {
        None => NetworkGlyph::Add,
        Some(row) if row.connected => NetworkGlyph::Connected,
        Some(row) if row.security == super::wifi::Security::Open => NetworkGlyph::Open,
        _ => NetworkGlyph::Locked,
    }
}
#[compose]
fn keyboard(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
            Row (
                height: 36,
                shrink: 0.0,
                padding: Padding::all(4),
                bg_color: PANEL,
                border_width: 2,
                border_color: ACCENT,
                min_width: 0
            ) {
                Text (
                    text: ${ state.wifi_editor(35) },
                    font: font(TITLE_SIZE),
                    font_size: TITLE_SIZE,
                    text_color: FOREGROUND,
                    grow: 1.0,
                    min_width: 0,
                    paragraph: super::widgets::paragraph(Start, 1)
                ) [
                    FONT_PAINT,
                ]
                Text (
                    text: ${ state.wifi_length().to_string() },
                    font: font(BODY_SIZE),
                    font_size: BODY_SIZE,
                    text_color: SECONDARY,
                    width: 28,
                    shrink: 0.0
                ) [
                    FONT_PAINT,
                ]
            }
            Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: 3) {
                walk 0_usize..4 with line {
                    Row (grow: 1.0, min_height: 24, min_width: 0, column_gap: 3) {
                        walk 0_usize..(if line == 3 { 5 } else { 10 }) with column {
                            key (
                                state,
                                if line == 3 { 30 + column } else { line * 10 + column },
                                metrics
                            )
                        }
                    }
                }
            }
            View (height: 20, shrink: 0.0, min_width: 0) [
                ActionAnchor::WifiKeyboard,
            ]
        }
    }
}
#[compose]
fn key(state: model!(Launcher), index: usize, _metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            bg_color: ${ if state.menu_selection() == index { ACCENT } else { PANEL } },
            border_width: 1,
            border_color: ${ if state.menu_selection() == index { FOREGROUND } else { DIVIDER } },
            transform: ${
                crate::types::Transform::translate(
                    Fixed::ZERO,
                    Fixed::from_int(
                        if state.menu_selection() == index {
                            super::motion::hop(state.selection_age(), 2.0)
                        } else {
                            0
                        },
                    ),
                )
            },
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            Text (
                text: ${ state.wifi_editor(index) },
                font: font(if index >= 30 { BODY_SIZE } else { TITLE_SIZE }),
                font_size: if index >= 30 { BODY_SIZE } else { TITLE_SIZE },
                text_color: ${ if state.menu_selection() == index { BACKGROUND } else { FOREGROUND } },
                paragraph: super::widgets::paragraph(Start, 1)
            ) [
                FONT_PAINT,
            ]
        } on HoverChanged {
            if event.hovered {
                state.select_action(index);
            }
        } on Tap {
            state.select_action(index);
            state.wifi_input(super::wifi::Action::Confirm);
        }
    }
}
#[compose]
fn progress(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Row (
                grow: 1.0,
                min_height: 50,
                align: AlignItems::Center,
                column_gap: metrics.gap,
                bg_color: PANEL,
                padding: Padding::all(metrics.gap * 2)
            ) {
                walk 0_usize..3 with stage {
                    Column (
                        grow: 1.0,
                        min_width: 0,
                        row_gap: metrics.gap,
                        align: AlignItems::Center
                    ) {
                        View (
                            width: 24,
                            height: 24,
                            border_width: 2,
                            border_color: DIVIDER,
                            bg_color: ${ if usize::from(state.wifi_stage()) >= stage { ACCENT } else { BACKGROUND } }
                        )
                    }
                }
            }
            View (height: 24, shrink: 0.0) [
                ActionAnchor::WifiProgress,
            ]
            Row (shrink: 0.0, min_height: i32::from(BODY_SIZE), column_gap: metrics.gap) {
                walk 0_usize..3 with stage {
                    Column (grow: 1.0, min_width: 0, min_height: i32::from(BODY_SIZE)) {
                        stage_label (state, stage)
                    }
                }
            }
        }
    }
}
#[compose]
fn facts(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Column (
            shrink: 0.0,
            min_width: 0,
            row_gap: 4,
            padding: Padding::all(metrics.gap),
            bg_color: PANEL
        ) {
            walk 0_usize..5 with index {
                fact (state, index, metrics)
                View (height: 1, shrink: 0.0, bg_color: DIVIDER)
            }
        }
    }
}

#[compose]
fn fact(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    ui! {
        Row (
            min_width: 0,
            min_height: i32::from(BODY_SIZE) + 4,
            shrink: 0.0,
            column_gap: metrics.gap
        ) {
            Column (width: 64, min_height: i32::from(BODY_SIZE), shrink: 0.0) {
                text_block (
                    ["网络", "IP 地址", "网关", "信号", "安全"][index],
                    SECONDARY,
                    BODY_SIZE,
                    Start,
                    0,
                    1
                )
            }
            Column (grow: 1.0, min_width: 0, min_height: i32::from(BODY_SIZE)) {
                text_block (
                    ${ state.wifi_fact(index) },
                    FOREGROUND,
                    BODY_SIZE,
                    Start,
                    0,
                    2
                )
            }
        }
    }
}
#[compose]
fn stage_label(state: model!(Launcher), stage: usize) -> Entity {
    ui! {
        text_block (
            ["连接网络", "获取地址", "已连接"][stage],
            ${ if usize::from(state.wifi_stage()) == stage { FOREGROUND } else { SECONDARY } },
            BODY_SIZE,
            Start,
            0,
            1
        )
    }
}
#[compose]
pub(super) fn heading(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Row (
            height: 30,
            shrink: 0.0,
            min_width: 0,
            column_gap: metrics.gap,
            align: AlignItems::Center
        ) {
            Image (
                src: path!(
                    M 1 2 H 15 V 4 H 1 Z M 3 6 H 13 V 8 H 3 Z M 5 10 H 11 V 12 H 5 Z M 7 14 H 9 V 16
                    H 7 Z
                ),
                viewbox: 16,
                width: 24,
                height: 24,
                color: ACCENT
            )
            Text (
                text: ${ state.overlay_title() },
                font: font(TITLE_SIZE),
                font_size: TITLE_SIZE,
                text_color: FOREGROUND,
                grow: 1.0,
                min_width: 0,
                paragraph: super::widgets::paragraph(Start, 1)
            ) [
                FONT_PAINT,
            ]
        }
    }
}

#[compose]
pub(super) fn hints(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Row (min_width: 0, column_gap: metrics.gap, align: AlignItems::Center) {
            if ${ state.wifi_page().pending() } {
                control_key ("B", metrics, true)
                control_label (
                    ${
                        if state.wifi_page() == Page::Connecting { "取消" } else { "恢复中" }
                            .to_string()
                    },
                    metrics
                )
            } else {
                Row (min_width: 0, column_gap: metrics.gap, align: AlignItems::Center) {
                    control_key ("A", metrics, true)
                    control_label (
                        ${
                            if state.wifi_keyboard() {
                                "输入"
                            } else if state.wifi_page() == Page::List {
                                "选择"
                            } else {
                                "确认"
                            }
                                .to_string()
                        },
                        metrics
                    )
                    control_key ("B", metrics, false)
                    control_label ("返回", metrics)
                    match ${ state.wifi_page() } {
                        Page :: List => {
                            control_key ("X", metrics, false)
                            control_label ("刷新", metrics)
                            control_key ("Y", metrics, false)
                            control_label ("网络信息", metrics)
                        }
                        Page :: Password | Page :: Hidden => {
                            control_key ("X", metrics, false)
                            control_label ("删除", metrics)
                            control_key ("Y", metrics, false)
                            control_label (${ state.wifi_echo() }, metrics)
                            control_key ("START", metrics, false)
                            control_label (
                                ${ if state.wifi_page() == Page::Hidden { "下一步" } else { "连接" }.to_string() },
                                metrics
                            )
                        }
                        _ => {
                        }
                    }
                }
            }
        }
    }
}
