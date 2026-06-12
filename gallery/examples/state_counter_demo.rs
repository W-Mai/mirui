fn main() {
    gallery::run("mirui - reactive counter", 360, 240, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::state_counter::setup_app(setup.app, parent);
        parent
    });
}
