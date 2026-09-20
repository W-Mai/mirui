fn main() {
    let (width, height) = mirui::gallery::demos::orbital_mission::VIEWPORT;
    gallery::run("mirui — Orbital Mission", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::orbital_mission::setup_app(setup.app, parent);
        parent
    });
}
