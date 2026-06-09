fn main() {
    gallery::run("mirui - butterfly", 480, 480, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::butterfly::setup_app(setup.app, parent);
        parent
    });
}
