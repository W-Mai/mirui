#[cfg(test)]
mod tests {
    use mirui::ecs::{Entity, World};
    use mirui::mold;
    use mirui::ui;
    use mirui::ui::IdMap;
    use mirui::ui::NicheMap;
    use mirui::ui::Parent;
    use mirui::ui::ViewRegistry;
    use mirui::ui::builder::WidgetBuilder;

    mold!(Card {
        @@header @@body @@footer
    });

    #[test]
    fn niche_resolves_to_anchor() {
        use mirui::ui::widgets::Text;

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::default();
        reg.insert(mold!(Card));
        world.insert_resource(reg);

        let root = WidgetBuilder::new(&mut world).id();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Card () {
                @body { Text("body content") {} }
                @footer { Text("footer text") {} }
            }
        };

        let cards = world.query::<Card>().collect();
        assert_eq!(cards.len(), 1);
        let card = cards[0];

        let map = world.get::<NicheMap>(card).expect("NicheMap registered");
        let body = map.get("body").unwrap();
        let footer = map.get("footer").unwrap();
        let header = map.get("header").unwrap();

        let text_entities = world.query::<Text>().collect();
        assert_eq!(text_entities.len(), 2);

        for &t in &text_entities {
            let parent = world.get::<Parent>(t).unwrap().0;
            let text = world.get::<Text>(t).unwrap();
            let bytes = text.resolve(&world);
            if &*bytes == "body content" {
                assert_eq!(parent, body);
            } else if &*bytes == "footer text" {
                assert_eq!(parent, footer);
            } else {
                panic!("unexpected text: {:?}", bytes);
            }
        }

        let header_children: Vec<_> = world
            .query::<Parent>()
            .collect()
            .into_iter()
            .filter(|&e| world.get::<Parent>(e).unwrap().0 == header)
            .collect();
        assert!(
            header_children.is_empty(),
            "header niche should be empty since @header was not used"
        );
    }
}
