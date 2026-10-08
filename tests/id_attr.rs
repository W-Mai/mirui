#[cfg(test)]
mod tests {
    use mirui::ui;

    #[test]
    fn id_attr_registers_in_idmap() {
        let mut app = mirui::app::App::headless(64, 64);

        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                slot (id: "src")
            }
        });
        let world = &app.world;

        let found = world.find_by_id("src").expect("id 'src' must register");
        assert_ne!(found, root);
    }

    #[test]
    fn id_attr_attaches_named_id_marker() {
        use mirui::ui::NamedId;

        let mut app = mirui::app::App::headless(64, 64);

        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                slot (id: "hero")
            }
        });
        let world = &app.world;

        let entity = world.find_by_id("hero").expect("registered");
        let marker = world.get::<NamedId>(entity).expect("NamedId attached");
        assert_eq!(marker.0, "hero");
    }

    #[test]
    fn id_lookup_supports_bare_ident() {
        struct LinkTo(mirui::ecs::Entity);

        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                container (direction: mirui::ui::layout::FlexDirection::Row) {
                    src (id: "anchor")
                    consumer () [
                        LinkTo(id(anchor)),
                    ]
                }
            }
        });
        let world = &app.world;

        let anchor = world.find_by_id("anchor").expect("anchor registered");
        let entities = world.query::<LinkTo>().collect();
        let consumer = entities.first().copied().expect("consumer with LinkTo");
        assert_eq!(world.get::<LinkTo>(consumer).unwrap().0, anchor);
    }

    #[test]
    fn id_lookup_in_enchant_resolves_to_entity() {
        struct LinkTo(mirui::ecs::Entity);

        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                container (direction: mirui::ui::layout::FlexDirection::Row) {
                    src (id: "anchor")
                    consumer () [
                        LinkTo(id("anchor")),
                    ]
                }
            }
        });
        let world = &app.world;

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
    #[should_panic(expected = "widget id `shared` is already bound to another entity")]
    fn duplicate_id_attributes_are_rejected() {
        let mut app = mirui::app::App::headless(64, 64);

        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                row (direction: mirui::ui::layout::FlexDirection::Row) {
                    first (id: "shared")
                    second (id: "shared")
                }
            }
        });
    }
}
