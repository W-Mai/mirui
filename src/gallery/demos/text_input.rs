extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
use crate::prelude::*;
use crate::ui::widgets::{Placeholder, Text, TextInput};

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 10
        ) {
            Text (
                "Text input",
                width: Dimension::percent(100),
                max_width: 420,
                height: 30,
                font_size: 20,
                text_color: ColorToken::OnSurface
            )
            Text (
                "A field keeps to one line. A text area wraps, scrolls, and takes Return as a line break.",
                width: Dimension::percent(100),
                max_width: 420,
                height: 40,
                font_size: 13,
                text_color: ColorToken::OnSurfaceVariant
            )
            TextInput (
                capacity: 256,
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 22,
                font_size: 15,
                width: Dimension::percent(100),
                max_width: 420,
                height: 44
            ) [
                Placeholder("Name"),
            ]
            TextInput (
                capacity: 1024,
                lines: 4,
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 18,
                font_size: 15,
                padding: Padding::all(14),
                width: Dimension::percent(100),
                max_width: 420
            ) [
                Placeholder("Notes"),
            ]
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
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    crate::gallery::showcase_theme::install_sample(app);
    app.compose(parent, build_widgets());
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 340);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;

    #[test]
    fn build_widgets_smoke() {
        let mut app = App::headless(64, 64);
        let parent = app.spawn_root().id();
        app.compose(parent, build_widgets());
        assert!(
            app.world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
