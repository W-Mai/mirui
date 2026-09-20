fn main() {
    let (width, height) = mirui::gallery::demos::lumen_lab::VIEWPORT;
    gallery::run("mirui — Lumen Lab", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::lumen_lab::setup_app(setup.app, parent);
        parent
    });
}
