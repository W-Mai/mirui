fn main() {
    gallery::run("mirui - app demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::app_demo::setup_app(setup.app, parent);
        parent
    });
}
