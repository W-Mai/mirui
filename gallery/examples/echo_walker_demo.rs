fn main() {
    let (width, height) = mirui::gallery::demos::echo_walker::VIEWPORT;
    gallery::run("mirui — Echo Walker", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::echo_walker::setup_app(setup.app, parent);
        parent
    });
}
