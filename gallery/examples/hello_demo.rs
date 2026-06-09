fn main() {
    gallery::run("mirui hello", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::hello::setup_app(setup.app, parent);
        parent
    });
}
