fn main() {
    gallery::run("mirui - click demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::click::setup_app(setup.app, parent);
        parent
    });
}
