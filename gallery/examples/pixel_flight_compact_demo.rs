fn main() {
    let autoplay = std::env::args().any(|arg| arg == "--auto");
    let (width, height) = mirui_pixel_flight::COMPACT_VIEWPORT;
    gallery::run("mirui — Pixel Flight Compact", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        let model = mirui_pixel_flight::setup_compact_app(setup.app, parent);
        model.set_autoplay(autoplay);
        parent
    });
}
