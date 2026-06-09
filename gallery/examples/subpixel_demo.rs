fn main() {
    gallery::run("mirui - subpixel motion", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::subpixel::setup_app(setup.app, parent);
        parent
    });
}
