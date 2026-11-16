fn main() {
    let autoplay = std::env::args().any(|arg| arg == "--auto");
    let (width, height) = mirui::gallery::demos::pixel_flight_compact::VIEWPORT;
    gallery::run("mirui — Pixel Flight Compact", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        let model = mirui::gallery::demos::pixel_flight_compact::setup_app(setup.app, parent);
        model.set_autoplay(autoplay);
        parent
    });
}
