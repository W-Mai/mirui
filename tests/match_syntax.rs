#[cfg(test)]
mod tests {
    use mirui::components::Text;
    use mirui::ecs::World;
    use mirui::ui;
    use mirui::widget::IdMap;
    use mirui::widget::builder::WidgetBuilder;

    enum LoadState {
        Loading,
        Ready(&'static str),
        Error,
    }

    #[test]
    fn match_picks_one_arm() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        let state = LoadState::Ready("hello");

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            match state {
                LoadState::Loading => {
                    Text("loading...") {}
                }
                LoadState::Ready(s) => {
                    Text(s) {}
                }
                LoadState::Error => {
                    Text("error") {}
                }
            }
        };

        let texts = world.query::<Text>().collect();
        assert_eq!(texts.len(), 1);
        let text = world.get::<Text>(texts[0]).unwrap();
        assert_eq!(text.0, b"hello");
    }

    #[test]
    fn match_with_no_match_emits_nothing() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        let state = LoadState::Loading;

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            match state {
                LoadState::Ready(_) => {
                    Text("ready") {}
                }
                _ => {}
            }
        };

        let texts = world.query::<Text>().collect();
        assert!(texts.is_empty());
    }
}
