#[cfg(test)]
mod tests {
    use mirui::ui;
    use mirui::ui::NicheMap;
    use mirui::ui::Parent;

    ui!(compose Card {
        @@header @@body @@footer
    });

    #[test]
    fn niche_resolves_to_anchor() {
        use mirui::ui::widgets::Text;

        let mut app = mirui::app::App::headless(64, 64);
        app.with_widget(ui!(compose Card));

        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                Card () {
                    @body {
                        Text ("body content")
                    }
                    @footer {
                        Text ("footer text")
                    }
                }
            }
        });
        let world = &app.world;

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
            let bytes = text.resolve(world);
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
