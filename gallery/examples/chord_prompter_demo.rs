fn main() {
    let (width, height) = mirui::gallery::demos::chord_prompter::VIEWPORT;
    gallery::run("mirui — Chord Prompter", width, height, |setup| {
        let root = setup.app.spawn_root().id();
        mirui::gallery::demos::chord_prompter::setup_app(setup.app, root);
        root
    });
}
