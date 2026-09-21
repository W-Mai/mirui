fn main() {
    let (width, height) = mirui::gallery::demos::atlas_restoration::VIEWPORT;
    gallery::run("mirui — Atlas Restoration", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::atlas_restoration::setup_app(setup.app, parent);
        parent
    });
}
