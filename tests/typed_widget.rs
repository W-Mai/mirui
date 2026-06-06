#[cfg(test)]
mod tests {
    use mirui::components::{Button, Checkbox};
    use mirui::ecs::World;
    use mirui::ui;
    use mirui::widget::IdMap;
    use mirui::widget::builder::WidgetBuilder;

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
                normal_color: mirui::widget::ColorToken::Surface,
            ) {}
        };

        let entities = world.query::<Button>().collect();
        assert_eq!(entities.len(), 1, "exactly one Button entity");
        let btn = world.get::<Button>(entities[0]).unwrap();
        assert!(matches!(
            btn.normal_color,
            mirui::widget::ThemedColor::Token(mirui::widget::ColorToken::Surface)
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
        use mirui::components::Text;

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
        assert_eq!(text.0, b"Hello");
    }

    #[test]
    fn text_input_text_color_routes_to_field() {
        use mirui::components::TextInput;

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            TextInput (
                text_color: mirui::widget::ColorToken::Primary,
            ) {}
        };

        let entities = world.query::<TextInput>().collect();
        assert_eq!(entities.len(), 1);
        let ti = world.get::<TextInput>(entities[0]).unwrap();
        assert!(matches!(
            ti.text_color,
            mirui::widget::ThemedColor::Token(mirui::widget::ColorToken::Primary)
        ));
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
                direction: mirui::layout::FlexDirection::Row,
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
