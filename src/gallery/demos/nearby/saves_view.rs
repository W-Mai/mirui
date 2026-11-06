use super::saves::schema::Tab;

const BODY_SIZE: u16 = 12;
const TITLE_SIZE: u16 = 24;
use super::saves::{Manager, Page};
use super::{
    actions::{ActionAnchor, ActionLayout},
    style::*,
    widgets::{control_key, control_label, shadow, text_block},
};
use crate::prelude::*;
use crate::ui::widgets::{
    Image, Text,
    TextAlign::{Center, Start},
};

pub(super) fn preview_texture() -> crate::render::texture::Texture<'static> {
    use crate::render::texture::{ColorFormat, Texture};
    static PIXELS: [u8; 48 * 32 * 4] = {
        let mut pixels = [0; 48 * 32 * 4];
        let mut y = 0;
        while y < 32 {
            let mut x = 0;
            while x < 48 {
                let color = if (x >= 20 && x < 28) && (y >= 15 && y < 25) {
                    [219, 108, 53]
                } else if y >= 25 || (x / 8 + y / 8) % 5 == 0 {
                    [66, 100, 59]
                } else {
                    [190, 206, 145]
                };
                let i = (y * 48 + x) * 4;
                pixels[i] = color[0];
                pixels[i + 1] = color[1];
                pixels[i + 2] = color[2];
                pixels[i + 3] = 255;
                x += 1;
            }
            y += 1;
        }
        pixels
    };
    Texture::expect_static(&PIXELS, 48, 32, ColorFormat::RGBA8888)
}

