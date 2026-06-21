fn main() {
    gallery::run("mirui - atlas font demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::atlas_font::setup_app(setup.app, parent);
        parent
    });
}
