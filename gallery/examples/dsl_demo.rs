fn main() {
    gallery::run("mirui - DSL demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::dsl::setup_app(setup.app, parent);
        parent
    });
}
