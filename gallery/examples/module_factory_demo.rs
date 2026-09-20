fn main() {
    let (width, height) = mirui::gallery::demos::module_factory::VIEWPORT;
    gallery::run("mirui — Module Factory", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::module_factory::setup_app(setup.app, parent);
        parent
    });
}
