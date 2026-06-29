fn main() {
    gallery::run("mirui - persistence counter", 320, 240, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::persistence_counter::setup_app(setup.app, parent);
        parent
    });
}
