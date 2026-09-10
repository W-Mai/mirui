fn main() {
    let (width, height) = mirui::gallery::demos::typography_lab::VIEWPORT;
    gallery::run("mirui — Typography Lab", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::typography_lab::setup_app(setup.app, parent);
        parent
    });
}
