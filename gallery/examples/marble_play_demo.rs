fn main() {
    gallery::run("mirui — Marble Play", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::marble_play::setup_app(setup.app, parent);
        parent
    });
}
