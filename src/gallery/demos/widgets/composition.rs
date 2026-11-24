use super::binding::create_row;
use super::state::{ACCENT, FormProgress, FormSlider};
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::OffscreenRender;
use crate::ui::widgets::{
    Button, Checkbox, Image, LazyList, ParagraphStyle, ProgressBar, Slider, Switch, TabBar,
    TabContent, Text, TextAlign,
};

const ITEM_COUNT: u32 = 50;
const FORM_BUTTON: StylePreset = style!(grow: 1.0, height: 40, border_radius: 20);

fn tab_panel() -> StylePreset {
    style!(
        position: Position::Absolute, left: 0, top: 0,
        width: Dimension::percent(100), height: Dimension::percent(100),
        bg_color: ColorToken::SurfaceVariant
    )
}

#[compose]
pub fn build_widgets() {
    const ROW_HEIGHT: i32 = 38;
    //~focus-start
    let tab_label = style!(
        grow: 1.0, height: Dimension::percent(100), text_color: ColorToken::OnSurface,
        paragraph: ParagraphStyle::label()
    );
    ui! {
        Column (
            bg_color: ColorToken::Surface,
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 10
        ) {
            Column (
                width: Dimension::percent(100),
                height: 48,
                row_gap: 2
            ) {
                Text (
                    "Widget workbench",
                    width: Dimension::percent(100),
                    height: 28,
                    font_size: 20,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "List, controls and live theme",
                    width: Dimension::percent(100),
                    height: 18,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
            TabBar (
                id: "widgets_tabs",
                width: Dimension::percent(100),
                height: 40,
                count: 3,
                indicator_height: Fixed::from_int(3),
                font_size: 14
            ) {
                Text (
                    "List",
                    id: "widgets_tab_list",
                    style: tab_label.clone()
                )
                Text (
                    "Controls",
                    id: "widgets_tab_form",
                    style: tab_label.clone()
                )
                Text (
                    "Theme",
                    id: "widgets_tab_theme",
                    style: tab_label
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
                    style: tab_panel(),
                    items: ITEM_COUNT,
                    item_extent: Fixed::from_int(ROW_HEIGHT)
                ) [
                    TabContent {
                        tab_bar: id("widgets_tabs"),
                        index: 0,
                    },
                    ScrollOffset {
                        x: Fixed::ZERO,
                        y: Fixed::ZERO,
                    },
                    ScrollConfig {
                        direction: ScrollAxis::Vertical,
                        elastic: false,
                        ..Default::default()
                    },
                ] on CreateCell(create_row::callback())
                Column (
                    style: tab_panel(),
                    padding: Padding::all(18),
                    row_gap: 12
                ) [
                    TabContent {
                        tab_bar: id("widgets_tabs"),
                        index: 1,
                    },
                ] {
                    Row (height: 34, align: AlignItems::Center) {
                        Text (
                            "Live controls",
                            grow: 1.0,
                            height: 24,
                            font_size: 16,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Switch (
                            id: "widgets_switch",
                            width: 44,
                            height: 24,
                            off_color: ColorToken::Outline,
                            thumb_color: Color::rgb(245, 246, 248)
                        ) [
                            OffscreenRender::default(),
                        ]
                    }
                    Text (
                        "Intensity",
                        width: Dimension::percent(100),
                        height: 16,
                        font_size: 12,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Slider (
                        id: "widgets_slider",
                        width: Dimension::percent(100),
                        height: 24,
                        min: Fixed::ZERO,
                        max: Fixed::from_int(100),
                        track_color: ColorToken::Outline,
                        thumb_color: Color::rgb(245, 246, 248)
                    ) [
                        FormSlider,
                    ]
                    ProgressBar (
                        width: Dimension::percent(100),
                        height: 8,
                        border_radius: 4,
                        track_color: ColorToken::Outline
                    ) [
                        FormProgress,
                    ]
                    Row (
                        height: 44,
                        align: AlignItems::Center,
                        column_gap: 10,
                        font_size: 15
                    ) {
                        Image (width: 28, height: 28, src: "thumbs_up")
                        Button (
                            "Apply",
                            style: FORM_BUTTON,
                            normal_color: ColorToken::Primary,
                            pressed_color: ColorToken::Secondary,
                            text_color: ColorToken::OnPrimary
                        )
                        Button (
                            "Reset",
                            style: FORM_BUTTON,
                            normal_color: ColorToken::Surface,
                            pressed_color: ColorToken::Outline,
                            text_color: ColorToken::OnSurface
                        )
                    }
                    Row (height: 32, align: AlignItems::Center, column_gap: 10) {
                        Text (
                            "Options",
                            grow: 1.0,
                            height: 24,
                            font_size: 14,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Checkbox (
                            width: 24,
                            height: 24,
                            border_radius: 8,
                            checked: true,
                            checked_color: ColorToken::Primary,
                            unchecked_color: ColorToken::Outline
                        )
                        Checkbox (
                            width: 24,
                            height: 24,
                            border_radius: 8,
                            checked_color: ColorToken::Primary,
                            unchecked_color: ColorToken::Outline
                        )
                    }
                }
                Column (
                    style: tab_panel(),
                    padding: Padding::all(18),
                    row_gap: 8
                ) [
                    TabContent {
                        tab_bar: id("widgets_tabs"),
                        index: 2,
                    },
                ] {
                    Text (
                        "Semantic palette",
                        width: Dimension::percent(100),
                        height: 28,
                        font_size: 18,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "Primary",
                        width: Dimension::percent(100),
                        height: 16,
                        font_size: 12,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (
                        width: Dimension::percent(100),
                        height: 44,
                        bg_color: ColorToken::Primary,
                        border_radius: 22
                    )
                    Text (
                        "Custom accent",
                        width: Dimension::percent(100),
                        height: 16,
                        font_size: 12,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (
                        width: Dimension::percent(100),
                        height: 44,
                        bg_color: ACCENT,
                        border_radius: 22
                    )
                    Text (
                        "Palette tokens update every three seconds.",
                        width: Dimension::percent(100),
                        height: 20,
                        font_size: 12,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
        }
    };
    //~focus-end
}
