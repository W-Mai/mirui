#[cfg(test)]
mod tests {
    use mirui::ecs::World;
    use mirui::ui;
    use mirui::ui::IdMap;
    use mirui::ui::builder::WidgetBuilder;
    use mirui::ui::widgets::{Button, Checkbox};

    #[test]
    fn capital_name_inserts_component() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Button (
                normal_color: mirui::ui::ColorToken::Surface,
            ) {}
        };

        let entities = world.query::<Button>().collect();
        assert_eq!(entities.len(), 1, "exactly one Button entity");
        let btn = world.get::<Button>(entities[0]).unwrap();
        assert!(matches!(
            btn.normal_color,
            mirui::ui::ThemedColor::Token(mirui::ui::ColorToken::Surface)
        ));
    }

    #[test]
    fn capital_name_uses_default_for_omitted_fields() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Checkbox () {}
        };

        let entities = world.query::<Checkbox>().collect();
        assert_eq!(entities.len(), 1);
        let cb = world.get::<Checkbox>(entities[0]).unwrap();
        assert_eq!(cb.checked, false);
    }

    #[test]
    fn text_widget_uses_tuple_init() {
        use mirui::ui::widgets::Text;

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Text (text: "Hello") {}
        };

        let entities = world.query::<Text>().collect();
        assert_eq!(entities.len(), 1);
        let text = world.get::<Text>(entities[0]).unwrap();
        assert_eq!(text.resolve(&world), "Hello");
    }

    #[test]
    fn text_widget_routes_font_size_to_style() {
        use mirui::ui::Style;
        use mirui::ui::widgets::Text;

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Text (
                "Hello",
                width: mirui::types::Dimension::Content,
                height: mirui::types::Dimension::Auto,
                font_size: 18
            ) {}
        };

        let entity = world.query::<Text>().collect()[0];
        let style = world.get::<Style>(entity).unwrap();
        assert_eq!(style.font_size, Some(18));
        assert_eq!(style.layout.width, mirui::types::Dimension::Content);
        assert_eq!(style.layout.height, mirui::types::Dimension::Auto);
    }

    #[test]
    fn text_input_text_color_routes_to_field() {
        use mirui::ui::widgets::TextInput;

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            TextInput (
                text_color: mirui::ui::ColorToken::Primary,
            ) {}
        };

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

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Text("Positional") {}
        };

        let entities = world.query::<Text>().collect();
        assert_eq!(entities.len(), 1);
        let text = world.get::<Text>(entities[0]).unwrap();
        assert_eq!(text.resolve(&world), "Positional");
    }

    #[test]
    fn row_widget_implies_row_direction() {
        use mirui::ui::Style;
        use mirui::ui::layout::FlexDirection;

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Row (width: 100) {}
        };

        let entities = world.query::<Style>().collect();
        let row_count = entities
            .iter()
            .filter(|&&e| {
                world
                    .get::<Style>(e)
                    .map(|s| matches!(s.layout.direction, FlexDirection::Row))
                    .unwrap_or(false)
            })
            .count();
        assert!(
            row_count >= 1,
            "Row widget must default to FlexDirection::Row"
        );
    }

    #[test]
    fn column_widget_implies_column_direction() {
        use mirui::ui::Style;
        use mirui::ui::layout::FlexDirection;

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Column (width: 100) {}
        };

        let entities = world.query::<Style>().collect();
        let col_count = entities
            .iter()
            .filter(|&&e| {
                world
                    .get::<Style>(e)
                    .map(|s| matches!(s.layout.direction, FlexDirection::Column))
                    .unwrap_or(false)
            })
            .count();
        assert!(
            col_count >= 1,
            "Column widget must default to FlexDirection::Column"
        );
    }

    #[test]
    fn gap_shorthand_routes_to_both_axes() {
        use mirui::types::Dimension;
        use mirui::ui::Style;

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Column (gap: 12) {}
        };

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

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View (
                min_width: 120,
                max_width: 320,
                min_height: 80,
                max_height: 240
            ) {}
        };

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

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View (shrink: 0.5) {}
        };

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
    fn lowercase_name_stays_layout_fallback() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            row (
                direction: mirui::ui::layout::FlexDirection::Row,
                width: 100
            ) {}
        };

        let buttons = world.query::<Button>().collect();
        assert!(
            buttons.is_empty(),
            "lowercase name should not implicitly insert any component"
        );
    }
}
