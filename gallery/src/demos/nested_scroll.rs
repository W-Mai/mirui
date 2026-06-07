use crate::Setup;
use mirui::ecs::Entity;
use mirui::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::plugins::{FpsSummaryPlugin, InputFeedbackPlugin, StdInstantClockPlugin};
use mirui::prelude::*;

pub const SIZE: (u16, u16) = (480, 400);

pub fn build(setup: &mut Setup<'_>) -> Entity {
    setup.app.add_plugin(InputFeedbackPlugin::new());
    setup.app.add_plugin(StdInstantClockPlugin);
    setup.app.add_plugin(FpsSummaryPlugin::default());

    let root = WidgetBuilder::new(&mut setup.app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(400),
            padding: Padding {
                top: Dimension::px(20),
                left: Dimension::px(20),
                right: Dimension::px(20),
                bottom: Dimension::px(20),
            },
            ..Default::default()
        })
        .id();

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

    let world = &mut setup.app.world;
    ui! {
        :(
            parent: root
            world: world
        :)

        View (
            direction: FlexDirection::Column,
            grow: 1.0,
            bg_color: Color::rgb(30, 30, 50)
        ) [
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: mirui::event::scroll::ScrollAxis::Vertical,
                elastic: true,
                content_height: Fixed::from_int(1200),
                content_width: Fixed::ZERO,
            },
        ] {
            walk colors_outer.iter().enumerate() with item {
                View (
                    direction: FlexDirection::Column,
                    height: 200,
                    bg_color: *item.1,
                    border_radius: 6
                ) {
                    View (height: 30, text: "Section", bg_color: Color::rgb(40, 40, 60)) {}
                    View (direction: FlexDirection::Row, grow: 1.0) [
                        ScrollOffset {
                            x: Fixed::ZERO,
                            y: Fixed::ZERO,
                        },
                        ScrollConfig {
                            direction: mirui::event::scroll::ScrollAxis::Horizontal,
                            elastic: true,
                            content_height: Fixed::ZERO,
                            content_width: Fixed::from_int(600),
                        },
                    ] {
                        walk colors_inner.iter() with color {
                            View (width: 100, height: 150, bg_color: *color, border_radius: 8) {}
                        }
                    }
                }
            }
            View (
                direction: FlexDirection::Column,
                height: 300,
                bg_color: Color::rgb(50, 40, 70),
                border_radius: 6
            ) {
                View (
                    height: 30,
                    text: "Nested V-Scroll",
                    bg_color: Color::rgb(60, 30, 80)
                ) {}
                View (
                    direction: FlexDirection::Column,
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
                        direction: mirui::event::scroll::ScrollAxis::Vertical,
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
                        ) {}
                    }
                }
            }
        }
    };

    root
}