#[compose]
pub(super) fn panel(state: model!(Manager), metrics: Metrics) -> Entity {
    let modal_layout = ActionLayout::SavesModal;
    let list_layout = ActionLayout::SavesList;
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
            text_block (${ state.identity() }, SECONDARY, BODY_SIZE, Start, 0, 1)
            Row (height: 24, shrink: 0.0, column_gap: metrics.gap) {
                walk 0_usize..3 with index {
                    tab (state, index, metrics)
                }
            }
            if ${ !state.busy() && !state.modal() && !state.status().is_empty() } {
                text_block (${ state.status() }, ACCENT, BODY_SIZE, Start, 0, 1)
            }
            if ${ state.busy() } {
                Column (
                    grow: 1.0,
                    min_width: 0,
                    min_height: 0,
                    justify: JustifyContent::Center,
                    row_gap: metrics.gap * 3
                ) {
                    text_block (${ state.status() }, FOREGROUND, TITLE_SIZE, Center, 0, 2)
                    View (height: 8, bg_color: PANEL, container: true) {
                        View (
                            position: Position::Absolute,
                            left: ${ Dimension::Percent(state.span_start() * Fixed::from_int(100)) },
                            top: 0,
                            height: 8,
                            width: ${ Dimension::Percent((state.span_end() - state.span_start()) * Fixed::from_int(100)) },
                            bg_color: ACCENT
                        )
                        View (
                            position: Position::Absolute,
                            left: ${ Dimension::Percent(state.span_end() * Fixed::from_int(100)) },
                            top: 0,
                            width: 2,
                            height: 8
                        ) [
                            ActionAnchor::Item {
                                index: 0,
                                layout: ActionLayout::SavesProgress,
                            },
                        ]
                    }
                }
            } else {
                Column (grow: 1.0, min_width: 0, min_height: 0, container: true) {
                    walk ${ core::iter::once(state.tab()) } with tab by tab {
                        Row (grow: 1.0, min_height: 0, min_width: 0, column_gap: metrics.gap * 2) {
                            if tab == Tab::Session {
                                Column (
                                    grow: 1.0,
                                    min_width: 0,
                                    min_height: 0,
                                    row_gap: metrics.gap
                                ) {
                                    reading (state, metrics)
                                    View (height: 2, width: 2) [
                                        ActionAnchor::Item {
                                            index: 0,
                                            layout: ActionLayout::SavesList,
                                        },
                                    ]
                                }
                            } else {
                                Column (
                                    width: Dimension::percent(46),
                                    min_width: 0,
                                    min_height: 0,
                                    row_gap: metrics.gap,
                                    clip_children: true
                                ) {
                                    walk ${ state.list_window() } with index by index {
                                        row (state, index, metrics, list_layout, false)
                                    }
                                } on SizeChanged {
                                    state
                                        .viewport(
                                            event.new.height + Fixed::from_int(metrics.gap),
                                            metrics.save_row_extent(),
                                        );
                                }
                                Column (
                                    grow: 1.0,
                                    min_width: 0,
                                    min_height: 0,
                                    row_gap: metrics.gap
                                ) {
                                    preview (state, metrics)
                                    facts (state, metrics)
                                }
                            }
                        }
                    }
                    walk ${ state.modal().then(|| state.page()) } with page by page {
                        Column (
                            position: Position::Absolute,
                            left: 0,
                            top: 0,
                            width: Dimension::percent(100),
                            height: Dimension::percent(100),
                            bg_color: Color::rgba(7, 16, 12, 180),
                            padding: Padding::all(metrics.gap),
                            justify: JustifyContent::Center
                        ) {
                            Column (
                                grow: 1.0,
                                min_width: 0,
                                min_height: 0,
                                padding: Padding::all(metrics.gap),
                                row_gap: metrics.gap,
                                bg_color: BACKGROUND,
                                border_width: 2,
                                border_color: DIVIDER,
                                clip_children: true
                            ) {
                                if page == Page::Options {
                                    Column (
                                        grow: 1.0,
                                        min_width: 0,
                                        min_height: 0,
                                        row_gap: metrics.gap
                                    ) {
                                        walk ${ state.window() } with index by index {
                                            row (state, index, metrics, modal_layout, false)
                                        }
                                    } on SizeChanged {
                                        state
                                            .modal_viewport(
                                                event.new.height + Fixed::from_int(metrics.gap),
                                                metrics.save_row_extent(),
                                            );
                                    }
                                } else {
                                    Column (
                                        grow: 1.0,
                                        min_width: 0,
                                        min_height: 0,
                                        row_gap: metrics.gap
                                    ) {
                                        if page == Page::Room {
                                            text_block (
                                                "退出房间后可管理单机存档",
                                                FOREGROUND,
                                                TITLE_SIZE,
                                                Start,
                                                0,
                                                3
                                            )
                                        }
                                        if page == Page::Failure {
                                            text_block (
                                                ${ state.status() },
                                                WARNING,
                                                BODY_SIZE,
                                                Start,
                                                0,
                                                4
                                            )
                                        }
                                        Column (grow: 1.0, min_height: 0, min_width: 0) {
                                            if page == Page::Location {
                                                reading (state, metrics)
                                            } else {
                                                facts (state, metrics)
                                            }
                                        }
                                        View (grow: 1.0, min_height: 0)
                                        Row (
                                            height: metrics.save_row_height(),
                                            min_width: 0,
                                            shrink: 0.0,
                                            column_gap: metrics.gap
                                        ) {
                                            walk ${ 0_usize..state.count() } with index by index {
                                                row (state, index, metrics, modal_layout, true)
                                            }
                                        }
                                    }
                                }
                            }
                            shadow (metrics)
                        }
                    }
                }
            }
        }
    }
}
#[compose]
fn tab(state: model!(Manager), index: usize, _metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: 2) {
            Text (
                text: ["即时存档", "游戏进度", "联机"][index],
                font: font(BODY_SIZE),
                font_size: BODY_SIZE,
                text_color: ${
                    if state.tab() == [Tab::States, Tab::Sram, Tab::Session][index] {
                        ACCENT
                    } else {
                        SECONDARY
                    }
                },
                paragraph: super::widgets::paragraph(Center, 1)
            ) [
                FONT_PAINT,
            ]
            View (
                height: 2,
                min_width: 0,
                bg_color: ${
                    if state.tab() == [Tab::States, Tab::Sram, Tab::Session][index] {
                        ACCENT
                    } else {
                        DIVIDER
                    }
                }
            )
        }
    }
}
#[compose]
fn row(
    state: model!(Manager),
    index: usize,
    metrics: Metrics,
    layout: ActionLayout,
    horizontal: bool,
) -> Entity {
    let modal = layout == ActionLayout::SavesModal;
    ui! {
        Column (
            grow: if horizontal { 1.0 } else { 0.0 },
            min_width: 0,
            height: metrics.save_row_height(),
            shrink: 0.0
        ) {
            Row (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                padding: Padding::all(metrics.gap),
                column_gap: metrics.gap,
                align: AlignItems::Center,
                border_width: 2,
                border_color: ${
                    if (if modal { state.selected() } else { state.list_selection() }) == index {
                        ACCENT
                    } else {
                        DIVIDER
                    }
                },
                bg_color: ${
                    if (if modal { state.selected() } else { state.list_selection() }) == index {
                        ACCENT_DIM
                    } else {
                        PANEL
                    }
                },
                transform: ${
                    crate::types::Transform::translate(
                        Fixed::ZERO,
                        Fixed::from_int(
                            if (if modal { state.selected() } else { state.list_selection() }) == index {
                                super::motion::hop(state.age(), 2.0)
                            } else {
                                0
                            },
                        ),
                    )
                }
            ) [
                ActionAnchor::Item {
                    index,
                    layout,
                },
            ] {
                View (width: 3, height: 24, shrink: 0.0, bg_color: ACCENT)
                Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: 2) {
                    text_block (
                        ${ if modal { state.row(index).0 } else { state.record_row(index).0 } },
                        FOREGROUND,
                        TITLE_SIZE,
                        Start,
                        0,
                        1
                    )
                    if ${ !modal && !state.record_row(index).1.is_empty() } {
                        text_block (
                            ${ state.record_row(index).1 },
                            SECONDARY,
                            BODY_SIZE,
                            Start,
                            0,
                            1
                        )
                    }
                }
            }
            shadow (metrics)
        }
    }
}
#[compose]
fn facts(state: model!(Manager), metrics: Metrics) -> Entity {
    ui! {
        Column (
            min_width: 0,
            height: ${ state.facts_extent(metrics.gap) },
            shrink: 0.0,
            row_gap: metrics.gap
        ) {
            walk ${ 0_usize..state.fact_count() } with index by index {
                fact (state, index)
            }
        }
    }
}
#[compose]
fn fact(state: model!(Manager), index: usize) -> Entity {
    ui! {
        Row (min_width: 0, height: ${ state.fact_extent(index) }, shrink: 0.0, column_gap: 4) {
            Column (width: 60, min_height: i32::from(BODY_SIZE), shrink: 0.0) {
                text_block (
                    ${ state.fact(index).map(|f| f.label).unwrap_or_default() },
                    SECONDARY,
                    BODY_SIZE,
                    Start,
                    0,
                    1
                )
            }
            Column (grow: 1.0, min_width: 0, min_height: i32::from(BODY_SIZE)) {
                Text (
                    text: ${ state.fact(index).map(|f| f.value).unwrap_or_default() },
                    height: Dimension::Content,
                    width: Dimension::percent(100),
                    min_width: 0,
                    min_height: i32::from(BODY_SIZE),
                    font: font(BODY_SIZE),
                    font_size: BODY_SIZE,
                    text_color: FOREGROUND,
                    paragraph: super::widgets::paragraph(Start, 6)
                ) [
                    FONT_PAINT,
                ] on SizeChanged { state.fact_height(index, event.new.height); }
            }
        }
    }
}
#[compose]
fn preview(state: model!(Manager), _metrics: Metrics) -> Entity {
    ui! {
        View (
            grow: 1.0,
            min_height: 48,
            min_width: 0,
            container: true,
            clip_children: true,
            border_width: 2,
            border_color: DIVIDER,
            bg_color: PANEL
        ) {
            Row (
                position: Position::Absolute,
                left: Dimension::percent(50),
                top: Dimension::percent(50),
                width: 32,
                height: 32,
                transform: ${ crate::types::Transform::translate(Fixed::from_int(-16), Fixed::from_int(-16)) }
            ) {
                Image (
                    src: path!(M 2 1 H 12 L 15 4 V 15 H 1 V 1 Z M 4 2 V 6 H 11 V 2 Z M 4 9 V 14 H 12 V 9 Z),
                    viewbox: 16,
                    width: 32,
                    height: 32,
                    color: SECONDARY
                )
            }
            if ${ !state.image().is_empty() } {
                Column (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100)
                ) {
                    Image (
                        src: ${ state.image() },
                        position: Position::Absolute,
                        left: ${ (state.preview_size().width - state.image_extent().width) / Fixed::from_int(2) },
                        top: ${ (state.preview_size().height - state.image_extent().height) / Fixed::from_int(2) },
                        width: ${ state.image_extent().width },
                        height: ${ state.image_extent().height }
                    )
                }
            }
        } on SizeChanged { state.resize_preview(event.new); }
    }
}
#[compose]
pub(super) fn hints(state: model!(Manager), metrics: Metrics) -> Entity {
    ui! {
        Row (grow: 1.0, min_width: 0, align: AlignItems::Center, column_gap: metrics.gap) {
            if ${ !state.busy() && state.tab() != Tab::Session } {
                control_key ("A", metrics, true)
                control_label ("确认", metrics)
            }
            control_key ("B", metrics, false)
            control_label ("返回", metrics)
            View (grow: 1.0, min_width: 0)
            if ${ !state.busy() && state.page() == Page::Browse && state.tab() != Tab::Session } {
                control_key ("X", metrics, false)
                control_label ("操作", metrics)
            }
            if ${ !state.modal() && !state.busy() } {
                control_key ("左右", metrics, false)
                control_label ("分区", metrics)
            }
        }
    }
}

#[component]
pub(super) struct SaveReading {
    state: model!(Manager),
}
#[system(order=Animation)]
fn scroll_reading(
    nodes: query!(
        Entity,
        SaveReading,
        crate::input::event::scroll::ScrollOffset
    ),
    commands: commands!(),
) {
    for (entity, node, offset) in nodes {
        let y = node.state.reading_offset();
        if node.state.active() && offset.y != y {
            commands.insert(
                entity,
                crate::input::event::scroll::ScrollOffset { x: Fixed::ZERO, y },
            );
        }
    }
}
#[view(component=SaveReading,systems=[scroll_reading::system()])]
pub(super) fn save_reading() {}
#[compose]
fn reading(state: model!(Manager), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, clip_children: true) [
            crate::input::event::scroll::ScrollOffset::default(),
            crate::input::event::scroll::ScrollConfig {
                elastic: false,
                ..Default::default()
            },
            SaveReading {
                state: state.clone(),
            },
        ] {
            facts (state, metrics)
        } on SizeChanged { state.reading_viewport(event.new.height, metrics.gap); }
    }
}
