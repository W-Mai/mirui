fn main() {
    let (width, height) = mirui::gallery::demos::folding_ark::VIEWPORT;
    gallery::run("mirui — Folding Ark", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::folding_ark::setup_app(setup.app, parent);
        parent
    });
}
