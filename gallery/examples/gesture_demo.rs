fn main() {
    gallery::run("mirui - gesture demo", 320, 240, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::gesture::setup_app(setup.app, parent);
        parent
    });
}
