fn main() {
    gallery::run("mirui - reactive walk list", 320, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::state_list::setup_app(setup.app, parent);
        parent
    });
}
