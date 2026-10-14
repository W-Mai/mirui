fn main() {
    gallery::run("mirui — Nearby", 640, 480, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::nearby::setup_app(setup.app, parent);
        parent
    });
}
