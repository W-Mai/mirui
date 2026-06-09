fn main() {
    gallery::run("mirui - image demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::image::setup_app(setup.app, parent);
        parent
    });
}
