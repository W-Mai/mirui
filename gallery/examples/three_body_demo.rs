fn main() {
    gallery::run("mirui - three body", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::three_body::setup_app(setup.app, parent);
        parent
    });
}
