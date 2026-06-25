fn main() {
    gallery::run("mirui - icon demo", 560, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::icon::setup_app(setup.app, parent);
        parent
    });
}
