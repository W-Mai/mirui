use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    let colors = [
        ("Item 0", Color::rgb(88, 166, 255)),
        ("Item 1", Color::rgb(63, 185, 80)),
        ("Item 2", Color::rgb(248, 81, 73)),
        ("Item 3", Color::rgb(210, 168, 255)),
        ("Item 4", Color::rgb(255, 200, 50)),
        ("Item 5", Color::rgb(150, 100, 200)),
        ("Item 6", Color::rgb(100, 200, 150)),
        ("Item 7", Color::rgb(200, 100, 100)),
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
                            text_color: Color::rgb(255, 255, 255),
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
