fn main() {
    gallery::run("mirui - nested scroll", 480, 400, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::nested_scroll::setup_app(setup.app, parent);
        parent
    });
}
