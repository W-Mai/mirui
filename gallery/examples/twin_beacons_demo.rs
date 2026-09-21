fn main() {
    let (width, height) = mirui::gallery::demos::twin_beacons::VIEWPORT;
    gallery::run("mirui — Twin Beacons", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::twin_beacons::setup_app(setup.app, parent);
        parent
    });
}
