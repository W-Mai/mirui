fn main() {
    gallery::run("mirui - transform demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::transform::setup_app(setup.app, parent);
        parent
    });
}
