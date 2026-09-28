use alloc::vec::Vec;

use super::binding::row_binder;
use super::state::{ACCENT, FormProgress, FormSlider};
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{
    Button, Checkbox, Image, LazyList, LazyListBinder, LazyListPool, ParagraphStyle, ProgressBar,
    Slider, Switch, TabBar, TabContent, Text, TextAlign,
};
use crate::ui::{Children, IdMap, OffscreenRender};

const POOL_SIZE: usize = 12;
const ITEM_COUNT: u32 = 50;

#[compose]
pub fn build_widgets(_view_w: u16, _view_h: u16) {
    const ROW_HEIGHT: i32 = 38;
    if cx.world_mut().resource::<IdMap>().is_none() {
        cx.world_mut().insert_resource(IdMap::new());
    }

    //~focus-start
    ui! {
        Column (
            bg_color: ColorToken::Surface,
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 12
        ) {
            Column (
                width: Dimension::percent(100),
                height: 50,
                row_gap: 2
            ) {
                Text (
                    "WIDGET WORKBENCH",
                    width: Dimension::percent(100),
                    height: 28,
                    font_size: 20,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "LIST / CONTROLS / LIVE THEME",
                    width: Dimension::percent(100),
                    height: 18,
                    font_size: 10,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
            TabBar (
                id: "widgets_tabs",
                width: Dimension::percent(100),
                height: 44,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 14,
                clip_children: true,
                count: 3,
                indicator_height: Fixed::from_int(3)
            ) {
                Text (
                    "LIST",
                    grow: 1.0,
                    height: Dimension::percent(100),
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "CONTROLS",
                    grow: 1.0,
                    height: Dimension::percent(100),
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "THEME",
                    grow: 1.0,
                    height: Dimension::percent(100),
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (
                width: Dimension::percent(100),
                grow: 1.0,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18,
                clip_children: true
            ) {
                LazyList (
                    id: "widgets_list",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    bg_color: ColorToken::SurfaceVariant,
                    item_count: ITEM_COUNT,
                    item_height: Fixed::from_int(ROW_HEIGHT),
                    pool_size: POOL_SIZE as u8
                ) [
                    TabContent {
                        tab_bar: id("widgets_tabs"),
                        index: 0,
                    },
                    LazyListBinder { bind: row_binder },
                    ScrollOffset {
                        x: Fixed::ZERO,
                        y: Fixed::ZERO,
                    },
                    ScrollConfig {
                        direction: ScrollAxis::Vertical,
                        elastic: false,
                        content_height: Fixed::from_int(ROW_HEIGHT * ITEM_COUNT as i32),
                        content_width: Fixed::ZERO,
                    },
                ] {
                    walk 0..POOL_SIZE with _i {
                        Row (
                            bg_color: ColorToken::Surface,
                            position: Position::Absolute,
                            left: 0,
                            top: 0,
                            width: Dimension::percent(100),
                            height: ROW_HEIGHT,
                            align: AlignItems::Center,
                            padding: Padding {
                                top: Dimension::px(0),
                                right: Dimension::px(14),
                                bottom: Dimension::px(0),
                                left: Dimension::px(14),
                            }
                        ) {
                            Text (
                                "",
                                grow: 1.0,
                                height: 24,
                                text_color: ColorToken::OnSurface,
                                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
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
                    padding: Padding::all(18),
                    row_gap: 14,
                    bg_color: ColorToken::SurfaceVariant
                ) [
                    TabContent {
                        tab_bar: id("widgets_tabs"),
                        index: 1,
                    },
                ] {
                    Row (height: 34, align: AlignItems::Center) {
                        Text (
                            "LIVE CONTROLS",
                            grow: 1.0,
                            height: 24,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Switch (width: 44, height: 24) [
                            OffscreenRender::default(),
                        ]
                    }
                    Text (
                        "INTENSITY",
                        width: Dimension::percent(100),
                        height: 18,
                        font_size: 10,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Slider (
                        width: Dimension::percent(100),
                        height: 20,
                        min: Fixed::ZERO,
                        max: Fixed::from_int(100)
                    ) [
                        FormSlider,
                    ]
                    ProgressBar (
                        width: Dimension::percent(100),
                        height: 8,
                        border_radius: 4
                    ) [
                        FormProgress,
                    ]
                    Row (
                        height: 42,
                        align: AlignItems::Center,
                        column_gap: 10
                    ) {
                        Image (width: 28, height: 28, src: "thumbs_up")
                        Button (
                            grow: 1.0,
                            height: 38,
                            border_radius: 12,
                            normal_color: ColorToken::Success,
                            pressed_color: ColorToken::Primary,
                            text_color: ColorToken::OnPrimary
                        ) [
                            Text::label("Apply"),
                        ]
                        Button (
                            grow: 1.0,
                            height: 38,
                            border_radius: 12,
                            normal_color: ColorToken::Surface,
                            pressed_color: ColorToken::Primary,
                            text_color: ColorToken::OnSurface
                        ) [
                            Text::label("Reset"),
                        ]
                    }
                    Row (height: 32, align: AlignItems::Center, column_gap: 10) {
                        Text (
                            "OPTIONS",
                            grow: 1.0,
                            height: 24,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Checkbox (
                            width: 22,
                            height: 22,
                            checked: true,
                            checked_color: ColorToken::Primary
                        )
                        Checkbox (
                            width: 22,
                            height: 22,
                            checked_color: ColorToken::Success
                        )
                    }
                }
                Column (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    padding: Padding::all(20),
                    row_gap: 12,
                    bg_color: ColorToken::SurfaceVariant
                ) [
                    TabContent {
                        tab_bar: id("widgets_tabs"),
                        index: 2,
                    },
                ] {
                    Text (
                        "SEMANTIC PALETTE",
                        width: Dimension::percent(100),
                        height: 28,
                        font_size: 18,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "PRIMARY",
                        width: Dimension::percent(100),
                        height: 18,
                        font_size: 10,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (
                        width: Dimension::percent(100),
                        height: 64,
                        bg_color: ColorToken::Primary,
                        border_radius: 14
                    )
                    Text (
                        "CUSTOM ACCENT",
                        width: Dimension::percent(100),
                        height: 18,
                        font_size: 10,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (
                        width: Dimension::percent(100),
                        height: 64,
                        bg_color: ACCENT,
                        border_radius: 14
                    )
                    Text (
                        "Palette tokens update every three seconds.",
                        width: Dimension::percent(100),
                        height: 24,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
        }
    };
    //~focus-end

    let list = cx
        .world_mut()
        .find_by_id("widgets_list")
        .expect("widgets list id");
    let pool: Vec<Entity> = cx
        .world_mut()
        .get::<Children>(list)
        .map(|children| children.0.clone())
        .unwrap_or_default();
    cx.world_mut().insert(list, LazyListPool::new(pool));
}
