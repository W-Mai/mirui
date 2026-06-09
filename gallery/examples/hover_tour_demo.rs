fn main() {
    gallery::run("mirui — hover tour", 720, 360, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::hover_tour::setup_app(setup.app, parent);
        parent
    });
}
