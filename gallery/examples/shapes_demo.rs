fn main() {
    gallery::run("mirui - shapes (clock)", 480, 480, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::shapes::setup_app(setup.app, parent);
        parent
    });
}
