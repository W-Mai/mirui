fn main() {
    gallery::run("mirui - rounded + border", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::rounded::setup_app(setup.app, parent);
        parent
    });
}
