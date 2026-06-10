#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
#[cfg(feature = "std")]
use crate::plugins::input_feedback::InputFeedbackPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

pub fn build_widgets(world: &mut World, parent: Entity) {
    let colors_outer = [
        Color::rgb(60, 60, 90),
        Color::rgb(70, 50, 80),
        Color::rgb(50, 70, 80),
        Color::rgb(80, 60, 60),
        Color::rgb(60, 80, 60),
    ];

    let colors_inner = [
        Color::rgb(88, 166, 255),
        Color::rgb(63, 185, 80),
        Color::rgb(248, 81, 73),
        Color::rgb(210, 168, 255),
        Color::rgb(255, 200, 50),
        Color::rgb(100, 200, 150),
    ];

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            grow: 1.0,
            bg_color: Color::rgb(30, 30, 50),
            padding: Padding {
                top: Dimension::px(20),
                left: Dimension::px(20),
                right: Dimension::px(20),
                bottom: Dimension::px(20),
            }
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
            walk colors_outer.iter().enumerate() with item {
                Column (
                    height: 200,
                    bg_color: *item.1,
                    border_radius: 6
                ) {
                    View (height: 30, text: "Section", bg_color: Color::rgb(40, 40, 60))
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
                            View (width: 100, height: 150, bg_color: *color, border_radius: 8)
                        }
                    }
                }
            }
            Column (
                height: 300,
                bg_color: Color::rgb(50, 40, 70),
                border_radius: 6
            ) {
                View (
                    height: 30,
                    text: "Nested V-Scroll",
                    bg_color: Color::rgb(60, 30, 80)
                )
                Column (
                    grow: 1.0,
                    bg_color: Color::rgb(40, 35, 60),
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
                        View (
                            height: 80,
                            bg_color: *item.1,
                            border_radius: 6,
                            text: "V-Item"
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
    app.add_plugin(InputFeedbackPlugin::new());
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Children;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
