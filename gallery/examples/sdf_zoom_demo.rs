fn main() {
    gallery::run("mirui - sdf zoom demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::sdf_zoom::setup_app(setup.app, parent);
        parent
    });
}
