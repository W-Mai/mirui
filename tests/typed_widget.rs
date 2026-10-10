#[cfg(test)]
mod tests {
    use mirui::app::App;
    use mirui::ui;
    use mirui::ui::widgets::{Button, Checkbox};

    #[test]
    fn capital_name_inserts_component() {
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Button (normal_color: mirui::ui::ColorToken::Surface)
            }
        });
        let world = &app.world;

        let entities = world.query::<Button>().collect();
        assert_eq!(entities.len(), 1, "exactly one Button entity");
        let btn = world.get::<Button>(entities[0]).unwrap();
        assert!(matches!(
            btn.normal_color,
            mirui::ui::ThemedColor::Token(mirui::ui::ColorToken::Surface)
        ));
    }

    #[test]
    fn button_accepts_positional_and_named_labels() {
        use mirui::ui::widgets::Text;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Row () {
                    Button ("Save")
                    Button (text: "Cancel")
                }
            }
        });
        let world = &app.world;

        let entities = world.query::<Text>().collect();
        let labels: Vec<_> = entities
            .into_iter()
            .map(|entity| world.get::<Text>(entity).unwrap().resolve(world.resource()))
            .collect();
        assert_eq!(labels, ["Save", "Cancel"]);
    }

    #[test]
    fn capital_name_uses_default_for_omitted_fields() {
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Checkbox ()
            }
        });
        let world = &app.world;

        let entities = world.query::<Checkbox>().collect();
        assert_eq!(entities.len(), 1);
        let cb = world.get::<Checkbox>(entities[0]).unwrap();
        assert!(!cb.checked);
    }

    #[test]
    fn text_widget_uses_tuple_init() {
        use mirui::ui::widgets::Text;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Text (text: "Hello")
            }
        });
        let world = &app.world;

        let entities = world.query::<Text>().collect();
        assert_eq!(entities.len(), 1);
        let text = world.get::<Text>(entities[0]).unwrap();
        assert_eq!(text.resolve(world.resource()), "Hello");
    }

    #[test]
    fn text_widget_routes_font_size_to_style() {
        use mirui::ui::Style;
        use mirui::ui::widgets::Text;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Text (
                    "Hello",
                    width: mirui::types::Dimension::Content,
                    height: mirui::types::Dimension::Auto,
                    font_size: 18
                )
            }
        });
        let world = &app.world;

        let entity = world.query::<Text>().collect()[0];
        let style = world.get::<Style>(entity).unwrap();
        assert_eq!(style.font_size, Some(18));
        assert_eq!(style.layout.width, mirui::types::Dimension::Content);
        assert_eq!(style.layout.height, mirui::types::Dimension::Auto);
    }

    #[test]
    fn text_widget_routes_font_stack_and_paragraph() {
        use mirui::render::font::{FontStack, FontToken};
        use mirui::ui::Style;
        use mirui::ui::widgets::{ParagraphStyle, Text, TextDirection, TextWrap};

        const PRIMARY: FontToken = FontToken::Custom("primary");
        static FALLBACKS: [FontToken; 2] = [
            FontToken::Custom("fallback-cjk"),
            FontToken::Custom("fallback-arabic"),
        ];

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let paragraph = ParagraphStyle {
            wrap: TextWrap::NoWrap,
            direction: TextDirection::RightToLeft,
            max_lines: Some(1),
            ..ParagraphStyle::default()
        };
        let expected_paragraph = paragraph.clone();

        app.compose(root, |cx| {
            ui! {
                Text (
                    "مرحبا",
                    font_stack: FontStack::new(PRIMARY).with_fallbacks(&FALLBACKS[..]),
                    paragraph: paragraph
                )
            }
        });
        let world = &app.world;

        let entity = world.query::<Text>().collect()[0];
        let style = world.get::<Style>(entity).unwrap();
        let text = world.get::<Text>(entity).unwrap();
        assert_eq!(style.font_stack.primary(), &PRIMARY);
        assert_eq!(style.font_stack.fallbacks(), &FALLBACKS);
        assert_eq!(text.paragraph(), &expected_paragraph);
    }

    #[test]
    fn text_widget_reacts_to_paragraph_signal() {
        use mirui::core::reactive::{Signal, flush_signal_dirty};
        use mirui::ui::widgets::{ParagraphStyle, Text, TextAlign, TextOverflow, TextWrap};

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let paragraph = Signal::new(ParagraphStyle::default());
        let paragraph_view = paragraph.clone();

        app.compose(root, |cx| {
            ui! {
                Text (
                    "responsive text",
                    paragraph: ${ paragraph_view.get() }
                )
            }
        });
        let world = &mut app.world;

        let entity = world.query::<Text>().collect()[0];
        paragraph.set(ParagraphStyle {
            wrap: TextWrap::Grapheme,
            align: TextAlign::End,
            overflow: TextOverflow::Ellipsis,
            max_lines: Some(2),
            ..ParagraphStyle::default()
        });
        flush_signal_dirty(world);

        let actual = world.get::<Text>(entity).unwrap().paragraph();
        assert_eq!(actual.wrap, TextWrap::Grapheme);
        assert_eq!(actual.align, TextAlign::End);
        assert_eq!(actual.overflow, TextOverflow::Ellipsis);
        assert_eq!(actual.max_lines, Some(2));
    }

    #[test]
    fn text_input_text_color_routes_to_field() {
        use mirui::ui::widgets::TextInput;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                TextInput (text_color: mirui::ui::ColorToken::Primary)
            }
        });
        let world = &app.world;

        let entities = world.query::<TextInput>().collect();
        assert_eq!(entities.len(), 1);
        let ti = world.get::<TextInput>(entities[0]).unwrap();
        assert!(matches!(
            ti.text_color,
            mirui::ui::ThemedColor::Token(mirui::ui::ColorToken::Primary)
        ));
    }

    #[test]
    fn text_widget_supports_positional_arg() {
        use mirui::ui::widgets::Text;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Text ("Positional")
            }
        });
        let world = &app.world;

        let entities = world.query::<Text>().collect();
        assert_eq!(entities.len(), 1);
        let text = world.get::<Text>(entities[0]).unwrap();
        assert_eq!(text.resolve(world.resource()), "Positional");
    }

    #[test]
    fn image_accepts_a_borrowed_vector_path_positionally() {
        use mirui::render::path::{Path, PathCmd};
        use mirui::types::Point;
        use mirui::ui::widgets::{Image, ImageSource};

        static ICON: &[PathCmd] = &[
            PathCmd::MoveTo(Point::ZERO),
            PathCmd::LineTo(Point {
                x: mirui::types::Fixed::from_int(24),
                y: mirui::types::Fixed::from_int(24),
            }),
        ];

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let icon = Path::from_static(ICON);

        app.compose(root, |cx| {
            ui! {
                Image (icon)
            }
        });
        let world = &app.world;

        let entity = world.query::<Image>().collect()[0];
        assert!(matches!(
            &world.get::<Image>(entity).unwrap().src,
            ImageSource::Vector(path) if path.commands().as_ptr() == ICON.as_ptr()
        ));
    }

    #[test]
    fn typed_icon_assets_feed_image_and_icon_widgets() {
        use mirui::ui::icons::ICON_HOME;
        use mirui::ui::widgets::{Icon, Image, ImageSource};

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Column () {
                    Image (ICON_HOME)
                    Icon (path: ICON_HOME)
                }
            }
        });
        let world = &app.world;

        let image_entity = world.query::<Image>().collect()[0];
        let icon_entity = world.query::<Icon>().collect()[0];
        let ImageSource::Vector(image_path) = &world.get::<Image>(image_entity).unwrap().src else {
            panic!("icon image source");
        };
        assert_eq!(
            image_path.commands().as_ptr(),
            ICON_HOME.path().commands().as_ptr()
        );
        assert_eq!(
            world
                .get::<Icon>(icon_entity)
                .unwrap()
                .path
                .commands()
                .as_ptr(),
            ICON_HOME.path().commands().as_ptr()
        );
    }

    #[test]
    fn row_widget_implies_row_direction() {
        use mirui::ui::Style;
        use mirui::ui::layout::FlexDirection;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let entity = app.compose(root, |cx| {
            ui! {
                Row (width: 100)
            }
        });
        let world = &app.world;

        assert_eq!(
            world.get::<Style>(entity).unwrap().layout.direction,
            FlexDirection::Row,
        );
    }

    #[test]
    fn column_widget_implies_column_direction() {
        use mirui::ui::Style;
        use mirui::ui::layout::FlexDirection;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let entity = app.compose(root, |cx| {
            ui! {
                Column (width: 100)
            }
        });
        let world = &app.world;

        assert_eq!(
            world.get::<Style>(entity).unwrap().layout.direction,
            FlexDirection::Column,
        );
    }

    #[test]
    fn gap_shorthand_routes_to_both_axes() {
        use mirui::types::Dimension;
        use mirui::ui::Style;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Column (gap: 12)
            }
        });
        let world = &app.world;

        let entity = world
            .query::<Style>()
            .collect()
            .into_iter()
            .find(|entity| *entity != root)
            .unwrap();
        let layout = world.get::<Style>(entity).unwrap().layout;
        assert_eq!(layout.row_gap, Dimension::px(12));
        assert_eq!(layout.column_gap, Dimension::px(12));
    }

    #[test]
    fn size_constraints_route_to_layout() {
        use mirui::types::Dimension;
        use mirui::ui::Style;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                View (
                    min_width: 120,
                    max_width: 320,
                    min_height: 80,
                    max_height: 240
                )
            }
        });
        let world = &app.world;

        let entity = world
            .query::<Style>()
            .collect()
            .into_iter()
            .find(|entity| *entity != root)
            .unwrap();
        let layout = world.get::<Style>(entity).unwrap().layout;
        assert_eq!(layout.min_width, Dimension::px(120));
        assert_eq!(layout.max_width, Dimension::px(320));
        assert_eq!(layout.min_height, Dimension::px(80));
        assert_eq!(layout.max_height, Dimension::px(240));
    }

    #[test]
    fn shrink_routes_to_layout() {
        use mirui::types::Fixed;
        use mirui::ui::Style;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                View (shrink: 0.5)
            }
        });
        let world = &app.world;

        let entity = world
            .query::<Style>()
            .collect()
            .into_iter()
            .find(|entity| *entity != root)
            .unwrap();
        let layout = world.get::<Style>(entity).unwrap().layout;
        assert_eq!(layout.shrink, Fixed::HALF);
    }

    #[test]
    fn flex_wrap_routes_to_layout() {
        use mirui::ui::Style;
        use mirui::ui::layout::FlexWrap;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Row (wrap: FlexWrap::Wrap)
            }
        });
        let world = &app.world;

        let entity = world
            .query::<Style>()
            .collect()
            .into_iter()
            .find(|entity| *entity != root)
            .unwrap();
        let layout = world.get::<Style>(entity).unwrap().layout;
        assert_eq!(layout.wrap, FlexWrap::Wrap);
    }

    #[test]
    fn lowercase_name_stays_layout_fallback() {
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                row (
                    direction: mirui::ui::layout::FlexDirection::Row,
                    width: 100
                )
            }
        });
        let world = &app.world;

        let buttons = world.query::<Button>().collect();
        assert!(
            buttons.is_empty(),
            "lowercase name should not implicitly insert any component"
        );
    }
}
