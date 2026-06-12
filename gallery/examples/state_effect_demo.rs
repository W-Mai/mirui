fn main() {
    gallery::run("mirui - reactive effect", 360, 260, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::state_effect::setup_app(setup.app, parent);
        parent
    });
}
