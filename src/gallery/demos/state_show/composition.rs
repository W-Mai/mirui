use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{Button, ProgressBar, Text};

#[compose]
pub fn build_widgets() {
    let shown = Signal::new(false);
    let toggle = shown.clone();
    let cond = shown.clone();

    let mode = Signal::new(0u8);
    let cycle = mode.clone();
    let sel = mode.clone();

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            padding: Padding::all(20),
            row_gap: 10,
            clip_children: true
        ) [
            ScrollOffset::default(),
            ScrollConfig::default(),
        ] {
            Button (
                font_size: 15,
                width: Dimension::percent(100),
                max_width: 220,
                height: 42,
                border_radius: 10,
                normal_color: ColorToken::Primary,
                pressed_color: ColorToken::Secondary,
                text_color: ColorToken::OnPrimary
            ) [
                Text::label("Toggle branch"),
            ] on Tap { toggle.update(|v| *v = !*v); }
            if $cond {
                View (
                    font_size: 17,
                    bg_color: ColorToken::Success,
                    width: Dimension::percent(100),
                    max_width: 260,
                    height: 58,
                    border_radius: 14,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("now you see me"),
                ]
            } else {
                View (
                    font_size: 17,
                    bg_color: ColorToken::SurfaceVariant,
                    width: Dimension::percent(100),
                    max_width: 260,
                    height: 58,
                    border_radius: 14,
                    text_color: ColorToken::OnSurfaceVariant
                ) [
                    Text::label("hidden, tap to show"),
                ]
            }
            Button (
                font_size: 15,
                width: Dimension::percent(100),
                max_width: 220,
                height: 42,
                border_radius: 10,
                normal_color: ColorToken::Outline,
                pressed_color: ColorToken::SurfaceVariant,
                text_color: ColorToken::OnSurface
            ) [
                Text::label("Cycle keyed layout"),
            ] on Tap { cycle.update(|m| *m = (*m + 1) % 3); }
            match $sel {
                0 => {
                    View (
                        font_size: 17,
                        width: Dimension::percent(100),
                        max_width: 260,
                        height: 60,
                        border_radius: 14,
                        bg_color: ColorToken::Primary,
                        text_color: ColorToken::OnPrimary
                    ) [
                        Text::label("single card"),
                    ]
                }
                1 => {
                    Row (
                        width: Dimension::percent(100),
                        max_width: 260,
                        height: 60,
                        justify: JustifyContent::SpaceBetween,
                        align: AlignItems::Center,
                        column_gap: 8
                    ) {
                        View (
                            grow: 1.0,
                            height: 60,
                            border_radius: 14,
                            bg_color: ColorToken::Error
                        )
                        View (
                            grow: 1.0,
                            height: 60,
                            border_radius: 14,
                            bg_color: ColorToken::Success
                        )
                        View (
                            grow: 1.0,
                            height: 60,
                            border_radius: 14,
                            bg_color: ColorToken::Primary
                        )
                    }
                }
                _ => {
                    Column (
                        width: Dimension::percent(100),
                        max_width: 260,
                        align: AlignItems::Stretch,
                        padding: Padding::all(8),
                        row_gap: 6,
                        bg_color: ColorToken::SurfaceVariant,
                        border_radius: 14
                    ) {
                        Text (
                            "stacked bars",
                            height: 24,
                            text_color: ColorToken::OnSurfaceVariant
                        )
                        ProgressBar (height: 12, border_radius: 6, value: 0.3)
                        ProgressBar (height: 12, border_radius: 6, value: 0.7)
                    }
                }
            }
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    crate::gallery::showcase_theme::install_sample(app);
    app.compose(parent, build_widgets());
}
