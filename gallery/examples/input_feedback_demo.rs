fn main() {
    gallery::run("mirui — input feedback", 640, 360, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::input_feedback::setup_app(setup.app, parent);
        parent
    });
}
