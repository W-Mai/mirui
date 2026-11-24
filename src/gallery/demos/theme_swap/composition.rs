use super::state::{ACCENT, ThemeChoice};
use crate::prelude::*;
use crate::ui::widgets::{
    Button, Checkbox, ParagraphStyle, Placeholder, ProgressBar, Slider, Switch, TabBar, Text,
    TextAlign, TextInput,
};

const THEME_BUTTON: StylePreset = style!(grow: 1.0, height: 40, border_radius: 20);
const CONTROL_ROW: StylePreset = style!(
    width: 200, grow: 1.0, height: 38, align: AlignItems::Center, column_gap: 10
);

#[event]
fn choose_theme(event: event!(Tap), choices: query!(ThemeChoice), commands: commands!()) {
    if let Some(theme) = choices.get(event.target) {
        commands.set_theme(theme.0);
    }
}

#[compose]
pub fn build_widgets() {
    //~focus-start
    let control_label = style!(
        width: 64, height: 38, text_color: ColorToken::OnSurfaceVariant,
        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
    );
    let tab_label =
        style!(grow: 1.0, text_color: ColorToken::OnSurface, paragraph: ParagraphStyle::label());
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
                column_gap: 8,
                font_size: 14
            ) {
                Button (
                    text: "Graphite",
                    style: THEME_BUTTON,
                    text_color: Color::rgb(242, 184, 64),
                    normal_color: Color::rgb(27, 28, 32),
                    pressed_color: Color::rgb(48, 50, 56)
                ) [
                    ThemeChoice(ThemeId::new("midnight-amber")),
                ] on Tap(choose_theme::callback())
                Button (
                    text: "Paper",
                    style: THEME_BUTTON,
                    text_color: Color::rgb(196, 56, 88),
                    normal_color: Color::rgb(238, 234, 235),
                    pressed_color: Color::rgb(222, 215, 217)
                ) [
                    ThemeChoice(ThemeId::new("paper-rose")),
                ] on Tap(choose_theme::callback())
                Button (
                    id: "theme_custom",
                    text: "Deep sea",
                    style: THEME_BUTTON,
                    text_color: Color::rgb(64, 196, 220),
                    normal_color: Color::rgb(18, 34, 47),
                    pressed_color: Color::rgb(38, 60, 78)
                ) [
                    ThemeChoice(ThemeId::new("deep-sea")),
                ] on Tap(choose_theme::callback())
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
                border_radius: 18,
                font_size: 13
            ) {
                Row (style: CONTROL_ROW) {
                    Text (
                        "Slider",
                        style: control_label.clone()
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
                Row (style: CONTROL_ROW) {
                    Text (
                        "Switch",
                        style: control_label.clone()
                    )
                    Switch (
                        width: 44,
                        height: 24,
                        off_color: ColorToken::Outline,
                        thumb_color: Color::rgb(245, 246, 248)
                    )
                }
                Row (style: CONTROL_ROW) {
                    Text (
                        "Checkbox",
                        style: control_label.clone()
                    )
                    Checkbox (
                        width: 22,
                        height: 22,
                        border_radius: 7,
                        unchecked_color: ColorToken::Outline
                    )
                }
                Row (style: CONTROL_ROW) {
                    Text (
                        "Progress",
                        style: control_label.clone()
                    )
                    ProgressBar (
                        grow: 1.0,
                        height: 10,
                        border_radius: 5,
                        value: 0.6,
                        track_color: ColorToken::Outline
                    )
                }
                Row (style: CONTROL_ROW) {
                    Text (
                        "Input",
                        style: control_label.clone()
                    )
                    TextInput (
                        grow: 1.0,
                        height: 32,
                        border_radius: 16,
                        bg_color: ColorToken::Surface,
                        border_color: ColorToken::Outline,
                        border_width: 1
                    ) [
                        Placeholder("Theme-aware"),
                    ]
                }
                Row (style: CONTROL_ROW) {
                    Text (
                        "Tabs",
                        style: control_label.clone()
                    )
                    TabBar (count: 3, grow: 1.0, height: 28) {
                        Text (
                            "A",
                            style: tab_label.clone()
                        )
                        Text (
                            "B",
                            style: tab_label.clone()
                        )
                        Text (
                            "C",
                            style: tab_label
                        )
                    }
                }
                Row (
                    id: "theme_accent_row",
                    style: CONTROL_ROW
                ) {
                    Text (
                        "Accent",
                        style: control_label
                    )
                    View (grow: 1.0, height: 22, border_radius: 11, bg_color: ACCENT)
                }
            }
        }
    };
    //~focus-end
}
