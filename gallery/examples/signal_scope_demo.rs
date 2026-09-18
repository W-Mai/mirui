fn main() {
    let (width, height) = mirui::gallery::demos::signal_scope::VIEWPORT;
    gallery::run("mirui — Signal Scope", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::signal_scope::setup_app(setup.app, parent);
        parent
    });
}
