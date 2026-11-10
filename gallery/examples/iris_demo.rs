fn main() {
    let (width, height) = mirui::gallery::demos::iris::VIEWPORT;
    gallery::run("mirui — Iris", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::iris::setup_app(setup.app, parent);
        parent
    });
}
