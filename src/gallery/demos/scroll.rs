use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    let colors = [
        ("Item 0", ColorToken::Primary, ColorToken::OnPrimary),
        ("Item 1", ColorToken::Secondary, ColorToken::OnSecondary),
        ("Item 2", ColorToken::Tertiary, ColorToken::OnTertiary),
        (
            "Item 3",
            ColorToken::SurfaceVariant,
            ColorToken::OnSurfaceVariant,
        ),
        ("Item 4", ColorToken::Primary, ColorToken::OnPrimary),
        ("Item 5", ColorToken::Secondary, ColorToken::OnSecondary),
        ("Item 6", ColorToken::Tertiary, ColorToken::OnTertiary),
        (
            "Item 7",
            ColorToken::SurfaceVariant,
            ColorToken::OnSurfaceVariant,
        ),
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
                "ELASTIC LIST",
                width: Dimension::percent(100),
                max_width: 420,
                height: 28,
                font_size: 18,
                text_color: ColorToken::OnSurface
            )
            Column (
                width: Dimension::percent(100),
                max_width: 420,
                grow: 1.0,
                row_gap: 8,
                padding: Padding::all(4),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 14
            ) [
                ScrollOffset {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                },
                ScrollConfig {
                    direction: ScrollAxis::Vertical,
                    elastic: true,
                    content_height: Fixed::from_int(536),
                    content_width: Fixed::ZERO,
                },
            ] {
                walk colors.iter() with item {
                    Row (
                        bg_color: item.1,
                        height: 60,
                        border_radius: 10,
                        padding: Padding {
                            top: Dimension::px(0),
                            right: Dimension::px(16),
                            bottom: Dimension::px(0),
                            left: Dimension::px(16),
                        },
                        align: AlignItems::Center
                    ) {
                        Text (
                            item.0,
                            grow: 1.0,
                            height: 24,
                            text_color: item.2,
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
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
