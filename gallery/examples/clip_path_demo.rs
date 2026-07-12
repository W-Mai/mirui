fn main() {
    gallery::run("mirui - clip path", 320, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::clip_path::setup_app(setup.app, parent);
        parent
    });
}
