fn main() {
    gallery::run("mirui — theme swap", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::theme_swap::setup_app(setup.app, parent);
        parent
    });
}
