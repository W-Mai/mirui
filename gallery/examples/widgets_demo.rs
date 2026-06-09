fn main() {
    let (w, h) = mirui::gallery::demos::widgets::DEFAULT_VIEW;
    gallery::run("mirui - widgets", w, h, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::widgets::setup_app(setup.app, parent);
        parent
    });
}
