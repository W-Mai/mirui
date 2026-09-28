use super::state::{ACCENT, ThemeChoice};
use crate::prelude::*;
use crate::ui::theme;
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
            row_gap: 10,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "LIVE THEME TOKENS",
                width: Dimension::percent(100),
                max_width: 480,
                height: 24,
                font_size: 16,
                text_color: ColorToken::OnSurface
            )
            Row (
                width: Dimension::percent(100),
                max_width: 480,
                height: 36,
                column_gap: 8
            ) {
                Button (
                    grow: 1.0,
                    height: 36,
                    border_radius: 10,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(40, 50, 70),
                    pressed_color: Color::rgb(20, 25, 35)
                ) [
                    ThemeChoice(ThemeId::new("midnight-amber")),
                    Text::label("Dark"),
                ] on Tap {
                    if let Some(id) = ctx.world.get::<ThemeChoice>(ctx.entity).map(|choice| choice.0) {
                        let _ = theme::set_theme(ctx.world, id);
                    }
                }
                Button (
                    grow: 1.0,
                    height: 36,
                    border_radius: 10,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(0, 100, 200),
                    pressed_color: Color::rgb(0, 70, 150)
                ) [
                    ThemeChoice(ThemeId::new("paper-rose")),
                    Text::label("Light"),
                ] on Tap {
                    if let Some(id) = ctx.world.get::<ThemeChoice>(ctx.entity).map(|choice| choice.0) {
                        let _ = theme::set_theme(ctx.world, id);
                    }
                }
                Button (
                    id: "theme_custom",
                    grow: 1.0,
                    height: 36,
                    border_radius: 10,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(255, 105, 180),
                    pressed_color: Color::rgb(200, 70, 140)
                ) [
                    ThemeChoice(ThemeId::new("plum")),
                    Text::label("Custom"),
                ] on Tap {
                    if let Some(id) = ctx.world.get::<ThemeChoice>(ctx.entity).map(|choice| choice.0) {
                        let _ = theme::set_theme(ctx.world, id);
                    }
                }
            }
            Row (
                id: "theme_token_grid",
                width: Dimension::percent(100),
                max_width: 480,
                grow: 1.0,
                max_height: 340,
                padding: Padding::all(10),
                wrap: FlexWrap::Wrap,
                align: AlignItems::FlexStart,
                row_gap: 6,
                column_gap: 8,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 14
            ) {
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Slider",
                        width: 48,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Slider (
                        min: Fixed::ZERO,
                        max: Fixed::from_int(100),
                        grow: 1.0,
                        height: 20
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Switch",
                        width: 48,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Switch (
                        width: 44,
                        height: 22,
                        off_color: ColorToken::Outline
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Checkbox",
                        width: 68,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Checkbox (
                        width: 20,
                        height: 20,
                        border_radius: 4,
                        border_color: ColorToken::Outline,
                        border_width: 1
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Progress",
                        width: 68,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    ProgressBar (grow: 1.0, height: 10, border_radius: 5, value: 0.6)
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Input",
                        width: 44,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    TextInput (
                        grow: 1.0,
                        height: 28,
                        border_radius: 8,
                        bg_color: ColorToken::Surface,
                        border_color: ColorToken::Outline,
                        border_width: 1
                    ) [
                        Placeholder("Theme-aware input"),
                    ]
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Tabs",
                        width: 32,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    TabBar (count: 3, grow: 1.0, height: 24) {
                        Text ("A", grow: 1.0, paragraph: ParagraphStyle::label())
                        Text ("B", grow: 1.0, paragraph: ParagraphStyle::label())
                        Text ("C", grow: 1.0, paragraph: ParagraphStyle::label())
                    }
                }
                Row (
                    id: "theme_accent_row",
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Accent",
                        width: 48,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (grow: 1.0, height: 22, border_radius: 6, bg_color: ACCENT)
                }
            }
        }
    };
    //~focus-end
}
