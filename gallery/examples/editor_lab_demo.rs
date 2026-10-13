fn main() {
    let (width, height) = mirui::gallery::demos::editor_lab::DEMO_SIZE
        .fixed_size()
        .expect("Editor Lab has a fixed canvas size");
    gallery::run("mirui — Editor Lab", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::editor_lab::setup_app(setup.app, parent);
        parent
    });
}
