#[cfg(test)]
mod tests {
    use mirui::ui;
    use mirui::ui::widgets::Text;

    enum LoadState {
        Loading,
        Ready(&'static str),
        Error,
    }

    #[test]
    fn match_picks_one_arm() {
        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();

        let state = LoadState::Ready("hello");

        app.compose(root, |cx| {
            ui! {
                match state {
                    LoadState :: Loading => {
                        Text ("loading...")
                    }
                    LoadState :: Ready (s) => {
                        Text (s)
                    }
                    LoadState :: Error => {
                        Text ("error")
                    }
                }
            }
        });
        let world = &app.world;

        let texts = world.query::<Text>().collect();
        assert_eq!(texts.len(), 1);
        let text = world.get::<Text>(texts[0]).unwrap();
        assert_eq!(text.resolve(world.resource()), "hello");
    }

    #[test]
    fn match_with_no_match_emits_nothing() {
        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();

        let state = LoadState::Loading;

        app.compose(root, |cx| {
            ui! {
                match state {
                    LoadState :: Ready (_) => {
                        Text ("ready")
                    }
                    _ => {
                    }
                }
            }
        });
        let world = &app.world;

        let texts = world.query::<Text>().collect();
        assert!(texts.is_empty());
    }

    #[test]
    fn match_picks_error_arm() {
        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();

        let state = LoadState::Error;

        app.compose(root, |cx| {
            ui! {
                match state {
                    LoadState :: Loading => {
                        Text ("loading...")
                    }
                    LoadState :: Ready (s) => {
                        Text (s)
                    }
                    LoadState :: Error => {
                        Text ("error")
                    }
                }
            }
        });
        let world = &app.world;

        let texts = world.query::<Text>().collect();
        assert_eq!(texts.len(), 1);
        let text = world.get::<Text>(texts[0]).unwrap();
        assert_eq!(text.resolve(world.resource()), "error");
    }
}
