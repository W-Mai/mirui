fn main() {
    gallery::run("mirui — interactive states", 720, 420, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::interactive_states::setup_app(setup.app, parent);
        parent
    });
}
