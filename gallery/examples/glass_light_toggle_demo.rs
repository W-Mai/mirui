fn main() {
    let (width, height) = mirui::gallery::demos::glass_light_toggle::VIEWPORT;
    gallery::run("mirui — Glass Light Toggle", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::glass_light_toggle::setup_app(setup.app, parent);
        parent
    });
}
