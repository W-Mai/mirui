fn main() {
    gallery::run("mirui - on handlers", 640, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::on_handlers::setup_app(setup.app, parent);
        parent
    });
}
