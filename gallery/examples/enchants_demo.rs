fn main() {
    gallery::run("mirui - enchants demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::enchants::setup_app(setup.app, parent);
        parent
    });
}
