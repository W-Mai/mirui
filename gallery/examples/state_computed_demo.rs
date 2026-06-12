fn main() {
    gallery::run("mirui - reactive computed", 360, 240, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::state_computed::setup_app(setup.app, parent);
        parent
    });
}
