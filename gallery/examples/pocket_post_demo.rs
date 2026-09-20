fn main() {
    let (width, height) = mirui::gallery::demos::pocket_post::VIEWPORT;
    gallery::run("mirui — Pocket Post", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::pocket_post::setup_app(setup.app, parent);
        parent
    });
}
