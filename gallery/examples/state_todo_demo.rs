fn main() {
    gallery::run("mirui - reactive todo", 320, 280, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::state_todo::setup_app(setup.app, parent);
        parent
    });
}
