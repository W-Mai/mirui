fn main() {
    let (w, h) = mirui::gallery::demos::pinch_rotate::DEFAULT_VIEW;
    gallery::run("mirui — pinch / rotate demo", w, h, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::pinch_rotate::setup_app(setup.app, parent);
        parent
    });
}
