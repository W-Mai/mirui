fn main() {
    let (w, h) = mirui::gallery::demos::niche::DEFAULT_VIEW;
    gallery::run("mirui - niche", w, h, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::niche::setup_app(setup.app, parent);
        parent
    });
}
