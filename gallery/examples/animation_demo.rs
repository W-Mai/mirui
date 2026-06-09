fn main() {
    gallery::run("mirui - animation demo", 320, 180, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::animation::setup_app(setup.app, parent);
        parent
    });
}
