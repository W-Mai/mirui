fn main() {
    gallery::run("TextInput Demo", 480, 200, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::text_input::setup_app(setup.app, parent);
        parent
    });
}
