#[cfg(test)]
mod tests {
    use mirui::ecs::World;
    use mirui::ui;
    use mirui::widget::IdMap;
    use mirui::widget::builder::WidgetBuilder;

    #[test]
    fn id_attr_registers_in_idmap() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());

        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            slot (id: "src") {}
        };

        let found = world.find_by_id("src").expect("id 'src' must register");
        assert_ne!(found, root);
    }

    #[test]
    fn id_attr_attaches_named_id_marker() {
        use mirui::widget::NamedId;

        let mut world = World::new();
        world.insert_resource(IdMap::new());

        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            slot (id: "hero") {}
        };

        let entity = world.find_by_id("hero").expect("registered");
        let marker = world.get::<NamedId>(entity).expect("NamedId attached");
        assert_eq!(marker.0, "hero");
    }

    #[test]
    fn id_lookup_in_enchant_resolves_to_entity() {
        struct LinkTo(mirui::ecs::Entity);

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            container (direction: mirui::layout::FlexDirection::Row) {
                src (id: "anchor") {}
                consumer () [LinkTo(id("anchor"))] {}
            }
        };

        let anchor = world.find_by_id("anchor").expect("anchor registered");

        let entities = world.query::<LinkTo>().collect();
        let consumer = entities
            .first()
            .copied()
            .expect("at least one consumer with LinkTo");
        let link = world.get::<LinkTo>(consumer).unwrap();
        assert_eq!(link.0, anchor);
    }

    #[test]
    fn id_attr_last_write_wins() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());

        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            row (direction: mirui::layout::FlexDirection::Row) {
                first (id: "shared") {}
                second (id: "shared") {}
            }
        };

        let found = world.find_by_id("shared").unwrap();
        let map = world.resource::<IdMap>().unwrap();
        assert_eq!(map.get("shared"), Some(found));
    }
}
