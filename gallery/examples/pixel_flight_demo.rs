fn main() {
    let (width, height) = mirui::gallery::demos::pixel_flight::VIEWPORT;
    gallery::run("mirui — Pixel Flight", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::pixel_flight::setup_app(setup.app, parent);
        parent
    });
}
