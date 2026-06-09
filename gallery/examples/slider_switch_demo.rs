fn main() {
    gallery::run("mirui - slider & switch", 320, 200, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::slider_switch::setup_app(setup.app, parent);
        parent
    });
}
