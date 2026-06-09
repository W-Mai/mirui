fn main() {
    gallery::run("TabBar Demo", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::tabbar::setup_app(setup.app, parent);
        parent
    });
}
