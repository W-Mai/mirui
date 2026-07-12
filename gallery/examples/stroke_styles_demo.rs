fn main() {
    gallery::run("mirui - stroke styles", 480, 360, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::stroke_styles::setup_app(setup.app, parent);
        parent
    });
}
