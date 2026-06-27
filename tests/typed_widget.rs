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
        assert_eq!(&*text.bytes(&world), b"Hello");
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
        assert_eq!(&*text.bytes(&world), b"Positional");
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
