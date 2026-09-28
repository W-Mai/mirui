use super::binding::{POOL_SIZE, ROW_HEIGHT, VIRTUAL_CONTENT_HEIGHT, VIRTUAL_ITEM_COUNT, bind_row};
use super::style::{CompactTheme, bitmap_label};
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::render::font::FontToken;
use crate::ui::theme;
use crate::ui::widgets::{
    Button, ButtonSize, LazyList, LazyListBinder, LazyListPool, ProgressBar, Slider, Switch,
    TabBar, TabContent, Text, TextAlign,
};
use crate::ui::{Children, IdMap, Theme};

#[compose]
pub fn build_widgets() {
    if cx.world_mut().resource::<IdMap>().is_none() {
        cx.world_mut().insert_resource(IdMap::new());
    }
    let progress = Signal::new(Fixed::from_ratio(62, 100));
    let progress_from_slider = progress.clone();

    ui! {
        View (
            id: "compact_widgets_shell",
            grow: 1.0,
            direction: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                super::compact_layout::shell_direction(compact_widgets_shell.width, compact_widgets_shell.height)
            },
            padding: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                super::compact_layout::padding(compact_widgets_shell.width, compact_widgets_shell.height, 4, 6)
            },
            row_gap: 4,
            column_gap: 4,
            bg_color: ColorToken::Surface
        ) {
            View (
                id: "compact_widgets_header",
                width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::header_width(compact_widgets_shell.width, compact_widgets_shell.height, 30)
                },
                height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::header_height(compact_widgets_shell.width, compact_widgets_shell.height)
                },
                direction: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::header_direction(compact_widgets_shell.width, compact_widgets_shell.height)
                },
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                row_gap: 3,
                column_gap: 4
            ) {
                View (
                    id: "compact_widgets_marker",
                    width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::marker_width(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::marker_height(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    bg_color: ColorToken::Primary,
                    border_radius: 2
                )
                Text (
                    id: "compact_widgets_title",
                    "WIDGETS",
                    grow: 1.0,
                    width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::title_width(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::title_height(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: ColorToken::OnSurface,
                    paragraph: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        bitmap_label(super::compact_layout::text_align(compact_widgets_shell.width, compact_widgets_shell.height))
                    }
                )
                Text (
                    id: "compact_widgets_mode",
                    "AUTO",
                    width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::mode_width(compact_widgets_shell.width, compact_widgets_shell.height, 30)
                    },
                    height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::mode_height(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        bitmap_label(super::compact_layout::mode_align(compact_widgets_shell.width, compact_widgets_shell.height))
                    }
                )
            }
            Column (
                id: "compact_widgets_body",
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::content_width(compact_widgets_shell.width, compact_widgets_shell.height)
                },
                height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::content_height(compact_widgets_shell.width, compact_widgets_shell.height)
                },
                row_gap: 4
            ) {
                TabBar (
                    id: "compact_widgets_tabs",
                    width: Dimension::percent(100),
                    height: 20,
                    count: 3,
                    indicator_height: Fixed::from_int(2),
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 8,
                    clip_children: true
                ) {
                    Text (
                        id: "compact_tab_list",
                        "LIST",
                        grow: 1.0,
                        height: 20,
                        font: FontToken::Default,
                        font_size: 6,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: bitmap_label(TextAlign::Center)
                    )
                    Text (
                        id: "compact_tab_controls",
                        "CTRL",
                        grow: 1.0,
                        height: 20,
                        font: FontToken::Default,
                        font_size: 6,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: bitmap_label(TextAlign::Center)
                    )
                    Text (
                        id: "compact_tab_color",
                        "THEME",
                        grow: 1.0,
                        height: 20,
                        font: FontToken::Default,
                        font_size: 6,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: bitmap_label(TextAlign::Center)
                    )
                }
                View (
                    grow: 1.0,
                    width: Dimension::percent(100),
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 10,
                    clip_children: true
                ) {
                    LazyList (
                        id: "compact_widgets_list",
                        position: Position::Absolute,
                        left: 0,
                        top: 0,
                        width: Dimension::percent(100),
                        height: Dimension::percent(100),
                        bg_color: ColorToken::SurfaceVariant,
                        item_count: VIRTUAL_ITEM_COUNT,
                        item_height: Fixed::from_int(ROW_HEIGHT),
                        pool_size: POOL_SIZE as u8
                    ) [
                        TabContent {
                            tab_bar: id("compact_widgets_tabs"),
                            index: 0,
                        },
                        LazyListBinder { bind: bind_row },
                        ScrollOffset {
                            x: Fixed::ZERO,
                            y: Fixed::ZERO,
                        },
                        ScrollConfig {
                            direction: ScrollAxis::Vertical,
                            elastic: false,
                            content_height: Fixed::from_int(VIRTUAL_CONTENT_HEIGHT),
                            content_width: Fixed::ZERO,
                        },
                    ] {
                        walk 0..POOL_SIZE with _index {
                            Row (
                                position: Position::Absolute,
                                left: 0,
                                top: 0,
                                width: Dimension::percent(100),
                                height: ROW_HEIGHT,
                                padding: Padding {
                                    top: Dimension::px(0),
                                    right: Dimension::px(7),
                                    bottom: Dimension::px(0),
                                    left: Dimension::px(7),
                                },
                                align: AlignItems::Center,
                                bg_color: ColorToken::SurfaceVariant
                            ) {
                                Text (
                                    "",
                                    grow: 1.0,
                                    height: ROW_HEIGHT,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnSurface,
                                    paragraph: bitmap_label(TextAlign::Start)
                                )
                                View (
                                    width: 4,
                                    height: 4,
                                    bg_color: ColorToken::Primary,
                                    border_radius: 2
                                )
                            }
                        }
                    }
                    Column (
                        position: Position::Absolute,
                        left: 0,
                        top: 0,
                        width: Dimension::percent(100),
                        height: Dimension::percent(100),
                        padding: Padding::all(6),
                        row_gap: 4,
                        bg_color: ColorToken::SurfaceVariant
                    ) [
                        TabContent {
                            tab_bar: id("compact_widgets_tabs"),
                            index: 1,
                        },
                    ] {
                        Row (height: 14, align: AlignItems::Center) {
                            Text (
                                "LIVE",
                                grow: 1.0,
                                height: 14,
                                font: FontToken::Default,
                                font_size: 6,
                                text_color: ColorToken::OnSurface,
                                paragraph: bitmap_label(TextAlign::Start)
                            )
                            Switch (id: "compact_widgets_switch", width: 28, height: 14, on: true)
                        }
                        Slider (
                            id: "compact_widgets_slider",
                            width: Dimension::percent(100),
                            height: 10,
                            min: Fixed::ZERO,
                            max: Fixed::from_int(100),
                            value: Fixed::from_int(62)
                        ) on ValueChanged {
                            progress_from_slider.set(*new / Fixed::from_int(100));
                        }
                        ProgressBar (
                            id: "compact_widgets_progress",
                            width: Dimension::percent(100),
                            height: 5,
                            border_radius: 2,
                            value: ${ progress.get().to_f32() }
                        )
                        Row (grow: 1.0, align: AlignItems::Center, column_gap: 5) {
                            Button (
                                size: ButtonSize::Custom,
                                grow: 1.0,
                                height: 18,
                                border_radius: 7,
                                normal_color: ColorToken::Primary,
                                pressed_color: ColorToken::Success,
                                text_color: ColorToken::OnPrimary
                            ) {
                                Text (
                                    "RUN",
                                    grow: 1.0,
                                    height: 18,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnPrimary,
                                    paragraph: bitmap_label(TextAlign::Center)
                                )
                            }
                            Button (
                                size: ButtonSize::Custom,
                                grow: 1.0,
                                height: 18,
                                border_radius: 7,
                                normal_color: ColorToken::Surface,
                                pressed_color: ColorToken::Primary,
                                text_color: ColorToken::OnSurface
                            ) {
                                Text (
                                    "RESET",
                                    grow: 1.0,
                                    height: 18,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnSurface,
                                    paragraph: bitmap_label(TextAlign::Center)
                                )
                            }
                        }
                    }
                    Column (
                        position: Position::Absolute,
                        left: 0,
                        top: 0,
                        width: Dimension::percent(100),
                        height: Dimension::percent(100),
                        padding: Padding::all(6),
                        row_gap: 4,
                        bg_color: ColorToken::SurfaceVariant
                    ) [
                        TabContent {
                            tab_bar: id("compact_widgets_tabs"),
                            index: 2,
                        },
                    ] {
                        Text (
                            "SEMANTIC TOKENS",
                            width: Dimension::percent(100),
                            height: 10,
                            font: FontToken::Default,
                            font_size: 6,
                            text_color: ColorToken::OnSurface,
                            paragraph: bitmap_label(TextAlign::Start)
                        )
                        Row (grow: 1.0, column_gap: 4) {
                            View (grow: 1.0, bg_color: ColorToken::Primary, border_radius: 5)
                            View (grow: 1.0, bg_color: ColorToken::Success, border_radius: 5)
                            View (grow: 1.0, bg_color: ColorToken::Error, border_radius: 5)
                        }
                        Row (width: Dimension::percent(100), height: 18, column_gap: 4) {
                            Button (
                                id: "compact_theme_light",
                                size: ButtonSize::Custom,
                                grow: 1.0,
                                height: 18,
                                border_radius: 6,
                                normal_color: ColorToken::Primary,
                                pressed_color: ColorToken::Success,
                                text_color: ColorToken::OnPrimary
                            ) [
                                CompactTheme(Theme::light()),
                            ] on Tap {
                                if let Some(theme) = ctx
                                    .world
                                    .get::<CompactTheme>(ctx.entity)
                                    .map(|choice| choice.0.clone())
                                {
                                    theme::set_theme(ctx.world, theme).unwrap();
                                }
                            }
                            {
                                Text (
                                    "LIGHT",
                                    grow: 1.0,
                                    height: 18,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnPrimary,
                                    paragraph: bitmap_label(TextAlign::Center)
                                )
                            }
                            Button (
                                id: "compact_theme_dark",
                                size: ButtonSize::Custom,
                                grow: 1.0,
                                height: 18,
                                border_radius: 6,
                                normal_color: ColorToken::Surface,
                                pressed_color: ColorToken::Primary,
                                text_color: ColorToken::OnSurface
                            ) [
                                CompactTheme(Theme::dark()),
                            ] on Tap {
                                if let Some(theme) = ctx
                                    .world
                                    .get::<CompactTheme>(ctx.entity)
                                    .map(|choice| choice.0.clone())
                                {
                                    theme::set_theme(ctx.world, theme).unwrap();
                                }
                            }
                            {
                                Text (
                                    "DARK",
                                    grow: 1.0,
                                    height: 18,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnSurface,
                                    paragraph: bitmap_label(TextAlign::Center)
                                )
                            }
                        }
                    }
                }
            }
        }
    };

    let list = cx
        .world_mut()
        .find_by_id("compact_widgets_list")
        .expect("compact list");
    let pool = cx
        .world_mut()
        .get::<Children>(list)
        .map(|children| children.0.clone())
        .unwrap_or_default();
    cx.world_mut().insert(list, LazyListPool::new(pool));
}
