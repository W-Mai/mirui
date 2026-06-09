fn main() {
    gallery::run("mirui - components demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::components::setup_app(setup.app, parent);
        parent
    });
}
