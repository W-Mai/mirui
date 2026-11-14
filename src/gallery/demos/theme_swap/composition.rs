use super::state::{ACCENT, ThemeChoice};
use crate::prelude::*;
use crate::ui::widgets::{
    Button, Checkbox, ParagraphStyle, Placeholder, ProgressBar, Slider, Switch, TabBar, Text,
    TextAlign, TextInput,
};

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            padding: Padding::all(16),
            row_gap: 12,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "Live theme tokens",
                width: Dimension::percent(100),
                max_width: 480,
                height: 26,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                width: Dimension::percent(100),
                max_width: 480,
                height: 40,
                column_gap: 8
            ) {
                Button (
                    grow: 1.0,
                    height: 40,
                    border_radius: 20,
                    font_size: 14,
                    text_color: Color::rgb(242, 184, 64),
                    normal_color: Color::rgb(27, 28, 32),
                    pressed_color: Color::rgb(48, 50, 56)
                ) [
                    ThemeChoice(ThemeId::new("midnight-amber")),
                    Text::label("Graphite"),
                ] on Tap(|event: event!(Tap), rows: query!(ThemeChoice), commands: commands!()| {
                    let theme = rows.get(event.target).map(|choice| choice.0);
                    if let Some(theme) = theme {
                        commands.set_theme(theme);
                    }
                })
                Button (
                    grow: 1.0,
                    height: 40,
                    border_radius: 20,
                    font_size: 14,
                    text_color: Color::rgb(196, 56, 88),
                    normal_color: Color::rgb(238, 234, 235),
                    pressed_color: Color::rgb(222, 215, 217)
                ) [
                    ThemeChoice(ThemeId::new("paper-rose")),
                    Text::label("Paper"),
                ] on Tap(|event: event!(Tap), rows: query!(ThemeChoice), commands: commands!()| {
                    let theme = rows.get(event.target).map(|choice| choice.0);
                    if let Some(theme) = theme {
                        commands.set_theme(theme);
                    }
                })
                Button (
                    id: "theme_custom",
                    grow: 1.0,
                    height: 40,
                    border_radius: 20,
                    font_size: 14,
                    text_color: Color::rgb(64, 196, 220),
                    normal_color: Color::rgb(18, 34, 47),
                    pressed_color: Color::rgb(38, 60, 78)
                ) [
                    ThemeChoice(ThemeId::new("deep-sea")),
                    Text::label("Deep sea"),
                ] on Tap(|event: event!(Tap), rows: query!(ThemeChoice), commands: commands!()| {
                    let theme = rows.get(event.target).map(|choice| choice.0);
                    if let Some(theme) = theme {
                        commands.set_theme(theme);
                    }
                })
            }
            Row (
                id: "theme_token_grid",
                width: Dimension::percent(100),
                max_width: 480,
                grow: 1.0,
                max_height: 340,
                padding: Padding::all(12),
                wrap: FlexWrap::Wrap,
                align: AlignItems::FlexStart,
                row_gap: 4,
                column_gap: 16,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            ) {
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 10
                ) {
                    Text (
                        "Slider",
                        width: 64,
                        height: 38,
                        font_size: 13,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Slider (
                        min: Fixed::ZERO,
                        max: Fixed::from_int(100),
                        value: Fixed::from_int(40),
                        grow: 1.0,
                        height: 22,
                        track_color: ColorToken::Outline,
                        thumb_color: Color::rgb(245, 246, 248)
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 10
                ) {
                    Text (
                        "Switch",
                        width: 64,
                        height: 38,
                        font_size: 13,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Switch (
                        width: 44,
                        height: 24,
                        off_color: ColorToken::Outline,
                        thumb_color: Color::rgb(245, 246, 248)
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 10
                ) {
                    Text (
                        "Checkbox",
                        width: 64,
                        height: 38,
                        font_size: 13,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Checkbox (
                        width: 22,
                        height: 22,
                        border_radius: 7,
                        unchecked_color: ColorToken::Outline
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 10
                ) {
                    Text (
                        "Progress",
                        width: 64,
                        height: 38,
                        font_size: 13,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    ProgressBar (
                        grow: 1.0,
                        height: 10,
                        border_radius: 5,
                        value: 0.6,
                        track_color: ColorToken::Outline
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 10
                ) {
                    Text (
                        "Input",
                        width: 64,
                        height: 38,
                        font_size: 13,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    TextInput (
                        grow: 1.0,
                        height: 32,
                        font_size: 13,
                        border_radius: 16,
                        bg_color: ColorToken::Surface,
                        border_color: ColorToken::Outline,
                        border_width: 1
                    ) [
                        Placeholder("Theme-aware"),
                    ]
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 10
                ) {
                    Text (
                        "Tabs",
                        width: 64,
                        height: 38,
                        font_size: 13,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    TabBar (count: 3, grow: 1.0, height: 28) {
                        Text (
                            "A",
                            grow: 1.0,
                            font_size: 13,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label()
                        )
                        Text (
                            "B",
                            grow: 1.0,
                            font_size: 13,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label()
                        )
                        Text (
                            "C",
                            grow: 1.0,
                            font_size: 13,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label()
                        )
                    }
                }
                Row (
                    id: "theme_accent_row",
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 10
                ) {
                    Text (
                        "Accent",
                        width: 64,
                        height: 38,
                        font_size: 13,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (grow: 1.0, height: 22, border_radius: 11, bg_color: ACCENT)
                }
            }
        }
    };
    //~focus-end
}
