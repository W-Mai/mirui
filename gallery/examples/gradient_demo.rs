fn main() {
    gallery::run("mirui - gradient paint", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::gradient::setup_app(setup.app, parent);
        parent
    });
}
