fn main() {
    gallery::run("mirui - toggle business events", 480, 240, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::toggle::setup_app(setup.app, parent);
        parent
    });
}
