fn main() {
    let (width, height) = mirui::gallery::demos::life_compact::VIEWPORT;
    gallery::run("mirui - life compact", width, height, |setup| {
        let root = setup.app.spawn_root().id();
        mirui::gallery::demos::life_compact::setup_app(setup.app, root);
        root
    });
}
