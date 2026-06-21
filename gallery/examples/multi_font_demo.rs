fn main() {
    gallery::run("mirui - multi-font bundle demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::multi_font::setup_app(setup.app, parent);
        parent
    });
}
