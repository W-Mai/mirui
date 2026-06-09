fn main() {
    gallery::run("mirui - absolute position demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::absolute::setup_app(setup.app, parent);
        parent
    });
}
