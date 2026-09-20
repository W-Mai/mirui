fn main() {
    let (width, height) = mirui::gallery::demos::moss_study::VIEWPORT;
    gallery::run("mirui — Moss Study", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::moss_study::setup_app(setup.app, parent);
        parent
    });
}
