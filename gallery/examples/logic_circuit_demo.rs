fn main() {
    let (width, height) = mirui::gallery::demos::logic_circuit::VIEWPORT;
    gallery::run("mirui — Logic Circuit", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::logic_circuit::setup_app(setup.app, parent);
        parent
    });
}
