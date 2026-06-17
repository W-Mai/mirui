#[cfg(test)]
mod tests {
    use mirui::ecs::{Entity, World};
    use mirui::ui;
    use mirui::ui::IdMap;
    use mirui::ui::NicheMap;
    use mirui::ui::Parent;
    use mirui::ui::View;
    use mirui::ui::ViewRegistry;
    use mirui::ui::builder::WidgetBuilder;

    #[derive(Default)]
    pub struct Card;

    fn card_attach(world: &mut World, entity: Entity) {
        if world.get::<NicheMap>(entity).is_some() {
            return;
        }
        let header = world.spawn_empty();
        world.insert(header, Parent(entity));
        let body = world.spawn_empty();
        world.insert(body, Parent(entity));
        let footer = world.spawn_empty();
        world.insert(footer, Parent(entity));
        world.insert(
            entity,
            NicheMap::from([("header", header), ("body", body), ("footer", footer)]),
        );
    }

    fn no_op_render(
        _: &mut dyn mirui::render::renderer::Renderer,
        _: &World,
        _: Entity,
        _: &mirui::types::Rect,
        _: &mut mirui::ui::view::ViewCtx,
    ) {
    }

    fn card_view() -> View {
        View::new("Card", 60, no_op_render).with_attach(card_attach)
    }

    #[test]
    fn niche_resolves_to_anchor() {
        use mirui::ui::widgets::Text;

        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::default();
        reg.insert(card_view());
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
            if text.0 == b"body content" {
                assert_eq!(parent, body);
            } else if text.0 == b"footer text" {
                assert_eq!(parent, footer);
            } else {
                panic!("unexpected text: {:?}", text.0);
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
