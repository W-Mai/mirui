fn main() {
    gallery::run("mirui - walk demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::walk::setup_app(setup.app, parent);
        parent
    });
}
