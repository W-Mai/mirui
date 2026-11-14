use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    let items = [
        ("Item 0", ColorToken::Primary),
        ("Item 1", ColorToken::Secondary),
        ("Item 2", ColorToken::Tertiary),
        ("Item 3", ColorToken::OnSurfaceVariant),
        ("Item 4", ColorToken::Primary),
        ("Item 5", ColorToken::Secondary),
        ("Item 6", ColorToken::Tertiary),
        ("Item 7", ColorToken::OnSurfaceVariant),
    ];

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
                "Elastic list",
                width: Dimension::percent(100),
                max_width: 420,
                height: 30,
                font_size: 20,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Column (
                width: Dimension::percent(100),
                max_width: 420,
                grow: 1.0,
                row_gap: 6,
                padding: Padding::all(6),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            ) [
                ScrollOffset {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                },
                ScrollConfig {
                    direction: ScrollAxis::Vertical,
                    elastic: true,
                    content_height: Fixed::from_int(470),
                    content_width: Fixed::ZERO,
                    transition: None,
                },
            ] {
                walk items.iter() with item {
                    Row (
                        bg_color: ColorToken::Outline,
                        height: 52,
                        border_radius: 12,
                        padding: Padding::all(16),
                        column_gap: 12,
                        align: AlignItems::Center
                    ) {
                        View (width: 8, height: 8, border_radius: 4, bg_color: item.1)
                        Text (
                            item.0,
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
    app.compose(parent, build_widgets());
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);

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
