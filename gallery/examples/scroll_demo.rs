fn main() {
    gallery::run("mirui - scroll demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::scroll::setup_app(setup.app, parent);
        parent
    });
}
