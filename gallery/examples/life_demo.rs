fn main() {
    gallery::run("mirui - game of life", 640, 640, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::life::setup_app(setup.app, parent);
        parent
    });
}
