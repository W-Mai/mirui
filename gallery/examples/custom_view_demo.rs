fn main() {
    gallery::run("custom_view_demo", 480, 200, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::custom_view::setup_app(setup.app, parent);
        parent
    });
}
