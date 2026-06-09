fn main() {
    gallery::run("mirui - book flip (transform-origin)", 640, 360, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::book_flip::setup_app(setup.app, parent);
        parent
    });
}
