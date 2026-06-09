fn main() {
    gallery::run("mirui - text demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::text::setup_app(setup.app, parent);
        parent
    });
}
