fn main() {
    gallery::run("mirui - reactive if/match", 320, 240, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::state_show::setup_app(setup.app, parent);
        parent
    });
}
