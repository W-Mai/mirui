fn main() {
    let autoplay = std::env::args().any(|arg| arg == "--auto");
    let (width, height) = mirui_pixel_flight::VIEWPORT;
    gallery::run("mirui — Pixel Flight", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        let model = mirui_pixel_flight::setup_app(setup.app, parent);
        model.set_autoplay(autoplay);
        parent
    });
}
