fn main() {
    let (width, height) = mirui::gallery::demos::tidal_atlas::VIEWPORT;
    gallery::run("mirui — Tidal Atlas", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::tidal_atlas::setup_app(setup.app, parent);
        parent
    });
}
