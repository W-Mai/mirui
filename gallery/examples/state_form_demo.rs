fn main() {
    gallery::run("mirui - reactive form", 360, 280, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::state_form::setup_app(setup.app, parent);
        parent
    });
}
