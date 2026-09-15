fn main() {
    let (width, height) = mirui::gallery::demos::kinetic_console::VIEWPORT;
    gallery::run("mirui — Kinetic Console", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::kinetic_console::setup_app(setup.app, parent);
        parent
    });
}
