fn main() {
    gallery::run("mirui - particles", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::particles::setup_app(setup.app, parent);
        parent
    });
}
