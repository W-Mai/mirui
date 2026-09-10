fn main() {
    let (width, height) = mirui::gallery::demos::layout_lab::VIEWPORT;
    gallery::run("mirui — Layout Lab", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::layout_lab::setup_app(setup.app, parent);
        parent
    });
}
