use mirui::app::App;
use mirui::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::layout::*;
use mirui::plugins::InputFeedbackPlugin;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn main() {
    let backend = SdlSurface::new("mirui - nested scroll", 480, 400);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();
    app.add_plugin(InputFeedbackPlugin::new());

    let root = WidgetBuilder::new(&mut app.world)
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

    // Outer vertical scroll (5 sections × 200px = 1000px content)
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        outer_scroll (
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
                section (
                    direction: FlexDirection::Column,
                    height: 200,
                    bg_color: *item.1,
                    border_radius: 6
                ) {
                    label (height: 30, text: "Section", bg_color: Color::rgb(40, 40, 60)) {}
                    inner_scroll_h (direction: FlexDirection::Row, grow: 1.0) [
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
                            card (width: 100, height: 150, bg_color: *color, border_radius: 8) {}
                        }
                    }
                }
            }
            nested_v_section (
                direction: FlexDirection::Column,
                height: 300,
                bg_color: Color::rgb(50, 40, 70),
                border_radius: 6
            ) {
                nested_label (
                    height: 30,
                    text: "Nested V-Scroll",
                    bg_color: Color::rgb(60, 30, 80)
                ) {}
                inner_scroll_v (
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
                        vcard (height: 80, bg_color: *item.1, border_radius: 6, text: "V-Item") {}
                    }
                }
            }
        }
    };

    app.set_root(root);
    app.run();
}
