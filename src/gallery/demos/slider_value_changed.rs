#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Slider, Text};

use alloc::format;

#[derive(Clone, Copy, Default)]
struct Stats {
    pub last_value: i32,
    pub changes: u32,
    pub drags: u32,
}

/// Slider 0..100 with `on ValueChanged / DragStarted / DragEnded` callbacks.
///
/// # Required plugins
/// - [`StdInstantClockPlugin`] — gesture timing
///
#[compose]
pub fn build_widgets() {
    let stats = Signal::new(Stats::default());
    let (s_value_text, s_read, s_value, s_drag_started, s_drag_ended) = (
        stats.clone(),
        stats.clone(),
        stats.clone(),
        stats.clone(),
        stats.clone(),
    );

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 8
        ) {
            Text (
                "Value",
                width: Dimension::percent(100),
                max_width: 480,
                height: 16,
                font_size: 12,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
            Text (
                text: ${ format!("{}", s_value_text.get().last_value) },
                width: Dimension::percent(100),
                max_width: 480,
                height: 70,
                font_size: 56,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Slider (
                width: Dimension::percent(100),
                max_width: 480,
                height: 32,
                min: Fixed::ZERO,
                max: Fixed::from_int(100),
                value: Fixed::ZERO,
                track_color: ColorToken::Outline,
                thumb_color: Color::rgb(245, 246, 248)
            ) on ValueChanged {
                let new_value = event.new.to_int();
                let _ = event.old;
                s_value
                    .update(|s| {
                        s.last_value = new_value;
                        s.changes += 1;
                    });
            } on DragStarted {
                s_drag_started
                    .update(|s| {
                        s.drags += 1;
                    });
            } on DragEnded {
                s_drag_ended
                    .update(|s| {
                        s.drags += 1;
                    });
            }
            Text (
                text: ${ format!("{} changes · {} drag edges", s_read.get().changes, s_read.get().drags) },
                id: "stats_label",
                width: Dimension::percent(100),
                max_width: 480,
                height: 24,
                font_size: 12,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
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
    app.add_plugin(StdInstantClockPlugin);
    crate::gallery::showcase_theme::install_sample(app);
    app.compose(parent, build_widgets());
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(720, 320);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;

    #[test]
    fn build_widgets_smoke() {
        let mut app = App::headless(64, 64);
        let parent = app.spawn_root().id();
        app.compose(parent, build_widgets());
        let world = &app.world;
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty())
        );
        // The reactive Text's first run must seed real content at build time,
        // not leave the label empty until the first event.
        let label = world.find_by_id("stats_label").expect("stats_label exists");
        let text = world.get::<Text>(label).expect("label has Text");
        assert!(
            !text.resolve(world.resource()).is_empty(),
            "reactive Text shows its initial value"
        );
    }
}
