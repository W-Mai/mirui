fn main() {
    let (width, height) = mirui::gallery::demos::curve_text::VIEWPORT;
    gallery::run("mirui — Kinetic Type", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::curve_text::setup_app(setup.app, parent);
        parent
    });
}
