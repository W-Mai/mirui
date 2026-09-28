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
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 10
        ) {
            Button (
                width: Dimension::percent(100),
                max_width: 220,
                height: 42,
                border_radius: 11,
                normal_color: ColorToken::Primary,
                pressed_color: ColorToken::Secondary,
                text_color: ColorToken::OnPrimary
            ) [
                Text::label("Toggle branch"),
            ] on Tap { toggle.update(|v| *v = !*v); }
            if $cond {
                View (
                    bg_color: ColorToken::Success,
                    width: Dimension::percent(100),
                    max_width: 260,
                    height: 58,
                    border_radius: 12,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("now you see me"),
                ]
            } else {
                View (
                    bg_color: ColorToken::SurfaceVariant,
                    width: Dimension::percent(100),
                    max_width: 260,
                    height: 58,
                    border_radius: 12,
                    text_color: ColorToken::OnSurfaceVariant
                ) [
                    Text::label("hidden — tap to show"),
                ]
            }
            Button (
                width: Dimension::percent(100),
                max_width: 220,
                height: 42,
                border_radius: 11,
                normal_color: ColorToken::Secondary,
                pressed_color: ColorToken::Primary,
                text_color: ColorToken::OnSecondary
            ) [
                Text::label("Cycle keyed layout"),
            ] on Tap { cycle.update(|m| *m = (*m + 1) % 3); }
            match $sel {
                0 => {
                    View (
                        width: Dimension::percent(100),
                        max_width: 260,
                        height: 60,
                        border_radius: 12,
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
                            border_radius: 12,
                            bg_color: ColorToken::Error
                        )
                        View (
                            grow: 1.0,
                            height: 60,
                            border_radius: 12,
                            bg_color: ColorToken::Success
                        )
                        View (
                            grow: 1.0,
                            height: 60,
                            border_radius: 12,
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
                        border_radius: 12
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
    app.compose(parent, build_widgets);
}
