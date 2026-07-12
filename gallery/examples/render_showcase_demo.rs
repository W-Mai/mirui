fn main() {
    gallery::run("mirui - render showcase", 640, 700, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::render_showcase::setup_app(setup.app, parent);
        parent
    });
}
