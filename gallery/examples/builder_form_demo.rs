fn main() {
    gallery::run("mirui - builder API (no DSL)", 320, 200, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::builder_form::setup_app(setup.app, parent);
        parent
    });
}
