fn main() {
    gallery::run("mirui - disabled demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::disabled::setup_app(setup.app, parent);
        parent
    });
}
