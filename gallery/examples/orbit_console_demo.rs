fn main() {
    let (width, height) = mirui::gallery::demos::orbit_console::VIEWPORT;
    gallery::run("mirui — Orbit Console", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::orbit_console::setup_app(setup.app, parent);
        parent
    });
}
