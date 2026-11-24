use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
#[cfg(feature = "std")]
use crate::prelude::plugin::InputFeedbackPlugin;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

fn section_style() -> StylePreset {
    style!(bg_color: ColorToken::SurfaceVariant, border_radius: 18, clip_children: true)
}

fn heading_style() -> StylePreset<Text> {
    style!(
        height: 34,
        font_size: 13,
        text_color: ColorToken::OnSurfaceVariant,
        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
    )
}

#[compose]
pub fn build_widgets() {
    let sections = [
        "Section 01",
        "Section 02",
        "Section 03",
        "Section 04",
        "Section 05",
    ];
    // A ramp of greys, so that a row of tiles reads as a row and not as five colours.
    let tiles = [
        ColorToken::Outline,
        ColorToken::Tertiary,
        ColorToken::Secondary,
        ColorToken::Outline,
        ColorToken::Tertiary,
        ColorToken::Secondary,
    ];
    let cards = [
        "Card 01", "Card 02", "Card 03", "Card 04", "Card 05", "Card 06",
    ];

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            bg_color: ColorToken::Surface,
            padding: Padding::all(16),
            row_gap: 12
        ) [
            ScrollOffset::default(),
            ScrollConfig {
                content_height: Fixed::from_int(1200),
                ..Default::default()
            },
        ] {
            Text (
                "Nested scroll",
                width: Dimension::percent(100),
                height: 30,
                font_size: 20,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            walk sections with title {
                Column (style: section_style(), height: 200) {
                    Text (title, style: heading_style())
                    Row (grow: 1.0, column_gap: 8) [
                        ScrollOffset::default(),
                        ScrollConfig {
                            direction: ScrollAxis::Horizontal,
                            content_width: Fixed::from_int(640),
                            ..Default::default()
                        },
                    ] {
                        walk tiles.iter() with tone {
                            View (width: 100, height: 150, bg_color: *tone, border_radius: 14)
                        }
                    }
                }
            }
            Column (style: section_style(), height: 300) {
                Text ("Nested vertical list", style: heading_style())
                Column (
                    grow: 1.0,
                    padding: Padding::all(6),
                    row_gap: 6
                ) [
                    ScrollOffset::default(),
                    ScrollConfig {
                        content_height: Fixed::from_int(500),
                        ..Default::default()
                    },
                ] {
                    walk cards with title {
                        Row (
                            height: 56,
                            bg_color: ColorToken::Outline,
                            border_radius: 12,
                            align: AlignItems::Center,
                            padding: Padding::all(16)
                        ) {
                            Text (
                                title,
                                grow: 1.0,
                                height: 24,
                                font_size: 15,
                                text_color: ColorToken::OnSurface,
                                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                            )
                        }
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
    crate::gallery::showcase_theme::install_sample(app);
    app.add_plugin(InputFeedbackPlugin::new());
    app.compose(parent, build_widgets());
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 400);

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
