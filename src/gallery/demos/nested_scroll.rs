use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
#[cfg(feature = "std")]
use crate::prelude::plugin::InputFeedbackPlugin;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    let colors_outer = [
        (ColorToken::Primary, ColorToken::OnPrimary),
        (ColorToken::Secondary, ColorToken::OnSecondary),
        (ColorToken::Tertiary, ColorToken::OnTertiary),
        (ColorToken::SurfaceVariant, ColorToken::OnSurfaceVariant),
        (ColorToken::Primary, ColorToken::OnPrimary),
    ];

    let colors_inner = [
        (ColorToken::Primary, ColorToken::OnPrimary),
        (ColorToken::Secondary, ColorToken::OnSecondary),
        (ColorToken::Tertiary, ColorToken::OnTertiary),
        (ColorToken::SurfaceVariant, ColorToken::OnSurfaceVariant),
        (ColorToken::Secondary, ColorToken::OnSecondary),
        (ColorToken::Primary, ColorToken::OnPrimary),
    ];

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            bg_color: ColorToken::Surface,
            padding: Padding::all(16),
            row_gap: 12
        ) [
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: true,
                content_height: Fixed::from_int(1200),
                content_width: Fixed::ZERO,
            },
        ] {
            Text (
                "NESTED SCROLL",
                width: Dimension::percent(100),
                height: 30,
                font_size: 20,
                text_color: ColorToken::OnSurface
            )
            walk colors_outer.iter().enumerate() with item {
                Column (
                    height: 200,
                    bg_color: item.1.0,
                    border_radius: 12,
                    clip_children: true
                ) {
                    Text (
                        text: ${ alloc::format!("SECTION {:02}", item.0 + 1) },
                        height: 34,
                        text_color: item.1.1,
                        paragraph: ParagraphStyle::label()
                    )
                    Row (grow: 1.0) [
                        ScrollOffset {
                            x: Fixed::ZERO,
                            y: Fixed::ZERO,
                        },
                        ScrollConfig {
                            direction: ScrollAxis::Horizontal,
                            elastic: true,
                            content_height: Fixed::ZERO,
                            content_width: Fixed::from_int(600),
                        },
                    ] {
                        walk colors_inner.iter() with color {
                            View (width: 100, height: 150, bg_color: color.0, border_radius: 8)
                        }
                    }
                }
            }
            Column (
                height: 300,
                bg_color: ColorToken::Primary,
                border_radius: 12,
                clip_children: true
            ) {
                Text (
                    "NESTED VERTICAL LIST",
                    height: 30,
                    text_color: ColorToken::OnPrimary,
                    paragraph: ParagraphStyle::label()
                )
                Column (
                    grow: 1.0,
                    bg_color: ColorToken::SurfaceVariant,
                    padding: Padding {
                        top: Dimension::px(8),
                        left: Dimension::px(8),
                        right: Dimension::px(8),
                        bottom: Dimension::px(8),
                    }
                ) [
                    ScrollOffset {
                        x: Fixed::ZERO,
                        y: Fixed::ZERO,
                    },
                    ScrollConfig {
                        direction: ScrollAxis::Vertical,
                        elastic: true,
                        content_height: Fixed::from_int(500),
                        content_width: Fixed::ZERO,
                    },
                ] {
                    walk colors_inner.iter().enumerate() with item {
                        Row (
                            height: 80,
                            bg_color: item.1.0,
                            border_radius: 6,
                            align: AlignItems::Center,
                            padding: Padding::all(12)
                        ) {
                            Text (
                                text: ${ alloc::format!("CARD {:02}", item.0 + 1) },
                                grow: 1.0,
                                height: 24,
                                text_color: item.1.1,
                                paragraph: ParagraphStyle::label()
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
    app.add_plugin(InputFeedbackPlugin::new());
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 400);

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
