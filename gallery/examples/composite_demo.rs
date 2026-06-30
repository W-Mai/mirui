fn main() {
    gallery::run("mirui - composite demo", 720, 240, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::composite::setup_app(setup.app, parent);
        parent
    });
}
