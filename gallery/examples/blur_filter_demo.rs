fn main() {
    gallery::run("mirui - blur filter", 480, 240, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::blur_filter::setup_app(setup.app, parent);
        parent
    });
}
