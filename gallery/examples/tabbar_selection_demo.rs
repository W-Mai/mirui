fn main() {
    gallery::run("mirui - tabbar selection events", 480, 200, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::tabbar_selection::setup_app(setup.app, parent);
        parent
    });
}
