#[cfg(test)]
mod tests {
    use mirui::ecs::World;
    use mirui::ui;
    use mirui::ui::IdMap;
    use mirui::ui::builder::WidgetBuilder;
    use mirui::ui::widgets::Text;

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
        assert_eq!(&*text.bytes(&world), b"hello");
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

    #[test]
    fn match_picks_error_arm() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let root = WidgetBuilder::new(&mut world).id();

        let state = LoadState::Error;

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
        assert_eq!(&*text.bytes(&world), b"error");
    }
}
