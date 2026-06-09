fn main() {
    gallery::run("mirui - slider business events", 640, 200, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::slider_value_changed::setup_app(setup.app, parent);
        parent
    });
}
