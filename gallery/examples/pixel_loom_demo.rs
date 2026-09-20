fn main() {
    let (width, height) = mirui::gallery::demos::pixel_loom::VIEWPORT;
    gallery::run("mirui — Pixel Loom", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::pixel_loom::setup_app(setup.app, parent);
        parent
    });
}
