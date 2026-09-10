fn main() {
    let (width, height) = mirui::gallery::demos::interaction_lab::VIEWPORT;
    gallery::run("mirui — Interaction Lab", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::interaction_lab::setup_app(setup.app, parent);
        parent
    });
}
